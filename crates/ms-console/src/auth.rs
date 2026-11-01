use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    time::{Duration, SystemTime},
};

const TOKEN_TTL: Duration = Duration::from_secs(600);
const SESSION_TTL: Duration = Duration::from_secs(12 * 60 * 60);

pub struct ConsoleAuth {
    token: String,
    expires_at: SystemTime,
    used: bool,
    sessions: BTreeMap<String, SystemTime>,
}

impl ConsoleAuth {
    pub fn new() -> Result<Self, String> {
        let token = random_hex(32)?;
        Ok(Self::with_token(token, SystemTime::now()))
    }

    pub fn with_token(token: String, now: SystemTime) -> Self {
        Self {
            token,
            expires_at: now + TOKEN_TTL,
            used: false,
            sessions: BTreeMap::new(),
        }
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn consume_token(&mut self, candidate: &str, now: SystemTime) -> Option<String> {
        if self.used
            || now >= self.expires_at
            || !constant_time_eq(self.token.as_bytes(), candidate.as_bytes())
        {
            return None;
        }
        self.used = true;
        self.token.clear();
        let id = random_hex(32).ok()?;
        self.sessions.insert(id.clone(), now);
        Some(id)
    }

    pub fn has_session(&mut self, id: &str, now: SystemTime) -> bool {
        self.sessions
            .retain(|_, seen| now.duration_since(*seen).unwrap_or_default() < SESSION_TTL);
        if let Some(seen) = self.sessions.get_mut(id) {
            *seen = now;
            true
        } else {
            false
        }
    }

    pub fn logout(&mut self, id: &str) {
        self.sessions.remove(id);
    }
}

pub fn body_status(len: usize) -> u16 {
    if len > 65_536 { 413 } else { 200 }
}

fn random_hex(bytes: usize) -> Result<String, String> {
    let mut raw = vec![0; bytes];
    File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut raw))
        .map_err(|e| format!("secure randomness unavailable: {e}"))?;
    Ok(raw.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        difference |= usize::from(
            left.get(index).copied().unwrap_or_default()
                ^ right.get(index).copied().unwrap_or_default(),
        );
    }
    difference == 0
}

pub fn check_request(
    method: &str,
    host: &str,
    origin: Option<&str>,
    content_type: Option<&str>,
    console_header: Option<&str>,
) -> u16 {
    let host_ok = ["127.0.0.1:8790", "localhost:8790"].contains(&host);
    if !host_ok {
        return 421;
    }
    if method.eq_ignore_ascii_case("POST") {
        if origin != Some(&format!("http://{host}")[..]) || console_header != Some("1") {
            return 403;
        }
        if !content_type.is_some_and(|value| {
            value
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("application/json")
        }) {
            return 415;
        }
    }
    200
}
