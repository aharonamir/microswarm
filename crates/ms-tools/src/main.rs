#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = ms_tools::serve_stdio().await {
        eprintln!("microswarm-tools failed: {error}");
        std::process::exit(75);
    }
}
