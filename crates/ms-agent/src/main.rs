use ms_agent::model::ModelClient;
use ms_proto::AgentEventType;
use serde_json::json;

fn main() {
    eprintln!("agent: booting");
    let mut context = match ms_agent::boot::boot() {
        Ok(context) => context,
        Err(error) => {
            eprintln!("agent boot failed: {error}");
            emit_boot_failure(error.exit_code());
            std::process::exit(error.exit_code());
        }
    };
    eprintln!("agent: boot complete; starting turn loop");
    let processor = match ModelClient::from_env(&context) {
        Ok(processor) => processor,
        Err(error) => {
            eprintln!("agent model configuration failed: {error}");
            std::process::exit(finish(&mut context, 78));
        }
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|error| {
            eprintln!("agent runtime failed: {error}");
            finish(&mut context, 75);
            std::process::exit(75);
        });
    let code = match runtime.block_on(ms_agent::agent_loop::run(context, processor)) {
        Ok(()) => {
            eprintln!("agent: turn loop returned successfully");
            0
        }
        Err(error) => {
            eprintln!("agent loop failed: {error}");
            error.exit_code()
        }
    };
    std::process::exit(finish_context_from_env(code));
}

fn finish(context: &mut ms_agent::boot::AgentContext, code: i32) -> i32 {
    let reason = ms_agent::exit::reason_for_code(code);
    if let Err(error) = ms_agent::emit::emit(
        &mut context.store,
        &context.session_id,
        &context.root_session_id,
        &context.role,
        AgentEventType::Exit,
        json!({"reason":ms_agent::exit::reason_name(reason),"code":code}),
    ) {
        eprintln!("cannot write agent.exit event: {error}");
        return 77;
    }
    code
}

fn finish_context_from_env(code: i32) -> i32 {
    let Some(session_id) = std::env::var("MS_SESSION_ID").ok() else {
        return code;
    };
    let root_session_id =
        std::env::var("MS_ROOT_SESSION_ID").unwrap_or_else(|_| session_id.clone());
    let role = std::env::var("MS_ROLE").unwrap_or_else(|_| "lead".to_owned());
    let dir = std::env::var_os("MS_SESSION_DIR").unwrap_or_else(|| "/session".into());
    let mut store = match ms_store::SessionStore::open_dir(dir) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("cannot open session store to write agent.exit: {error}");
            return code;
        }
    };
    let reason = ms_agent::exit::reason_for_code(code);
    if let Err(error) = ms_agent::emit::emit(
        &mut store,
        &session_id,
        &root_session_id,
        &role,
        AgentEventType::Exit,
        json!({"reason":ms_agent::exit::reason_name(reason),"code":code}),
    ) {
        eprintln!("cannot write agent.exit event: {error}");
        77
    } else {
        code
    }
}

fn emit_boot_failure(code: i32) {
    let _ = finish_context_from_env(code);
}
