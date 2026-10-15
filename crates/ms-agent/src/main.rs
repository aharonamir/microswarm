use ms_agent::model::ModelClient;

fn main() {
    let context = match ms_agent::boot::boot() {
        Ok(context) => context,
        Err(error) => {
            eprintln!("agent boot failed: {error}");
            std::process::exit(error.exit_code());
        }
    };
    let processor = match ModelClient::from_env(&context) {
        Ok(processor) => processor,
        Err(error) => {
            eprintln!("agent model configuration failed: {error}");
            std::process::exit(78);
        }
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|error| {
            eprintln!("agent runtime failed: {error}");
            std::process::exit(75);
        });
    if let Err(error) = runtime.block_on(ms_agent::agent_loop::run(context, processor)) {
        eprintln!("agent loop failed: {error}");
        std::process::exit(error.exit_code());
    }
}
