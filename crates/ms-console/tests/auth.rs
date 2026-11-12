use ms_console::{
    auth::{ConsoleAuth, body_status, check_request},
    server::{session_cookie, validate_listen},
};
use std::{
    net::SocketAddr,
    time::{Duration, SystemTime},
};

#[test]
fn startup_token_is_single_use_and_expires_after_ten_minutes() {
    let now = SystemTime::now();
    let mut auth = ConsoleAuth::with_token("ab".repeat(32), now);
    assert!(auth.consume_token(&"ab".repeat(32), now).is_some());
    assert!(auth.consume_token(&"ab".repeat(32), now).is_none());
    let mut expired = ConsoleAuth::with_token("cd".repeat(32), now);
    assert!(
        expired
            .consume_token(&"cd".repeat(32), now + Duration::from_secs(601))
            .is_none()
    );
}

#[test]
fn login_cookie_has_required_attributes_and_listen_is_loopback_only() {
    let cookie = session_cookie("opaque");
    for attribute in ["HttpOnly", "SameSite=Strict", "Path=/"] {
        assert!(cookie.contains(attribute));
    }
    assert!(validate_listen("127.0.0.1:8790".parse::<SocketAddr>().unwrap()).is_ok());
    assert!(validate_listen("0.0.0.0:8790".parse::<SocketAddr>().unwrap()).is_err());
}

#[test]
fn non_loopback_error_includes_ssh_and_tls_guidance() {
    let error = validate_listen("0.0.0.0:8790".parse::<SocketAddr>().unwrap()).unwrap_err();
    assert_eq!(
        error,
        "The console only listens on loopback. Use an SSH tunnel (ssh -L 8790:127.0.0.1:8790 host) or a TLS proxy."
    );
}

#[test]
fn request_guard_enforces_host_origin_header_and_content_type() {
    assert_eq!(check_request("GET", "attacker.test", None, None, None), 421);
    assert_eq!(
        check_request(
            "POST",
            "127.0.0.1:8790",
            None,
            Some("application/json"),
            Some("1")
        ),
        403
    );
    assert_eq!(
        check_request(
            "POST",
            "127.0.0.1:8790",
            Some("http://localhost:8790"),
            Some("application/json"),
            Some("1")
        ),
        403
    );
    assert_eq!(
        check_request(
            "POST",
            "localhost:8790",
            Some("http://localhost:8790"),
            Some("text/plain"),
            Some("1")
        ),
        415
    );
    assert_eq!(
        check_request(
            "POST",
            "localhost:8790",
            Some("http://localhost:8790"),
            Some("application/json"),
            None
        ),
        403
    );
    assert_eq!(
        check_request("GET", "localhost:8790", None, None, None),
        200
    );
}

#[test]
fn request_body_limit_is_64_kib_and_unknown_cookie_is_not_a_session() {
    assert_eq!(body_status(65_536), 200);
    assert_eq!(body_status(65_537), 413);
    let mut auth = ConsoleAuth::with_token("ef".repeat(32), SystemTime::now());
    assert!(!auth.has_session("not-a-session", SystemTime::now()));
}
