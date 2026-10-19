use crate::{StoreError, open_supervisor_db};
use ms_proto::SupervisorEvent;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use std::path::Path;

pub struct SupervisorStore {
    connection: Connection,
}

#[derive(Clone, Debug)]
pub struct SessionRecord {
    pub id: String,
    pub root_id: String,
    pub parent_id: Option<String>,
    pub role: String,
    pub kind: String,
    pub channel: Option<String>,
    pub channel_thread: Option<String>,
    pub status: String,
    pub scope_hash: Option<String>,
    pub scope: Option<Value>,
    pub created_at: String,
    pub last_active_at: String,
}

#[derive(Clone, Debug)]
pub struct DelegationRecord {
    pub id: String,
    pub lead_id: String,
    pub member_id: Option<String>,
    pub role: String,
    pub task: String,
    pub scope_request: Value,
    pub policy: String,
    pub approval_id: Option<String>,
    pub timeout_s: u64,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug)]
pub struct ApprovalRecord {
    pub id: String,
    pub code: String,
    pub root_session_id: String,
    pub requesting_session_id: String,
    pub kind: String,
    pub payload: Value,
    pub status: String,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Clone, Debug)]
pub struct TeamRoleRecord {
    pub skill: String,
    pub skill_version: String,
    pub skill_hash: String,
    pub role: String,
    pub spawn_policy: String,
    pub scope: Value,
    pub instructions: String,
}

#[derive(Clone, Debug)]
pub struct SessionLifecycleUpdate {
    pub session_id: String,
    pub status: String,
    pub container_id: Option<String>,
    pub restart_count: u8,
    pub last_active_at: String,
    pub closed_at: Option<String>,
    pub close_reason: Option<String>,
}

impl SupervisorStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        Ok(Self {
            connection: open_supervisor_db(path)?,
        })
    }

    pub fn insert_session(&mut self, session: &SessionRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO sessions(id, root_id, parent_id, role, kind, channel, channel_thread, status, scope_hash, scope_json, created_at, last_active_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                session.id, session.root_id, session.parent_id, session.role, session.kind,
                session.channel, session.channel_thread, session.status, session.scope_hash,
                session.scope.as_ref().map(serde_json::to_string).transpose()?, session.created_at,
                session.last_active_at
            ],
        )?;
        Ok(())
    }

    pub fn active_lead_for_thread(
        &self,
        channel: &str,
        thread: &str,
    ) -> Result<Option<String>, StoreError> {
        self.connection
            .query_row(
                "SELECT id FROM sessions WHERE kind='lead' AND channel=?1 AND channel_thread=?2 AND status NOT IN ('closed','failed') ORDER BY created_at DESC LIMIT 1",
                [channel, thread],
                |row| row.get(0),
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn insert_delegation(&mut self, delegation: &DelegationRecord) -> Result<(), StoreError> {
        let timeout_s =
            i64::try_from(delegation.timeout_s).map_err(|_| StoreError::IntegerOutOfRange)?;
        self.connection.execute(
            "INSERT INTO delegations(id, lead_id, member_id, role, task, scope_request, policy, approval_id, timeout_s, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                delegation.id,
                delegation.lead_id,
                delegation.member_id,
                delegation.role,
                delegation.task,
                serde_json::to_string(&delegation.scope_request)?,
                delegation.policy,
                delegation.approval_id,
                timeout_s,
                delegation.status,
                delegation.created_at,
                delegation.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn insert_approval(&mut self, approval: &ApprovalRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO approvals(id, code, root_session_id, requesting_session_id, kind, payload, status, expires_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                approval.id,
                approval.code,
                approval.root_session_id,
                approval.requesting_session_id,
                approval.kind,
                serde_json::to_string(&approval.payload)?,
                approval.status,
                approval.expires_at,
                approval.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn upsert_team_role(&mut self, role: &TeamRoleRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO team_roles(skill, skill_version, skill_hash, role, spawn_policy, scope, instructions)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(skill, role) DO UPDATE SET skill_version=excluded.skill_version,
               skill_hash=excluded.skill_hash, spawn_policy=excluded.spawn_policy,
               scope=excluded.scope, instructions=excluded.instructions",
            params![
                role.skill,
                role.skill_version,
                role.skill_hash,
                role.role,
                role.spawn_policy,
                serde_json::to_string(&role.scope)?,
                role.instructions,
            ],
        )?;
        Ok(())
    }

    pub fn upsert_tail_cursor(
        &mut self,
        session_id: &str,
        outbound_last_id: i64,
        events_last_seq: i64,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO tail_cursors(session_id, outbound_last_id, events_last_seq) VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET outbound_last_id=excluded.outbound_last_id,
               events_last_seq=excluded.events_last_seq",
            params![session_id, outbound_last_id, events_last_seq],
        )?;
        Ok(())
    }

    pub fn tail_cursor(&self, session_id: &str) -> Result<Option<(i64, i64)>, StoreError> {
        self.connection
            .query_row(
                "SELECT outbound_last_id, events_last_seq FROM tail_cursors WHERE session_id=?1",
                [session_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn append_supervisor_event(&mut self, event: &SupervisorEvent) -> Result<i64, StoreError> {
        let transaction = self.connection.transaction()?;
        let previous: Option<i64> =
            transaction.query_row("SELECT MAX(seq) FROM supervisor_events", [], |row| {
                row.get(0)
            })?;
        let seq = previous.unwrap_or(0) + 1;
        let event_type = serde_json::to_value(event.event_type)?;
        let ts_mono = i64::try_from(event.ts_mono).map_err(|_| StoreError::IntegerOutOfRange)?;
        transaction.execute(
            "INSERT INTO supervisor_events(seq, ts, ts_mono, session_id, root_session_id, agent_role, type, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                seq,
                event.ts,
                ts_mono,
                event.session_id,
                event.root_session_id,
                event.agent_role,
                event_type.as_str().unwrap_or_default(),
                serde_json::to_string(&event.data)?,
            ],
        )?;
        transaction.commit()?;
        Ok(seq)
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn update_session_lifecycle(
        &mut self,
        update: &SessionLifecycleUpdate,
    ) -> Result<(), StoreError> {
        let changed = self.connection.execute(
            "UPDATE sessions SET status=?2, container_id=COALESCE(?3,container_id), restart_count=?4,
             last_active_at=?5, closed_at=COALESCE(?6,closed_at), close_reason=COALESCE(?7,close_reason)
             WHERE id=?1",
            params![
                update.session_id,
                update.status,
                update.container_id,
                i64::from(update.restart_count),
                update.last_active_at,
                update.closed_at,
                update.close_reason
            ],
        )?;
        if changed != 1 {
            return Err(StoreError::InvalidContract("session lifecycle row missing"));
        }
        Ok(())
    }
}
