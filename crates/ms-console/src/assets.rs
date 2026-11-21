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
        "/img/mark.svg" => ("image/svg+xml", include_bytes!("../web/img/mark.svg")),
        "/js/app.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/app.js"),
        ),
        "/js/dom.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/dom.js"),
        ),
        "/js/format.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/format.js"),
        ),
        "/js/live.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/live.js"),
        ),
        "/js/components/nav.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/components/nav.js"),
        ),
        "/js/components/avatar.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/components/avatar.js"),
        ),
        "/js/components/pill.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/components/pill.js"),
        ),
        "/js/components/step.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/components/step.js"),
        ),
        "/js/components/teamcard.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/components/teamcard.js"),
        ),
        "/js/components/dock.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/components/dock.js"),
        ),
        "/js/components/tabs.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/components/tabs.js"),
        ),
        "/js/components/drawer.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/components/drawer.js"),
        ),
        "/js/components/lanes.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/components/lanes.js"),
        ),
        "/js/components/teampanel.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/components/teampanel.js"),
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
        "/js/pages/session.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/pages/session.js"),
        ),
        "/js/pages/approvals.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/pages/approvals.js"),
        ),
        "/js/pages/trajectory.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/pages/trajectory.js"),
        ),
        "/js/components/inspector.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/components/inspector.js"),
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
        "/js/pages/overview.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/js/pages/overview.js"),
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
