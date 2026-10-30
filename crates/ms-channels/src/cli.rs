use crate::{ChannelAdapter, ChannelInbound};
use ms_proto::{InboundKind, InboundMessage, OutboundKind, OutboundMessage};
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
        let mut line = String::new();
        let read = self.input.read_line(&mut line)?;
        if read == 0 {
            return Ok(None);
        }
        while matches!(line.chars().last(), Some('\n' | '\r')) {
            line.pop();
        }
        ChannelAdapter::parse_inbound(self, &serde_json::json!({"text":line}))
            .map(|inbound| inbound.map(|message| message.message))
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub fn deliver(&mut self, message: &OutboundMessage) -> io::Result<()> {
        let text = match message.kind {
            OutboundKind::Reply => message
                .body
                .get("text")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "reply has no text"))?,
            OutboundKind::ApprovalRequest => message
                .body
                .get("prompt")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("An action needs approval."),
            _ => return Ok(()),
        };
        writeln!(self.output, "{text}")?;
        self.output.flush()
    }

    pub fn into_output(self) -> W {
        self.output
    }
}

impl<R, W> ChannelAdapter for CliChannel<R, W> {
    fn kind(&self) -> &'static str {
        "cli"
    }

    fn parse_inbound(&self, payload: &serde_json::Value) -> Result<Option<ChannelInbound>, String> {
        let text = payload
            .get("text")
            .and_then(serde_json::Value::as_str)
            .ok_or("CLI message requires a text string")?;
        Ok(Some(ChannelInbound {
            thread_id: "default".into(),
            message: InboundMessage {
                id: None,
                kind: InboundKind::UserMessage,
                body: serde_json::json!({"text":text}),
                created_at: None,
            },
        }))
    }

    fn reply_payload(&self, _thread_id: &str, text: &str) -> serde_json::Value {
        serde_json::json!({"text":text})
    }
}
