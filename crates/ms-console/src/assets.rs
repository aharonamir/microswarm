pub struct Asset {
    pub content_type: &'static str,
    pub bytes: &'static [u8],
}

pub fn get(path: &str) -> Option<Asset> {
    let (content_type, bytes): (&str, &[u8]) = match path {
        "/" => (
            "text/html; charset=utf-8",
            include_bytes!("../web/index.html"),
        ),
        "/styles.css" => (
            "text/css; charset=utf-8",
            include_bytes!("../web/styles.css"),
        ),
        "/js/app.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/app.js"),
        ),
        "/js/dom.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/dom.js"),
        ),
        "/js/router.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/router.js"),
        ),
        "/js/api.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/api.js"),
        ),
        "/js/sse.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/sse.js"),
        ),
        "/js/i18n.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/i18n.js"),
        ),
        "/js/pages/sessions.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/pages/sessions.js"),
        ),
        "/js/pages/approvals.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/pages/approvals.js"),
        ),
        "/js/pages/trace.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/pages/trace.js"),
        ),
        "/js/pages/host.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/pages/host.js"),
        ),
        "/js/pages/member.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/pages/member.js"),
        ),
        "/i18n/en.json" => (
            "application/json; charset=utf-8",
            include_bytes!("../web/i18n/en.json"),
        ),
        "/i18n/zh.json" => (
            "application/json; charset=utf-8",
            include_bytes!("../web/i18n/zh.json"),
        ),
        "/fonts/plex-sans-regular.woff2" => (
            "font/woff2",
            include_bytes!("../web/fonts/plex-sans-regular.woff2"),
        ),
        "/fonts/plex-sans-medium.woff2" => (
            "font/woff2",
            include_bytes!("../web/fonts/plex-sans-medium.woff2"),
        ),
        "/fonts/plex-sans-semibold.woff2" => (
            "font/woff2",
            include_bytes!("../web/fonts/plex-sans-semibold.woff2"),
        ),
        "/fonts/plex-mono-regular.woff2" => (
            "font/woff2",
            include_bytes!("../web/fonts/plex-mono-regular.woff2"),
        ),
        "/fonts/plex-mono-medium.woff2" => (
            "font/woff2",
            include_bytes!("../web/fonts/plex-mono-medium.woff2"),
        ),
        _ if path.starts_with("/fonts/") => return None,
        _ => return None,
    };
    Some(Asset {
        content_type,
        bytes,
    })
}
