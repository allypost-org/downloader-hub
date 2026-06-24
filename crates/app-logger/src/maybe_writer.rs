use std::io::{self, Write};

use tracing_appender::non_blocking::NonBlocking;

#[derive(Debug, Clone)]
pub struct MaybeFileWriter(Option<NonBlocking>);
impl MaybeFileWriter {
    pub const fn new(writer: Option<NonBlocking>) -> Self {
        Self(writer)
    }
}

impl Write for MaybeFileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.as_mut().map_or(Ok(buf.len()), |w| w.write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.as_mut().map_or(Ok(()), Write::flush)
    }
}
