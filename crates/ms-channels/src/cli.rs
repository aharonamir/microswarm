use ms_proto::{InboundMessage, OutboundMessage};
use std::io::{self, BufRead, Write};

pub struct CliChannel<R, W> {
    input: R,
    output: W,
}

impl<R: BufRead, W: Write> CliChannel<R, W> {
    pub fn new(input: R, output: W) -> Self {
        Self { input, output }
    }

    pub fn receive(&mut self) -> io::Result<Option<InboundMessage>> {
        todo!("T043: read and validate a CLI channel message")
    }

    pub fn deliver(&mut self, _message: &OutboundMessage) -> io::Result<()> {
        todo!("T043: render an outbound CLI channel message")
    }

    pub fn into_output(self) -> W {
        self.output
    }
}
