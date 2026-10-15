use crate::{StoreError, open_session_dir};
use ms_proto::{AgentEvent, InboundMessage, OutboundMessage};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use std::path::Path;

pub struct SessionStore {
    inbound: Connection,
    outbound: Connection,
    events: Connection,
}

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub role: String,
    pub content: Value,
}

impl SessionStore {
    pub fn open_dir(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let connections = open_session_dir(path)?;
        Ok(Self {
            inbound: connections.inbound,
            outbound: connections.outbound,
            events: connections.events,
        })
    }

    pub fn append_inbound(&mut self, message: InboundMessage) -> Result<i64, StoreError> {
        let kind = serde_json::to_value(message.kind)?;
        let body = serde_json::to_string(&message.body)?;
        self.inbound.execute(
            "INSERT INTO inbound(kind, body) VALUES (?1, ?2)",
            params![kind.as_str().unwrap_or_default(), body],
        )?;
        Ok(self.inbound.last_insert_rowid())
    }

    pub fn next_inbound(&self, after_id: i64) -> Result<Option<InboundMessage>, StoreError> {
        let row: Option<(i64, String, String, String)> = self
            .inbound
            .query_row(
                "SELECT id, kind, body, created_at FROM inbound WHERE id > ?1 ORDER BY id LIMIT 1",
                [after_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        row.map(|(id, kind, body, created_at)| {
            Ok(InboundMessage {
                id: Some(id),
                kind: serde_json::from_value(Value::String(kind))?,
                body: serde_json::from_str(&body)?,
                created_at: Some(created_at),
            })
        })
        .transpose()
    }

    pub fn commit_turn(
        &mut self,
        last_inbound_id: i64,
        outbound: &[OutboundMessage],
        history: &[HistoryEntry],
    ) -> Result<(), StoreError> {
        let transaction = self.outbound.transaction()?;
        for message in outbound {
            let kind = serde_json::to_value(&message.kind)?;
            transaction.execute(
                "INSERT INTO outbound(kind, body, in_reply_to) VALUES (?1, ?2, ?3)",
                params![
                    kind.as_str().unwrap_or_default(),
                    serde_json::to_string(&message.body)?,
                    message.in_reply_to
                ],
            )?;
        }
        transaction.execute(
            "UPDATE cursor SET last_inbound_id = MAX(last_inbound_id, ?1) WHERE singleton=1",
            [last_inbound_id],
        )?;
        for entry in history {
            transaction.execute(
                "INSERT INTO history(role, content) VALUES (?1, ?2)",
                params![entry.role, serde_json::to_string(&entry.content)?],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn append_agent_event(&mut self, event: &AgentEvent) -> Result<i64, StoreError> {
        let transaction = self.events.transaction()?;
        let previous: Option<i64> = transaction
            .query_row("SELECT MAX(seq) FROM events", [], |row| row.get(0))
            .optional()?
            .flatten();
        let seq = previous.unwrap_or(0) + 1;
        let event_type = serde_json::to_value(event.event_type)?;
        let data = serde_json::to_string(&event.data)?;
        let ts_mono = i64::try_from(event.ts_mono).map_err(|_| StoreError::IntegerOutOfRange)?;
        transaction.execute(
            "INSERT INTO events(seq, ts, ts_mono, session_id, root_session_id, agent_role, type, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                seq,
                event.ts,
                ts_mono,
                event.session_id,
                event.root_session_id,
                event.agent_role,
                event_type.as_str().unwrap_or_default(),
                data
            ],
        )?;
        transaction.commit()?;
        Ok(seq)
    }

    pub fn cursor(&self) -> Result<i64, StoreError> {
        Ok(self.outbound.query_row(
            "SELECT last_inbound_id FROM cursor WHERE singleton=1",
            [],
            |row| row.get(0),
        )?)
    }

    pub fn history(&self) -> Result<Vec<HistoryEntry>, StoreError> {
        let mut statement = self
            .outbound
            .prepare("SELECT role, content FROM history ORDER BY id")?;
        let rows = statement.query_map([], |row| {
            let role: String = row.get(0)?;
            let content: String = row.get(1)?;
            let content = serde_json::from_str(&content).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    content.len(),
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok(HistoryEntry { role, content })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }
}
