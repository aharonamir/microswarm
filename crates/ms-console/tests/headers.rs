use ms_console::server::security_headers;

#[test]
fn security_headers_are_exact_and_complete() {
    let headers = security_headers();
    assert_eq!(
        headers["content-security-policy"],
        "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"
    );
    for name in ["x-content-type-options", "referrer-policy"] {
        assert!(headers.contains_key(name), "missing {name}");
    }
}
