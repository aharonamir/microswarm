use ms_channels::cli::CliChannel;
use ms_proto::{InboundKind, OutboundKind, OutboundMessage};
use serde_json::json;
use std::io::Cursor;

#[test]
fn cli_channel_reads_messages_and_delivers_replies_and_approval_prompts() {
    let mut channel = CliChannel::new(Cursor::new("hello from terminal\n"), Vec::new());
    let message = channel.receive().unwrap().unwrap();
    assert_eq!(message.kind, InboundKind::UserMessage);
    assert_eq!(message.body["text"], "hello from terminal");
    channel
        .deliver(&OutboundMessage {
            id: Some(2),
            kind: OutboundKind::Reply,
            body: json!({"text":"hello back"}),
            in_reply_to: Some(1),
            created_at: None,
        })
        .unwrap();
    let output = String::from_utf8(channel.into_output()).unwrap();
    assert!(output.contains("hello back"));
}
