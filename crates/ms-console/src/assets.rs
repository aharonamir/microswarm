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
        "/i18n/en.json" => (
            "application/json; charset=utf-8",
            include_bytes!("../web/i18n/en.json"),
        ),
        "/i18n/zh.json" => (
            "application/json; charset=utf-8",
            include_bytes!("../web/i18n/zh.json"),
        ),
        _ if path.starts_with("/fonts/") => return None,
        _ => return None,
    };
    Some(Asset {
        content_type,
        bytes,
    })
}
