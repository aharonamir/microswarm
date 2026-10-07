-- MicroSwarm supervisor.db  (writer: supervisor only)
-- PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA user_version=1;
-- synchronous=FULL here (vs NORMAL for session files): this file holds the state that has
-- no other source of truth after a crash.

CREATE TABLE sessions (
  id              TEXT PRIMARY KEY,                       -- ULID
  root_id         TEXT NOT NULL,
  parent_id       TEXT REFERENCES sessions(id),
  role            TEXT NOT NULL,
  kind            TEXT NOT NULL CHECK (kind IN ('lead','member')),
  channel         TEXT,
  channel_thread  TEXT,
  status          TEXT NOT NULL CHECK (status IN (
                    'pending','starting','ready','busy','stopping',
                    'crashed','closed','failed')),
  container_id    TEXT,
  restart_count   INTEGER NOT NULL DEFAULT 0,
  scope_hash      TEXT,
  created_at      TEXT NOT NULL,
  last_active_at  TEXT NOT NULL,
  closed_at       TEXT,
  close_reason    TEXT CHECK (close_reason IN (
                    'dismissed','session_end','idle','failed','safety','orphaned')),
  CHECK ((kind = 'lead'   AND parent_id IS NULL AND root_id = id)
      OR (kind = 'member' AND parent_id IS NOT NULL AND channel IS NULL))
);
CREATE UNIQUE INDEX sessions_channel_thread ON sessions(channel, channel_thread)
  WHERE kind = 'lead' AND status NOT IN ('closed','failed');
CREATE INDEX sessions_parent ON sessions(parent_id);

-- Tail cursors so a restarted supervisor resumes reading each agent-written file.
CREATE TABLE tail_cursors (
  session_id         TEXT PRIMARY KEY REFERENCES sessions(id),
  outbound_last_id   INTEGER NOT NULL DEFAULT 0,
  events_last_seq    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE delegations (
  id                TEXT PRIMARY KEY,
  lead_id           TEXT NOT NULL REFERENCES sessions(id),
  member_id         TEXT REFERENCES sessions(id),
  role              TEXT NOT NULL,
  task              TEXT NOT NULL,
  scope_request     TEXT NOT NULL CHECK (json_valid(scope_request)),
  policy            TEXT NOT NULL CHECK (policy IN ('allow','ask','deny')),
  approval_id       TEXT REFERENCES approvals(id),
  timeout_s         INTEGER NOT NULL,
  waiting_since     TEXT,           -- wall clock, persisted
  deadline_at       TEXT,           -- wall clock, persisted; re-armed on restart
  status            TEXT NOT NULL CHECK (status IN (
                      'requested','awaiting_approval','spawning','active',
                      'waiting_reply','dismissed','denied','failed')),
  failure_reason    TEXT CHECK (failure_reason IN (
                      'denied','limit','scope_empty','spawn_error',
                      'timeout','crashed','safety','config')),
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL
);
CREATE INDEX delegations_waiting ON delegations(deadline_at) WHERE status = 'waiting_reply';

CREATE TABLE approvals (
  id                     TEXT PRIMARY KEY,
  code                   TEXT NOT NULL,                 -- 4-char, unique among pending
  root_session_id        TEXT NOT NULL REFERENCES sessions(id),
  requesting_session_id  TEXT NOT NULL REFERENCES sessions(id),
  kind                   TEXT NOT NULL CHECK (kind IN ('tool_call','spawn')),
  payload                TEXT NOT NULL CHECK (json_valid(payload)),
  status                 TEXT NOT NULL CHECK (status IN (
                           'pending','approved','denied','timed_out','expired')),
  expires_at             TEXT NOT NULL,
  created_at             TEXT NOT NULL,
  resolved_at            TEXT,
  reissued_as            TEXT REFERENCES approvals(id)
);
CREATE UNIQUE INDEX approvals_pending_code ON approvals(code) WHERE status = 'pending';

-- Roles parsed from installed team skills.
CREATE TABLE team_roles (
  skill          TEXT NOT NULL,
  skill_version  TEXT NOT NULL,
  skill_hash     TEXT NOT NULL,
  role           TEXT NOT NULL,
  spawn_policy   TEXT NOT NULL CHECK (spawn_policy IN ('allow','ask','deny')),
  scope          TEXT NOT NULL CHECK (json_valid(scope)),   -- scope request template
  instructions   TEXT NOT NULL,                             -- path to Markdown body
  PRIMARY KEY (skill, role)
);

-- Supervisor-authored events (spawn, scope, approvals, exits, routing).
CREATE TABLE supervisor_events (
  seq              INTEGER PRIMARY KEY,
  ts               TEXT    NOT NULL,
  ts_mono          INTEGER NOT NULL,
  session_id       TEXT    NOT NULL,
  root_session_id  TEXT    NOT NULL,
  agent_role       TEXT    NOT NULL,
  type             TEXT    NOT NULL,
  data             TEXT    NOT NULL CHECK (json_valid(data))
);
CREATE INDEX supervisor_events_root ON supervisor_events(root_session_id, seq);
