use ms_agent::agent_loop::{TurnOutput, TurnProcessor};
use ms_agent::boot::AgentContext;
use ms_proto::InboundMessage;
use ms_store::HistoryEntry;
use std::future::Future;
use std::pin::Pin;

struct ModelNotConfigured;

impl TurnProcessor for ModelNotConfigured {
    fn process<'a>(
        &'a mut self,
        _context: &'a AgentContext,
        _message: InboundMessage,
        _history: Vec<HistoryEntry>,
    ) -> Pin<Box<dyn Future<Output = Result<TurnOutput, String>> + 'a>> {
        Box::pin(async { Err("model client is not configured".to_owned()) })
    }
}

fn main() {
    let context = match ms_agent::boot::boot() {
        Ok(context) => context,
        Err(error) => {
            eprintln!("agent boot failed: {error}");
            std::process::exit(error.exit_code());
        }
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|error| {
            eprintln!("agent runtime failed: {error}");
            std::process::exit(75);
        });
    if let Err(error) = runtime.block_on(ms_agent::agent_loop::run(context, ModelNotConfigured)) {
        eprintln!("agent loop failed: {error}");
        std::process::exit(error.exit_code());
    }
}
