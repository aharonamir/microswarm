use rusqlite::Connection;
use rusqlite::OpenFlags;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const SESSION_SCHEMA: &str = include_str!("../../../specs/001-microswarm/contracts/session-db.sql");
const SUPERVISOR_SCHEMA: &str =
    include_str!("../../../specs/001-microswarm/contracts/supervisor-db.sql");
const SESSION_SCHEMA_VERSION: i64 = 1;
const SUPERVISOR_SCHEMA_VERSION: i64 = 2;

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

impl StoreError {
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::Sqlite(rusqlite::Error::SqliteFailure(error, _))
                if matches!(
                    error.code,
                    rusqlite::ffi::ErrorCode::DatabaseBusy
                        | rusqlite::ffi::ErrorCode::DatabaseLocked
                        | rusqlite::ffi::ErrorCode::CannotOpen
                )
        )
    }
}

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
    let control = path.join("control");
    let out = path.join("out");
    fs::create_dir_all(&control)?;
    fs::create_dir_all(&out)?;
    let (inbound_sql, outbound_sql, events_sql) = session_sections()?;
    let inbound = open_database(
        control.join("inbound.db"),
        inbound_sql,
        "NORMAL",
        SESSION_SCHEMA_VERSION,
    )?;
    set_persist_wal(&inbound)?;
    // Force SQLite to initialize the WAL and shared-memory sidecars while the
    // supervisor has a writable connection, before scope persistence/container start.
    inbound.execute_batch("BEGIN IMMEDIATE; COMMIT;")?;
    Ok(SessionConnections {
        inbound,
        outbound: open_database(
            out.join("outbound.db"),
            outbound_sql,
            "NORMAL",
            SESSION_SCHEMA_VERSION,
        )?,
        events: open_database(
            out.join("events.db"),
            events_sql,
            "NORMAL",
            SESSION_SCHEMA_VERSION,
        )?,
    })
}

fn set_persist_wal(connection: &Connection) -> Result<(), StoreError> {
    let database = std::ffi::CString::new("main").expect("static database name has no NUL");
    let mut persist: std::ffi::c_int = 1;
    // SQLITE_FCNTL_PERSIST_WAL expects a pointer to an integer flag.
    let result = unsafe {
        rusqlite::ffi::sqlite3_file_control(
            connection.handle(),
            database.as_ptr(),
            rusqlite::ffi::SQLITE_FCNTL_PERSIST_WAL,
            (&mut persist as *mut std::ffi::c_int).cast(),
        )
    };
    if result != rusqlite::ffi::SQLITE_OK {
        return Err(StoreError::Sqlite(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(result),
            None,
        )));
    }
    Ok(())
}

/// Open the agent view: immutable control input plus writable output databases.
/// The supervisor must retain its inbound writer connection for the session lifetime.
pub fn open_agent_session_dir(path: impl AsRef<Path>) -> Result<SessionConnections, StoreError> {
    let path = path.as_ref();
    Ok(SessionConnections {
        inbound: open_existing(
            path.join("control/inbound.db"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?,
        outbound: open_existing(
            path.join("out/outbound.db"),
            OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?,
        events: open_existing(
            path.join("out/events.db"),
            OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?,
    })
}

pub fn open_supervisor_db(path: impl AsRef<Path>) -> Result<Connection, StoreError> {
    open_database(
        path.as_ref().to_owned(),
        SUPERVISOR_SCHEMA,
        "FULL",
        SUPERVISOR_SCHEMA_VERSION,
    )
}

fn open_database(
    path: PathBuf,
    schema: &str,
    synchronous: &str,
    schema_version: i64,
) -> Result<Connection, StoreError> {
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
        current if current == schema_version => {}
        0 => {
            connection.execute_batch(schema)?;
            connection.execute_batch(&format!("PRAGMA user_version={schema_version};"))?;
        }
        1 if schema_version == SUPERVISOR_SCHEMA_VERSION => {
            connection.execute_batch("ALTER TABLE sessions ADD COLUMN scope_json TEXT CHECK (scope_json IS NULL OR json_valid(scope_json)); PRAGMA user_version=2;")?;
        }
        other => return Err(StoreError::UnsupportedSchemaVersion(other)),
    }
    Ok(connection)
}

fn open_existing(path: PathBuf, flags: OpenFlags) -> Result<Connection, StoreError> {
    let connection = Connection::open_with_flags(path, flags)?;
    connection.busy_timeout(Duration::from_secs(5))?;
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
