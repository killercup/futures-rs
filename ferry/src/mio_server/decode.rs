use std::io::{self, Read};

use futures::{Poll, Task};
use futures::stream::Stream;
use futuremio::*;

pub trait Decode: Sized + Send + 'static {
    type Decoder: Default + Send + 'static;
    type Error: Send + 'static + From<io::Error>;

    fn decode(decoder: &mut Self::Decoder, buf: &mut InputBuf) -> Poll<Self, Self::Error>;
}

/// A stream for parsing from an underlying reader, using an unbounded internal
/// buffer.
pub struct DecodeStream<R, D: Decode> {
    buf: BufReader<R>,
    decoder: D::Decoder,

    /// Is there new data in `buf` that we need to try to parse?
    need_parse: bool,
}

impl<R, D> DecodeStream<R, D>
    where R: Read + Send + 'static,
          D: Decode
{
    pub fn new(source: R, source_ready: ReadinessStream) -> DecodeStream<R, D> {
        DecodeStream {
            buf: BufReader::new(source, source_ready),
            decoder: Default::default(),
            need_parse: false,
        }
    }
}

impl<R, D> Stream for DecodeStream<R, D>
    where R: Read + Send + 'static,
          D: Decode
{
    type Item = D;
    type Error = D::Error;

    fn poll(&mut self, task: &mut Task) -> Poll<Option<D>, D::Error> {
        let mut task = task.scoped();
        loop {
            if self.need_parse {
                debug!("attempting to decode");
                match D::decode(&mut self.decoder, self.buf.buf()) {
                    Poll::Ok(item) => {
                        debug!("got an item");
                        return Poll::Ok(Some(item))
                    }
                    Poll::Err(e) => return Poll::Err(e),
                    Poll::NotReady => {
                        self.need_parse = false;
                    }
                }
            }

            match self.buf.poll(&mut task) {
                Poll::Ok(Some(())) => self.need_parse = true,
                Poll::Ok(None) => return Poll::Ok(None),
                Poll::Err(e) => return Poll::Err(e.into()),
                Poll::NotReady => return Poll::NotReady,
            }

            task.ready();
        }
    }

    fn schedule(&mut self, task: &mut Task) {
        if self.need_parse {
            // No tokens because in a `need_parse` situation, we'll attempt
            // to parse regardless of tokens
            task.notify();
        } else {
            self.buf.schedule(task)
        }
    }
}
