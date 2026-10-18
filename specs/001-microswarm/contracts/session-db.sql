-- MicroSwarm per-session databases.
-- Paths: control/inbound.db (supervisor writer; agent read-only mount),
-- out/outbound.db and out/events.db (agent writers; supervisor readers).
-- Each statement block belongs to exactly one file, and each file has exactly one writer.
-- All files: PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;
-- Schema version is tracked with PRAGMA user_version = 1.

-- ───────────────────────── control/inbound.db  (writer: supervisor · read-only agent mount)
CREATE TABLE inbound (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  kind        TEXT    NOT NULL CHECK (kind IN (
                'user_message','approval_result','delegation_result',
                'member_reply','member_failed','task','control')),
  body        TEXT    NOT NULL CHECK (json_valid(body)),
  created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

-- ───────────────────────── out/outbound.db  (writer: agent · reader: supervisor)
CREATE TABLE outbound (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  kind        TEXT    NOT NULL CHECK (kind IN (
                'reply','approval_request','delegation_request',
                'send_to_member','dismiss')),
  body        TEXT    NOT NULL CHECK (json_valid(body)),
  in_reply_to INTEGER,                -- inbound.id that produced this row, if any
  created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

-- Single-row cursor. Updated in the SAME transaction as the outbound rows of a turn.
-- A message in inbound.db is acknowledged iff inbound.id <= cursor.last_inbound_id.
CREATE TABLE cursor (
  singleton        INTEGER PRIMARY KEY CHECK (singleton = 1),
  last_inbound_id  INTEGER NOT NULL DEFAULT 0
);
INSERT INTO cursor (singleton, last_inbound_id) VALUES (1, 0);

-- Model conversation history for this session. Private to the agent.
CREATE TABLE history (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  role        TEXT NOT NULL CHECK (role IN ('system','user','assistant','tool')),
  content     TEXT NOT NULL CHECK (json_valid(content)),
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

-- ───────────────────────── out/events.db  (writer: agent · reader: supervisor, host plane)
CREATE TABLE events (
  seq              INTEGER PRIMARY KEY,      -- strictly increasing, no gaps
  ts               TEXT    NOT NULL,
  ts_mono          INTEGER NOT NULL,
  session_id       TEXT    NOT NULL,
  root_session_id  TEXT    NOT NULL,
  agent_role       TEXT    NOT NULL,
  type             TEXT    NOT NULL,         -- closed set, see events.schema.json
  data             TEXT    NOT NULL CHECK (json_valid(data))
);
CREATE INDEX events_type ON events(type);

-- ───────────────────────── Body shapes (JSON), by kind
-- inbound.user_message       {"text": str, "attachments": [..], "from": str}
-- inbound.approval_result    {"approval_id": ulid, "decision": "approved"|"denied"|"timed_out"|"expired"}
-- inbound.delegation_result  {"delegation_id": ulid, "status": "spawned"|"denied",
--                             "member_id": ulid?, "reason": str?, "scope": {...}?, "narrowed": [...]?}
-- inbound.member_reply       {"member_id": ulid, "delegation_id": ulid, "text": str}
-- inbound.member_failed      {"member_id": ulid, "delegation_id": ulid,
--                             "reason": "timeout"|"crashed"|"safety"|"spawn_error"|"config"}
-- inbound.task               {"delegation_id": ulid, "from_lead": ulid, "text": str}
-- inbound.control            {"op": "shutdown", "grace_ms": int}
--
-- outbound.reply             {"text": str}
-- outbound.approval_request  {"approval_local_id": str, "tool": str, "server": str, "args": {...}}
-- outbound.delegation_request see contracts/delegation-tools.json → spawn_member.input
-- outbound.send_to_member    {"member_id": ulid, "text": str, "timeout_s": int?}
-- outbound.dismiss           {"member_id": ulid}
