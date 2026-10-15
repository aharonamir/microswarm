fn main() {
    if let Err(error) = ms_agent::boot::boot() {
        eprintln!("agent boot failed: {error}");
        std::process::exit(error.exit_code());
    }
}
