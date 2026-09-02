//! Transcripts are deleted after 30 days by default (`cleanupPeriodDays`). Keeping our own
//! copy is why this module exists; skipping rescans is a side effect.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::common::model::{Call, Session, ToolUse, Usage};
use crate::sources::session_id;

const SCHEMA_VERSION: i64 = 3;

const TAIL_LEN: u64 = 64;

pub struct Store {
    conn: Connection,
}

struct Cursor {
    offset: u64,
    consumed: u64,
    mtime: i64,
    tail: Option<Vec<u8>>,
}

#[derive(Debug, Default)]
pub struct ScanReport {
    pub files_seen: usize,
    pub files_read: usize,
    pub files_skipped: usize,
    pub calls_written: usize,
}

impl Store {
    pub fn open_default() -> rusqlite::Result<Self> {
        let path = default_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        Self::open(&path)
    }

    fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::from_connection(Connection::open(path)?)
    }

    #[cfg(test)]
    fn open_in_memory() -> rusqlite::Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(mut conn: Connection) -> rusqlite::Result<Self> {
        // Web UI and CLI can write to the same DB; wait instead of failing outright.
        conn.busy_timeout(Duration::from_secs(5))?;
        // Cannot assume CASCADE is on by default; set it per connection.
        conn.pragma_update(None, "foreign_keys", true)?;
        let fresh: bool = conn.query_row(
            "SELECT count(*) = 0 FROM sqlite_master WHERE type = 'table'",
            [],
            |row| row.get(0),
        )?;
        conn.execute_batch(include_str!("schema.sql"))?;
        if fresh {
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        } else {
            migrate(&mut conn)?;
        }
        Ok(Self { conn })
    }

    pub fn sync(&mut self) -> rusqlite::Result<ScanReport> {
        let mut report = ScanReport::default();
        for source in crate::sources::all() {
            for path in source.discover() {
                report.files_seen += 1;
                // A DB error stops the scan: swallowing it would pass off a
                // half-written store as complete.
                match self.sync_file(source.as_ref(), &path)? {
                    None => report.files_skipped += 1,
                    Some(0) => {}
                    Some(written) => {
                        report.files_read += 1;
                        report.calls_written += written;
                    }
                }
            }
        }
        Ok(report)
    }

    /// `None` if the file could not be read; the scan goes on.
    pub(crate) fn sync_file(
        &mut self,
        source: &dyn crate::sources::Source,
        path: &Path,
    ) -> rusqlite::Result<Option<usize>> {
        let Ok(meta) = std::fs::metadata(path) else {
            return Ok(None);
        };
        let size = meta.len();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos() as i64);

        let key = path.to_string_lossy().into_owned();
        // Reading the cursor outside the transaction lets two scans see the same
        // cursor and write the same range. Immediate queues them here.
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let cursor = tx
            .query_row(
                "SELECT offset, size, mtime_nanos, tail FROM file_cursor WHERE path = ?1",
                params![key],
                |row| {
                    Ok(Cursor {
                        offset: row.get::<_, i64>(0)? as u64,
                        consumed: row.get::<_, i64>(1)? as u64,
                        mtime: row.get(2)?,
                        tail: row.get(3)?,
                    })
                },
            )
            .optional()?;

        let mut handle = std::fs::File::open(path).ok();
        let mut offset = 0u64;
        if let Some(saved) = cursor {
            if saved.consumed == size && saved.mtime == mtime {
                return Ok(Some(0));
            }
            if saved.offset > 0
                && size > saved.consumed
                && handle.as_mut().and_then(|f| read_tail(f, saved.offset)) == saved.tail
            {
                offset = saved.offset;
            }
        }

        let Ok(fragment) = source.load_from(path, offset) else {
            return Ok(None);
        };
        let tail = handle
            .as_mut()
            .and_then(|f| read_tail(f, fragment.resume_at));

        if offset == 0 {
            tx.execute(
                "DELETE FROM session WHERE id = ?1 AND path IN ('', ?2)",
                params![session_id(path), key],
            )?;
        }
        let written = write_session(&tx, &fragment.session, &key)?;
        // Committing the cursor apart from the data would mark an unread range as
        // read if we die in between.
        tx.execute(
            "INSERT INTO file_cursor (path, offset, size, mtime_nanos, scanned_at, tail)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(path) DO UPDATE SET
                 offset = excluded.offset, size = excluded.size,
                 mtime_nanos = excluded.mtime_nanos, scanned_at = excluded.scanned_at,
                 tail = excluded.tail",
            params![
                key,
                fragment.resume_at as i64,
                fragment.consumed as i64,
                mtime,
                now_millis(),
                tail,
            ],
        )?;
        tx.commit()?;

        Ok(Some(written))
    }

    pub fn sessions(&self) -> rusqlite::Result<Vec<Session>> {
        // One snapshot for all four SELECTs: a concurrent sync would otherwise yield
        // a session with new calls and old failures.
        let tx = self.conn.unchecked_transaction()?;
        let mut stmt = tx.prepare(
            "SELECT id, source, project, title, parent, agent_type, parent_tool_use_id,
                    spawn_depth, started_at FROM session",
        )?;
        let mut sessions: Vec<Session> = stmt
            .query_map([], |row| {
                Ok(Session {
                    id: row.get(0)?,
                    source: row.get(1)?,
                    project: row.get(2)?,
                    title: row.get(3)?,
                    title_is_meta: false,
                    parent: row.get(4)?,
                    agent_type: row.get(5)?,
                    parent_tool_use_id: row.get(6)?,
                    spawn_depth: row.get(7)?,
                    started_at: row.get(8)?,
                    failures: Vec::new(),
                    calls: Vec::new(),
                    orphan_results: Vec::new(),
                    orphan_errors: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;

        let mut calls = tx.prepare(
            "SELECT c.session_id, c.message_id, c.at, c.model, c.error, c.compacted,
                    c.input, c.cache_write_5m, c.cache_write_1h, c.cache_read, c.output, c.thinking
             FROM call c ORDER BY c.session_id, c.seq",
        )?;
        let mut by_session: std::collections::HashMap<String, Vec<Call>> =
            std::collections::HashMap::new();
        for row in calls.query_map([], |row| {
            let write_5m: i64 = row.get(7)?;
            let write_1h: i64 = row.get(8)?;
            Ok((
                row.get::<_, String>(0)?,
                Call {
                    id: row.get(1)?,
                    at: row.get(2)?,
                    model: row.get(3)?,
                    error: row.get(4)?,
                    compacted: row.get(5)?,
                    // Stored, but nothing reads them back yet.
                    effort: None,
                    skill: None,
                    plugin: None,
                    agent: None,
                    usage_json: None,
                    usage: Usage {
                        input: row.get::<_, i64>(6)? as u64,
                        cache_write_5m: write_5m as u64,
                        cache_write_1h: write_1h as u64,
                        cache_read: row.get::<_, i64>(9)? as u64,
                        output: row.get::<_, i64>(10)? as u64,
                        thinking: row.get::<_, i64>(11)? as u64,
                    },
                    tools: Vec::new(),
                },
            ))
        })? {
            let (session_id, call) = row?;
            by_session.entry(session_id).or_default().push(call);
        }

        let mut tools = tx.prepare(
            "SELECT session_id, message_id, tool_use_id, name, result_chars
             FROM tool_use",
        )?;
        let mut by_message: std::collections::HashMap<(String, String), Vec<ToolUse>> =
            std::collections::HashMap::new();
        for row in tools.query_map([], |row| {
            Ok((
                (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                ToolUse {
                    id: row.get(2)?,
                    name: row.get(3)?,
                    result_chars: row.get::<_, i64>(4)? as usize,
                    input_chars: 0,
                    input_preview: String::new(),
                },
            ))
        })? {
            let (key, tool) = row?;
            by_message.entry(key).or_default().push(tool);
        }

        let mut fails = tx.prepare("SELECT session_id, key FROM failure")?;
        let mut by_failure: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for row in fails.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            let (session_id, key) = row?;
            by_failure.entry(session_id).or_default().push(key);
        }

        for session in &mut sessions {
            session.failures = by_failure.remove(&session.id).unwrap_or_default();
            let mut session_calls = by_session.remove(&session.id).unwrap_or_default();
            for call in &mut session_calls {
                if let Some(tools) = by_message.remove(&(session.id.clone(), call.id.clone())) {
                    call.tools = tools;
                }
            }
            session.calls = session_calls;
        }
        // A session where nothing succeeded is exactly the one worth showing.
        sessions.retain(|s| !s.calls.is_empty() || !s.failures.is_empty());
        Ok(sessions)
    }
}

fn write_session(tx: &Connection, session: &Session, path: &str) -> rusqlite::Result<usize> {
    let now = now_millis();

    tx.execute(
        "INSERT INTO session (id, source, project, title, parent, agent_type,
             parent_tool_use_id, spawn_depth, started_at, path, first_seen, last_seen)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11)
         ON CONFLICT(id) DO UPDATE SET
             -- Keep the first value seen: incremental fragments carry no header.
             project      = CASE WHEN project <> '' THEN project ELSE excluded.project END,
             -- Except the real title (aiTitle), which arrives mid-conversation. The
             -- sidecar placeholder (?12) only fills a blank.
             title        = CASE WHEN excluded.title = '' OR (?12 AND title <> '')
                                 THEN title ELSE excluded.title END,
             started_at   = CASE WHEN started_at <> '' THEN started_at ELSE excluded.started_at END,
             parent       = COALESCE(excluded.parent, parent),
             agent_type   = COALESCE(excluded.agent_type, agent_type),
             parent_tool_use_id = COALESCE(excluded.parent_tool_use_id, parent_tool_use_id),
             spawn_depth  = COALESCE(excluded.spawn_depth, spawn_depth),
             path         = excluded.path,
             last_seen    = excluded.last_seen",
        params![
            session.id,
            session.source,
            session.project,
            session.title,
            session.parent,
            session.agent_type,
            session.parent_tool_use_id,
            session.spawn_depth,
            session.started_at,
            path,
            now,
            session.title_is_meta,
        ],
    )?;

    for key in &session.failures {
        tx.prepare_cached("INSERT OR IGNORE INTO failure (session_id, key) VALUES (?1, ?2)")?
            .execute(params![session.id, key])?;
    }

    let mut next_seq: i64 = tx.query_row(
        "SELECT COALESCE(MAX(seq), -1) + 1 FROM call WHERE session_id = ?1",
        params![session.id],
        |row| row.get(0),
    )?;

    let mut written = 0;
    for call in &session.calls {
        let existing: Option<i64> = tx
            .query_row(
                "SELECT seq FROM call WHERE session_id = ?1 AND message_id = ?2",
                params![session.id, call.id],
                |row| row.get(0),
            )
            .optional()?;
        let seq = existing.unwrap_or_else(|| {
            let seq = next_seq;
            next_seq += 1;
            seq
        });

        let u = call.usage;
        tx.prepare_cached(
            "INSERT INTO call (session_id, seq, message_id, at, model, effort, error,
                 skill, plugin, agent,
                 input, cache_write_5m, cache_write_1h, cache_read, output, thinking,
                 compacted, usage_json)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)
             ON CONFLICT(session_id, message_id) DO UPDATE SET
                 at = excluded.at, model = excluded.model,
                 -- A later healthy line must not blank the error or the attribution.
                 error = COALESCE(excluded.error, error),
                 effort = COALESCE(excluded.effort, effort),
                 skill = COALESCE(excluded.skill, skill),
                 plugin = COALESCE(excluded.plugin, plugin),
                 agent = COALESCE(excluded.agent, agent),
                 input = excluded.input, cache_write_5m = excluded.cache_write_5m,
                 cache_write_1h = excluded.cache_write_1h, cache_read = excluded.cache_read,
                 output = excluded.output, thinking = excluded.thinking,
                 -- The marker and the call it flags can fall in different fragments.
                 compacted = MAX(compacted, excluded.compacted),
                 usage_json = excluded.usage_json",
        )?
        .execute(params![
            session.id,
            seq,
            call.id,
            call.at,
            call.model,
            call.effort,
            call.error,
            call.skill,
            call.plugin,
            call.agent,
            u.input as i64,
            u.cache_write_5m as i64,
            u.cache_write_1h as i64,
            u.cache_read as i64,
            u.output as i64,
            u.thinking as i64,
            call.compacted,
            call.usage_json,
        ])?;
        written += 1;

        for tool in &call.tools {
            tx.prepare_cached(
                "INSERT INTO tool_use (session_id, tool_use_id, message_id, name,
                     result_chars, input_chars, input_preview)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)
                 ON CONFLICT(session_id, tool_use_id) DO UPDATE SET
                     result_chars = MAX(result_chars, excluded.result_chars),
                     -- A repeated block without input must not erase what we stored.
                     input_chars = COALESCE(NULLIF(excluded.input_chars, 0), input_chars),
                     input_preview = COALESCE(NULLIF(excluded.input_preview, ''), input_preview)",
            )?
            .execute(params![
                session.id,
                tool.id,
                call.id,
                tool.name,
                tool.result_chars as i64,
                tool.input_chars as i64,
                tool.input_preview,
            ])?;
        }
    }

    for (message_id, kind) in &session.orphan_errors {
        tx.prepare_cached(
            "UPDATE call SET error = COALESCE(error, ?3)
             WHERE session_id = ?1 AND message_id = ?2",
        )?
        .execute(params![session.id, message_id, kind])?;
    }

    for (tool_use_id, chars) in &session.orphan_results {
        tx.execute(
            "UPDATE tool_use SET result_chars = ?3
             WHERE session_id = ?1 AND tool_use_id = ?2 AND result_chars = 0",
            params![session.id, tool_use_id, *chars as i64],
        )?;
    }

    Ok(written)
}

fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= SCHEMA_VERSION {
        return Ok(());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if version < 3 {
        // DEFAULT 0 leaves old rows to the fallback; no re-scan needed.
        tx.execute(
            "ALTER TABLE call ADD COLUMN compacted INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()
}

fn read_tail(file: &mut std::fs::File, offset: u64) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};

    let start = offset.saturating_sub(TAIL_LEN);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = vec![0u8; (offset - start) as usize];
    file.read_exact(&mut buf).ok()?;
    Some(buf)
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn default_path() -> PathBuf {
    // Empty or relative counts as unset (XDG), or the DB scatters across cwds.
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| crate::sources::home().map(|h| h.join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("token-perf/token-perf.db")
}

#[cfg(test)]
mod tests;
