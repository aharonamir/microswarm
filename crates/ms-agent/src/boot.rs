use ms_proto::{AgentEventType, ExitReason, Scope, exit_code};
use ms_store::SessionStore;
use serde_json::json;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

static PROCESS_START: OnceLock<Instant> = OnceLock::new();

pub struct AgentContext {
    pub session_id: String,
    pub root_session_id: String,
    pub kind: String,
    pub role: String,
    pub proxy_url: Option<String>,
    pub scope: Scope,
    pub store: SessionStore,
}

#[derive(Debug)]
pub enum BootError {
    Config(String),
    Storage(ms_store::StoreError),
}

impl BootError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Config(_) => exit_code(ExitReason::Config),
            Self::Storage(error) if error.is_transient() => exit_code(ExitReason::Transient),
            Self::Storage(_) => exit_code(ExitReason::Safety),
        }
    }
}

impl fmt::Display for BootError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(message) => formatter.write_str(message),
            Self::Storage(error) => write!(formatter, "storage failed: {error}"),
        }
    }
}

impl Error for BootError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::Config(_) => None,
        }
    }
}

pub fn boot() -> Result<AgentContext, BootError> {
    let session_id = required_env("MS_SESSION_ID")?;
    let kind = required_env("MS_KIND")?;
    if !matches!(kind.as_str(), "lead" | "member") {
        return Err(BootError::Config(format!(
            "unsupported agent kind {kind:?}"
        )));
    }
    let role = required_env("MS_ROLE")?;
    if role.trim().is_empty() {
        return Err(BootError::Config(
            "MS_ROLE must contain the event role name".to_owned(),
        ));
    }
    let root_session_id =
        std::env::var("MS_ROOT_SESSION_ID").unwrap_or_else(|_| session_id.clone());
    let proxy_url = std::env::var("MS_PROXY_URL").ok();
    let session_dir = std::env::var_os("MS_SESSION_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/session"));
    let scope_path = session_dir.join("control/scope.json");
    let scope_bytes = std::fs::read(&scope_path).map_err(|error| {
        BootError::Config(format!("cannot read {}: {error}", scope_path.display()))
    })?;
    let scope: Scope = serde_json::from_slice(&scope_bytes)
        .map_err(|error| BootError::Config(format!("invalid scope.json: {error}")))?;
    if scope.session_id != session_id {
        return Err(BootError::Config(format!(
            "scope session_id {:?} does not match MS_SESSION_ID {:?}",
            scope.session_id, session_id
        )));
    }
    if scope.canonical_hash() != scope.hash {
        return Err(BootError::Config("scope.json hash mismatch".to_owned()));
    }

    let mut store = SessionStore::open_dir(&session_dir).map_err(BootError::Storage)?;
    crate::emit::emit(
        &mut store,
        &session_id,
        &root_session_id,
        &role,
        AgentEventType::Ready,
        json!({ "scope_hash": scope.hash }),
    )
    .map_err(BootError::Storage)?;

    Ok(AgentContext {
        session_id,
        root_session_id,
        kind,
        role,
        proxy_url,
        scope,
        store,
    })
}

fn required_env(name: &str) -> Result<String, BootError> {
    std::env::var(name)
        .map_err(|_| BootError::Config(format!("required environment variable {name} is missing")))
}

pub(crate) fn monotonic_nanos() -> u64 {
    PROCESS_START
        .get_or_init(Instant::now)
        .elapsed()
        .as_nanos()
        .try_into()
        .unwrap_or(u64::MAX)
}

pub(crate) fn utc_timestamp() -> String {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let days = (elapsed.as_secs() / 86_400) as i64;
    let second_of_day = elapsed.as_secs() % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = second_of_day / 3_600;
    let minute = (second_of_day % 3_600) / 60;
    let second = second_of_day % 60;
    let millis = elapsed.subsec_millis();
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}
