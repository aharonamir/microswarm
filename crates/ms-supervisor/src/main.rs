use std::path::PathBuf;

#[tokio::main]
async fn main() {
    let home = std::env::var_os("MS_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|path| PathBuf::from(path).join(".microswarm")))
        .unwrap_or_else(|| PathBuf::from(".microswarm"));
    if let Err(error) = ms_supervisor::service::serve(home).await {
        eprintln!("microswarmd: {error}");
        std::process::exit(1);
    }
}
