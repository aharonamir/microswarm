use ms_console::read;
use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

#[test]
fn credential_values_never_appear_in_host_view() {
    let home = std::env::temp_dir().join(format!(
        "ms-console-secret-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let credentials = home.join("credentials");
    fs::create_dir_all(&credentials).unwrap();
    fs::write(
        credentials.join("model-key"),
        "never-return-this-secret-value",
    )
    .unwrap();
    let response = read::host(&home).to_string();
    assert!(response.contains("model-key"));
    assert!(!response.contains("never-return-this-secret-value"));
    fs::remove_dir_all(home).unwrap();
}
