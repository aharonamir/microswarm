use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const SESSION_SCHEMA: &str = include_str!("../../../specs/001-microswarm/contracts/session-db.sql");
const SUPERVISOR_SCHEMA: &str =
    include_str!("../../../specs/001-microswarm/contracts/supervisor-db.sql");
const SCHEMA_VERSION: i64 = 1;

#[derive(Debug)]
pub enum StoreError {
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    Io(std::io::Error),
    UnsupportedSchemaVersion(i64),
    InvalidContract(&'static str),
    IntegerOutOfRange,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite(error) => write!(f, "SQLite error: {error}"),
            Self::Json(error) => write!(f, "JSON error: {error}"),
            Self::Io(error) => write!(f, "storage I/O error: {error}"),
            Self::UnsupportedSchemaVersion(version) => {
                write!(f, "unsupported database schema version {version}")
            }
            Self::InvalidContract(section) => write!(f, "missing SQL contract section: {section}"),
            Self::IntegerOutOfRange => write!(f, "integer does not fit SQLite INTEGER"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<std::io::Error> for StoreError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub struct SessionConnections {
    pub(crate) inbound: Connection,
    pub(crate) outbound: Connection,
    pub(crate) events: Connection,
}

impl SessionConnections {
    pub fn inbound(&self) -> &Connection {
        &self.inbound
    }

    pub fn outbound(&self) -> &Connection {
        &self.outbound
    }

    pub fn events(&self) -> &Connection {
        &self.events
    }
}

pub fn open_session_dir(path: impl AsRef<Path>) -> Result<SessionConnections, StoreError> {
    let path = path.as_ref();
    fs::create_dir_all(path)?;
    let (inbound_sql, outbound_sql, events_sql) = session_sections()?;
    Ok(SessionConnections {
        inbound: open_database(path.join("inbound.db"), inbound_sql, "NORMAL")?,
        outbound: open_database(path.join("outbound.db"), outbound_sql, "NORMAL")?,
        events: open_database(path.join("events.db"), events_sql, "NORMAL")?,
    })
}

pub fn open_supervisor_db(path: impl AsRef<Path>) -> Result<Connection, StoreError> {
    open_database(path.as_ref().to_owned(), SUPERVISOR_SCHEMA, "FULL")
}

fn open_database(path: PathBuf, schema: &str, synchronous: &str) -> Result<Connection, StoreError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let connection = Connection::open(path)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.execute_batch("PRAGMA foreign_keys=ON;")?;
    connection.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
    connection.execute_batch(&format!("PRAGMA synchronous={synchronous};"))?;

    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    match version {
        SCHEMA_VERSION => {}
        0 => {
            connection.execute_batch(schema)?;
            connection.execute_batch(&format!("PRAGMA user_version={SCHEMA_VERSION};"))?;
        }
        other => return Err(StoreError::UnsupportedSchemaVersion(other)),
    }
    Ok(connection)
}

fn session_sections() -> Result<(&'static str, &'static str, &'static str), StoreError> {
    let inbound_start = SESSION_SCHEMA
        .find("CREATE TABLE inbound (")
        .ok_or(StoreError::InvalidContract("inbound"))?;
    let outbound_start = SESSION_SCHEMA
        .find("CREATE TABLE outbound (")
        .ok_or(StoreError::InvalidContract("outbound"))?;
    let events_start = SESSION_SCHEMA
        .find("CREATE TABLE events (")
        .ok_or(StoreError::InvalidContract("events"))?;
    let bodies_start = SESSION_SCHEMA
        .find("-- ───────────────────────── Body shapes")
        .ok_or(StoreError::InvalidContract("body comments"))?;
    Ok((
        &SESSION_SCHEMA[inbound_start..outbound_start],
        &SESSION_SCHEMA[outbound_start..events_start],
        &SESSION_SCHEMA[events_start..bodies_start],
    ))
}
