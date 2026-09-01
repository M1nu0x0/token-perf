PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- offset resumes at the last response's first line; size is bytes consumed.
-- The stat size would leave a tail without a newline unread forever.
CREATE TABLE IF NOT EXISTS file_cursor (
    path        TEXT PRIMARY KEY,
    offset      INTEGER NOT NULL,
    size        INTEGER NOT NULL,
    mtime_nanos INTEGER NOT NULL,
    scanned_at  INTEGER NOT NULL,
    -- A file that grew was not necessarily appended to, and resuming into
    -- replaced content stays wrong forever. Resume only while this is unchanged.
    tail        BLOB
);

CREATE TABLE IF NOT EXISTS session (
    id                 TEXT PRIMARY KEY,
    source             TEXT NOT NULL,
    project            TEXT NOT NULL DEFAULT '',
    title              TEXT NOT NULL DEFAULT '',
    parent             TEXT,
    agent_type         TEXT,
    parent_tool_use_id TEXT,
    spawn_depth        INTEGER,
    started_at         TEXT NOT NULL DEFAULT '',
    -- The session id is a filename, so two directories can collide on one id.
    -- This scopes a full re-read's DELETE.
    path               TEXT NOT NULL DEFAULT '',
    first_seen         INTEGER NOT NULL,
    last_seen          INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS call (
    session_id     TEXT NOT NULL REFERENCES session(id) ON DELETE CASCADE,
    seq            INTEGER NOT NULL,
    message_id     TEXT NOT NULL,
    at             TEXT NOT NULL DEFAULT '',
    model          TEXT NOT NULL DEFAULT '',
    effort         TEXT,
    error          TEXT,
    skill          TEXT,
    plugin         TEXT,
    agent          TEXT,
    input          INTEGER NOT NULL DEFAULT 0,
    cache_write_5m INTEGER NOT NULL DEFAULT 0,
    cache_write_1h INTEGER NOT NULL DEFAULT 0,
    cache_read     INTEGER NOT NULL DEFAULT 0,
    output         INTEGER NOT NULL DEFAULT 0,
    thinking       INTEGER NOT NULL DEFAULT 0,
    usage_json     TEXT,
    PRIMARY KEY (session_id, seq),
    UNIQUE (session_id, message_id)
);

-- Joined to call on (session_id, message_id): seq shifts with insert order,
-- message_id holds across scan boundaries.
CREATE TABLE IF NOT EXISTS tool_use (
    session_id   TEXT NOT NULL REFERENCES session(id) ON DELETE CASCADE,
    tool_use_id  TEXT NOT NULL,
    message_id   TEXT NOT NULL,
    name         TEXT NOT NULL,
    result_chars INTEGER NOT NULL DEFAULT 0,
    input_chars  INTEGER,
    input_preview TEXT,
    PRIMARY KEY (session_id, tool_use_id)
);

-- One idempotent row per identity (message.id, else @<line offset>) rather than
-- a counter, so a re-read or a concurrent scan cannot inflate the count.
CREATE TABLE IF NOT EXISTS failure (
    session_id TEXT NOT NULL REFERENCES session(id) ON DELETE CASCADE,
    key        TEXT NOT NULL,
    PRIMARY KEY (session_id, key)
);
