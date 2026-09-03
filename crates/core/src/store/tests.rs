use super::*;
use crate::sources::claude::ClaudeCode;

const LINE_A: &str = r#"{"type":"assistant","timestamp":"t0","cwd":"/p","message":{"id":"m1","model":"opus","usage":{"input_tokens":1,"cache_read_input_tokens":100,"output_tokens":5},"content":[{"type":"tool_use","id":"t1","name":"Bash"}]}}"#;
const LINE_B: &str = r#"{"type":"user","timestamp":"t1","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"hello"}]}}"#;
const LINE_C: &str = r#"{"type":"assistant","timestamp":"t2","cwd":"/p","message":{"id":"m2","model":"opus","usage":{"input_tokens":2,"cache_read_input_tokens":200,"output_tokens":7},"content":[]}}"#;
const SPLIT_1_BILLED: &str = r#"{"type":"assistant","timestamp":"t3","message":{"id":"m3","model":"opus","usage":{"input_tokens":2,"cache_read_input_tokens":300,"output_tokens":7},"content":[{"type":"thinking"}]}}"#;
const SPLIT_2_UNBILLED_ERROR: &str = r#"{"type":"assistant","timestamp":"t3","isApiErrorMessage":true,"message":{"id":"m3","model":"opus","usage":{"input_tokens":0,"output_tokens":0},"content":[{"type":"tool_use","id":"t9","name":"Read"}]}}"#;

fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "token-perf-store-{name}-{}.jsonl",
        std::process::id()
    ))
}

#[test]
fn incremental_sync_matches_a_full_scan() {
    let path = temp("incremental");
    std::fs::write(&path, format!("{LINE_A}\n")).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    store.sync_file(&ClaudeCode, &path).unwrap();
    std::fs::write(&path, format!("{LINE_A}\n{LINE_B}\n{LINE_C}\n")).unwrap();

    store.sync_file(&ClaudeCode, &path).unwrap();

    let sessions = store.sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];
    assert_eq!(s.calls.len(), 2);
    assert_eq!(s.calls[0].usage.cache_read, 100);
    assert_eq!(s.calls[1].usage.cache_read, 200);
    assert_eq!(
        s.calls[0].tools[0].result_chars, 5,
        "a tool_result past the boundary must attach to the earlier tool_use"
    );

    std::fs::remove_file(&path).ok();
}

#[test]
fn a_truncated_file_is_rescanned_from_scratch() {
    let path = temp("truncate");
    std::fs::write(&path, format!("{LINE_A}\n{LINE_C}\n")).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    store.sync_file(&ClaudeCode, &path).unwrap();
    std::fs::write(&path, format!("{LINE_C}\n")).unwrap();

    store.sync_file(&ClaudeCode, &path).unwrap();

    let sessions = store.sessions().unwrap();
    assert_eq!(sessions[0].calls.len(), 1, "stale rows must not survive");
    assert_eq!(sessions[0].calls[0].id, "m2");

    std::fs::remove_file(&path).ok();
}

#[test]
fn a_deleted_transcript_keeps_its_archived_session() {
    let path = temp("archive");
    std::fs::write(&path, format!("{LINE_A}\n")).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    store.sync_file(&ClaudeCode, &path).unwrap();
    std::fs::remove_file(&path).unwrap();

    store.sync_file(&ClaudeCode, &path).unwrap();

    assert_eq!(
        store.sessions().unwrap().len(),
        1,
        "the session outlives its transcript"
    );
}

#[test]
fn an_unchanged_file_is_not_reread() {
    let path = temp("unchanged");
    std::fs::write(&path, format!("{LINE_A}\n")).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    let first = store.sync_file(&ClaudeCode, &path).unwrap();

    let second = store.sync_file(&ClaudeCode, &path).unwrap();

    assert_eq!(first, Some(1));
    assert_eq!(second, Some(0), "same size and mtime means skip");

    std::fs::remove_file(&path).ok();
}

#[test]
fn a_same_size_rewrite_is_rescanned() {
    let path = temp("same-size");
    std::fs::write(&path, format!("{LINE_A}\n")).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    store.sync_file(&ClaudeCode, &path).unwrap();
    std::fs::write(
        &path,
        format!("{}\n", LINE_A.replace(r#""id":"m1""#, r#""id":"m3""#)),
    )
    .unwrap();
    store
        .conn
        .execute("UPDATE file_cursor SET mtime_nanos = 0", [])
        .unwrap();

    store.sync_file(&ClaudeCode, &path).unwrap();

    let sessions = store.sessions().unwrap();
    assert_eq!(sessions[0].calls.len(), 1, "stale calls must not survive");
    assert_eq!(sessions[0].calls[0].id, "m3");

    std::fs::remove_file(&path).ok();
}

#[test]
fn the_first_timestamp_wins_as_the_session_start() {
    let path = temp("started-at");
    std::fs::write(&path, format!("{LINE_A}\n")).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    store.sync_file(&ClaudeCode, &path).unwrap();
    std::fs::write(&path, format!("{LINE_A}\n{LINE_C}\n")).unwrap();

    store.sync_file(&ClaudeCode, &path).unwrap();

    assert_eq!(store.sessions().unwrap()[0].started_at, "t0");

    std::fs::remove_file(&path).ok();
}

#[test]
fn a_full_reread_does_not_double_the_failed_call_count() {
    const API_ERROR: &str = r#"{"type":"assistant","timestamp":"t3","isApiErrorMessage":true,"message":{"id":"err","model":"opus","usage":{"input_tokens":0,"output_tokens":0}}}"#;
    let path = temp("failed-twice");
    std::fs::write(&path, format!("{LINE_A}\n{API_ERROR}\n")).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    store.sync_file(&ClaudeCode, &path).unwrap();
    store.conn.execute("DELETE FROM file_cursor", []).unwrap();

    store.sync_file(&ClaudeCode, &path).unwrap();

    let sessions = store.sessions().unwrap();
    assert_eq!(
        sessions[0].failures.len(),
        1,
        "failures must not be added twice"
    );
    assert_eq!(sessions[0].calls.len(), 1);

    std::fs::remove_file(&path).ok();
}

#[test]
fn clearing_the_cursors_makes_the_next_sync_read_the_file_again() {
    let path = temp("clear-cursors");
    std::fs::write(&path, format!("{LINE_A}\n")).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    assert_eq!(store.sync_file(&ClaudeCode, &path).unwrap(), Some(1));
    assert_eq!(store.sync_file(&ClaudeCode, &path).unwrap(), Some(0));

    store.clear_cursors().unwrap();

    assert_eq!(
        store.sync_file(&ClaudeCode, &path).unwrap(),
        Some(1),
        "an unchanged file is read again once its cursor is gone"
    );
    assert_eq!(store.sessions().unwrap()[0].calls.len(), 1);

    std::fs::remove_file(&path).ok();
}

#[test]
fn an_implausible_usage_never_reaches_the_store() {
    let broken = r#"{"type":"assistant","timestamp":"t0","message":{"id":"m9","model":"opus","usage":{"input_tokens":1,"cache_read_input_tokens":2000000000,"output_tokens":5}}}"#;
    let path = temp("clamp");
    std::fs::write(&path, format!("{LINE_A}\n{broken}\n")).unwrap();
    let mut store = Store::open_in_memory().unwrap();

    store.sync_file(&ClaudeCode, &path).unwrap();

    let sessions = store.sessions().unwrap();
    assert_eq!(
        sessions[0].calls.len(),
        1,
        "an implausible usage is not stored"
    );
    assert_eq!(sessions[0].calls[0].id, "m1");

    std::fs::remove_file(&path).ok();
}

#[test]
fn a_fresh_db_is_born_at_the_current_schema_version() {
    let store = Store::open_in_memory().unwrap();

    let version: i64 = store
        .conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
}

#[test]
fn a_scan_boundary_never_splits_one_response() {
    let path = temp("split-response");
    std::fs::write(&path, format!("{LINE_A}\n{SPLIT_1_BILLED}\n")).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    store.sync_file(&ClaudeCode, &path).unwrap();
    std::fs::write(
        &path,
        format!("{LINE_A}\n{SPLIT_1_BILLED}\n{SPLIT_2_UNBILLED_ERROR}\n"),
    )
    .unwrap();

    store.sync_file(&ClaudeCode, &path).unwrap();

    let sessions = store.sessions().unwrap();
    let m3 = &sessions[0].calls[1];
    assert_eq!(m3.id, "m3");
    assert_eq!(
        m3.error.as_deref(),
        Some("api_error"),
        "the error mark was lost"
    );
    assert_eq!(m3.usage.cache_read, 300, "billed usage must not be erased");
    assert_eq!(
        m3.tools.len(),
        1,
        "an error line's tool_use attaches to the call"
    );
    assert_eq!(sessions[0].failures.len(), 1);

    std::fs::remove_file(&path).ok();
}

#[test]
fn a_rewound_failure_is_not_counted_twice_when_the_file_grows_again() {
    let path = temp("split-response-regrow");
    std::fs::write(&path, format!("{LINE_A}\n{SPLIT_1_BILLED}\n")).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    store.sync_file(&ClaudeCode, &path).unwrap();
    std::fs::write(
        &path,
        format!("{LINE_A}\n{SPLIT_1_BILLED}\n{SPLIT_2_UNBILLED_ERROR}\n"),
    )
    .unwrap();
    store.sync_file(&ClaudeCode, &path).unwrap();

    std::fs::write(
        &path,
        format!("{LINE_A}\n{SPLIT_1_BILLED}\n{SPLIT_2_UNBILLED_ERROR}\n{LINE_B}\n"),
    )
    .unwrap();
    store.sync_file(&ClaudeCode, &path).unwrap();

    assert_eq!(
        store.sessions().unwrap()[0].failures.len(),
        1,
        "a re-read failure is not added again"
    );

    std::fs::remove_file(&path).ok();
}

#[test]
fn a_file_replaced_by_larger_content_is_rescanned_from_scratch() {
    let path = temp("replaced-larger");
    std::fs::write(&path, format!("{LINE_A}\n{LINE_C}\n")).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    store.sync_file(&ClaudeCode, &path).unwrap();
    let other = LINE_A
        .replace(r#""id":"m1""#, r#""id":"m7""#)
        .replace(r#""name":"Bash""#, r#""name":"Grep""#);
    std::fs::write(&path, format!("{other}\n{other}\n{LINE_C}\n")).unwrap();

    store.sync_file(&ClaudeCode, &path).unwrap();

    let ids: Vec<_> = store.sessions().unwrap()[0]
        .calls
        .iter()
        .map(|c| c.id.clone())
        .collect();
    assert_eq!(ids, ["m7", "m2"], "must not resume from the old offset");

    std::fs::remove_file(&path).ok();
}

#[test]
fn a_file_with_an_unfinished_tail_is_not_marked_as_fully_read() {
    let path = temp("unfinished-tail");
    std::fs::write(&path, format!("{LINE_A}\n{LINE_C}")).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    store.sync_file(&ClaudeCode, &path).unwrap();

    let again = store.sync_file(&ClaudeCode, &path).unwrap();

    let size: i64 = store
        .conn
        .query_row("SELECT size FROM file_cursor", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        size,
        LINE_A.len() as i64 + 1,
        "only consumed bytes are recorded"
    );
    assert_eq!(again, Some(1), "not treated as fully read; retried");

    std::fs::remove_file(&path).ok();
}

#[test]
fn a_repeated_error_line_at_a_scan_boundary_matches_a_full_scan() {
    let path = temp("boundary-error");
    let mut store = Store::open_in_memory().unwrap();
    for lines in [
        format!("{LINE_A}\n{SPLIT_1_BILLED}\n"),
        format!("{LINE_A}\n{SPLIT_1_BILLED}\n{SPLIT_2_UNBILLED_ERROR}\n"),
        format!("{LINE_A}\n{SPLIT_1_BILLED}\n{SPLIT_2_UNBILLED_ERROR}\n{LINE_B}\n"),
        format!("{LINE_A}\n{SPLIT_1_BILLED}\n{SPLIT_2_UNBILLED_ERROR}\n{LINE_B}\n{LINE_C}\n"),
    ] {
        std::fs::write(&path, lines).unwrap();
        store.sync_file(&ClaudeCode, &path).unwrap();
    }

    let mut fresh = Store::open_in_memory().unwrap();
    fresh.sync_file(&ClaudeCode, &path).unwrap();

    let (incr, full) = (&store.sessions().unwrap()[0], &fresh.sessions().unwrap()[0]);
    assert_eq!(
        incr.failures, full.failures,
        "failure keys must not depend on fragmentation"
    );
    assert_eq!(incr.failures.len(), 1);
    assert_eq!(incr.calls.len(), full.calls.len());

    let m3 = incr.calls.iter().find(|c| c.id == "m3").unwrap();
    assert!(
        m3.error.is_some(),
        "the rewound range's error must reach call.error, not just its failure count"
    );

    std::fs::remove_file(&path).ok();
}

#[test]
fn an_unbilled_error_line_keeps_its_session_visible() {
    const ONLY_ERROR: &str = r#"{"type":"assistant","timestamp":"t0","cwd":"/p","isApiErrorMessage":true,"message":{"id":"err","model":"opus"}}"#;
    let path = temp("failures-only");
    std::fs::write(&path, format!("{ONLY_ERROR}\n")).unwrap();
    let mut store = Store::open_in_memory().unwrap();

    store.sync_file(&ClaudeCode, &path).unwrap();

    let sessions = store.sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].failures.len(), 1);

    std::fs::remove_file(&path).ok();
}

#[test]
fn a_second_file_with_the_same_stem_does_not_wipe_the_first() {
    let dir_a = std::env::temp_dir().join(format!("token-perf-stem-a-{}", std::process::id()));
    let dir_b = std::env::temp_dir().join(format!("token-perf-stem-b-{}", std::process::id()));
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();
    let (a, b) = (dir_a.join("dup.jsonl"), dir_b.join("dup.jsonl"));
    std::fs::write(&a, format!("{LINE_A}\n")).unwrap();
    std::fs::write(&b, format!("{LINE_C}\n")).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    store.sync_file(&ClaudeCode, &a).unwrap();

    store.sync_file(&ClaudeCode, &b).unwrap();

    let ids: Vec<_> = store.sessions().unwrap()[0]
        .calls
        .iter()
        .map(|c| c.id.clone())
        .collect();
    assert!(
        ids.contains(&"m1".to_string()),
        "another file's rows must not be wiped"
    );

    std::fs::remove_dir_all(&dir_a).ok();
    std::fs::remove_dir_all(&dir_b).ok();
}

#[test]
fn two_processes_syncing_the_same_file_do_not_double_the_failures() {
    let dir = std::env::temp_dir().join(format!("token-perf-concurrent-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.jsonl");
    std::fs::write(
        &path,
        format!("{LINE_A}\n{SPLIT_1_BILLED}\n{SPLIT_2_UNBILLED_ERROR}\n"),
    )
    .unwrap();
    let db = dir.join("db.sqlite");
    std::fs::remove_file(&db).ok();
    Store::open(&db).unwrap();

    std::thread::scope(|scope| {
        for _ in 0..2 {
            scope.spawn(|| {
                let mut store = Store::open(&db).unwrap();
                for _ in 0..5 {
                    store.sync_file(&ClaudeCode, &path).unwrap();
                }
            });
        }
    });

    let store = Store::open(&db).unwrap();
    let sessions = store.sessions().unwrap();
    assert_eq!(
        sessions[0].failures.len(),
        1,
        "keys, not deltas, so overlap cannot inflate"
    );
    assert_eq!(sessions[0].calls.len(), 2);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_scan_boundary_between_the_marker_and_its_call_keeps_the_flag() {
    const BOUNDARY: &str =
        r#"{"type":"system","subtype":"compact_boundary","compactMetadata":{"trigger":"manual"}}"#;
    let path = temp("compact-boundary");
    std::fs::write(&path, format!("{LINE_A}\n{BOUNDARY}\n{LINE_C}\n")).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    store.sync_file(&ClaudeCode, &path).unwrap();
    std::fs::write(&path, format!("{LINE_A}\n{BOUNDARY}\n{LINE_C}\n{LINE_B}\n")).unwrap();

    store.sync_file(&ClaudeCode, &path).unwrap();

    let calls = &store.sessions().unwrap()[0].calls;
    assert!(!calls[0].compacted);
    assert!(
        calls[1].compacted,
        "the flag must survive the re-read even though the cursor rewound past the marker"
    );

    std::fs::remove_file(&path).ok();
}

#[test]
fn an_upgraded_db_gains_the_compacted_column() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        &include_str!("schema.sql").replace("compacted      INTEGER NOT NULL DEFAULT 0,", ""),
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 2).unwrap();

    let store = Store::from_connection(conn).unwrap();

    let version: i64 = store
        .conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
    store
        .conn
        .query_row("SELECT compacted FROM call LIMIT 1", [], |r| {
            r.get::<_, i64>(0)
        })
        .optional()
        .unwrap();
}

#[test]
fn a_setting_survives_a_rewrite_and_is_missing_until_written() {
    let store = Store::open_in_memory().unwrap();

    assert_eq!(store.setting("lang").unwrap(), None);
    store.set_setting("lang", "ko").unwrap();
    assert_eq!(store.setting("lang").unwrap().as_deref(), Some("ko"));
    store.set_setting("lang", "ja").unwrap();
    assert_eq!(store.setting("lang").unwrap().as_deref(), Some("ja"));
}
