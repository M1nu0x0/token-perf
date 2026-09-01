use super::*;

const ASSISTANT: &str = r#"{"type":"assistant","timestamp":"t0","cwd":"/p","message":{"id":"m1","model":"opus","usage":{"input_tokens":1,"cache_read_input_tokens":100,"output_tokens":5},"content":[{"type":"tool_use","id":"t1","name":"Bash"}]}}"#;
const EMPTY_STUB: &str = r#"{"type":"assistant","timestamp":"t0","message":{"id":"stub","model":"opus","usage":{"input_tokens":0,"output_tokens":0}}}"#;
const TOOL_RESULT: &str = r#"{"type":"user","timestamp":"t1","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"hello"}]}}"#;

struct Transcript(PathBuf);

impl Transcript {
    fn new(name: &str, lines: &[&str]) -> Self {
        let path =
            std::env::temp_dir().join(format!("token-perf-{name}-{}.jsonl", std::process::id()));
        // Without a trailing newline the last line reads as still being written.
        let mut body = lines.join("\n");
        body.push('\n');
        std::fs::write(&path, body).expect("write temp file");
        Self(path)
    }

    fn load(&self) -> Session {
        ClaudeCode.load(&self.0).expect("load transcript")
    }
}

impl Drop for Transcript {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).ok();
    }
}

#[test]
fn truncates_the_input_preview_on_a_char_boundary() {
    // 300 Hangul chars, so the preview cut lands mid-multibyte.
    let arg = "가".repeat(300);
    let line = format!(
        r#"{{"type":"assistant","timestamp":"t0","message":{{"id":"m1","model":"opus","usage":{{"input_tokens":1,"output_tokens":1}},"content":[{{"type":"tool_use","id":"t1","name":"Read","input":{{"p":"{arg}"}}}}]}}}}"#
    );
    let file = Transcript::new("preview", &[&line]);

    let session = file.load();

    let tool = &session.calls[0].tools[0];
    assert_eq!(tool.input_chars, r#"{"p":""#.len() + 300 + 2);
    assert_eq!(tool.input_preview.chars().count(), 200);
    assert!(tool.input_preview.starts_with(r#"{"p":"가"#));
}

#[test]
fn folds_duplicate_stream_lines() {
    let file = Transcript::new("duplicates", &[ASSISTANT, ASSISTANT, ASSISTANT]);

    let session = file.load();

    assert_eq!(session.calls.len(), 1);
    assert_eq!(session.calls[0].usage.cache_read, 100);
}

#[test]
fn collects_tools_from_every_line_of_one_response() {
    const THINKING_LINE: &str = r#"{"type":"assistant","timestamp":"t0","cwd":"/p","message":{"id":"m1","model":"opus","usage":{"input_tokens":1,"cache_read_input_tokens":100,"output_tokens":5},"content":[{"type":"thinking"}]}}"#;
    let file = Transcript::new("split-blocks", &[THINKING_LINE, ASSISTANT]);

    let session = file.load();

    assert_eq!(session.calls.len(), 1, "usage is counted once");
    assert_eq!(
        session.calls[0].usage.cache_read, 100,
        "usage must not be added twice"
    );
    assert_eq!(
        session.calls[0].tools.len(),
        1,
        "a later line's tool_use joins the same call"
    );
    assert_eq!(session.calls[0].tools[0].name, "Bash");
}

#[test]
fn drops_empty_usage_stubs() {
    let file = Transcript::new("stubs", &[ASSISTANT, EMPTY_STUB]);

    let session = file.load();

    assert_eq!(session.calls.len(), 1, "a stub is not a call");
}

#[test]
fn links_tool_results_to_the_call_that_asked_for_them() {
    let file = Transcript::new("tool-results", &[ASSISTANT, TOOL_RESULT]);

    let session = file.load();

    assert_eq!(session.calls[0].tools[0].name, "Bash");
    assert!(session.calls[0].tools[0].result_chars > 0);
}

#[test]
fn takes_the_session_name_from_the_ai_title_line() {
    let file = Transcript::new(
        "ai-title",
        &[
            ASSISTANT,
            r#"{"type":"ai-title","sessionId":"m1","aiTitle":"토큰 낭비 조사"}"#,
        ],
    );

    let session = file.load();

    assert_eq!(session.title, "토큰 낭비 조사");
}

#[test]
fn reads_the_parent_session_from_the_subagent_path() {
    let parent = format!("tp-parent-{}", std::process::id());
    let root = std::env::temp_dir().join(&parent);
    let dir = root.join("subagents");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("agent-abc.jsonl");
    std::fs::write(&path, ASSISTANT).unwrap();

    let session = ClaudeCode.load(&path).unwrap();

    assert_eq!(session.parent.as_deref(), Some(parent.as_str()));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_top_level_session_has_no_parent() {
    let file = Transcript::new("top-level", &[ASSISTANT]);

    let session = file.load();

    assert_eq!(session.parent, None);
}

#[test]
fn later_lines_of_one_response_own_the_final_usage() {
    const PARTIAL: &str = r#"{"type":"assistant","timestamp":"t0","message":{"id":"m1","model":"opus","usage":{"input_tokens":1,"cache_read_input_tokens":100,"output_tokens":40},"content":[{"type":"thinking"}]}}"#;
    const FINAL: &str = r#"{"type":"assistant","timestamp":"t0","message":{"id":"m1","model":"opus","usage":{"input_tokens":1,"cache_read_input_tokens":100,"output_tokens":900},"content":[{"type":"tool_use","id":"t9","name":"Read"}]}}"#;
    let file = Transcript::new("cumulative", &[PARTIAL, FINAL]);

    let session = file.load();

    assert_eq!(session.calls.len(), 1);
    assert_eq!(
        session.calls[0].usage.output, 900,
        "the last line holds the final usage"
    );
    assert_eq!(session.calls[0].tools.len(), 1);
}

#[test]
fn does_not_count_a_tool_use_twice_when_a_snapshot_repeats_it() {
    let file = Transcript::new("repeat-tool", &[ASSISTANT, ASSISTANT]);

    let session = file.load();

    assert_eq!(
        session.calls[0].tools.len(),
        1,
        "the same tool_use.id counts once"
    );
}

#[test]
fn splits_cache_writes_by_ttl() {
    const TTL_LINE: &str = r#"{"type":"assistant","timestamp":"t0","message":{"id":"m1","model":"opus","usage":{"input_tokens":1,"cache_creation_input_tokens":300,"cache_creation":{"ephemeral_5m_input_tokens":100,"ephemeral_1h_input_tokens":200},"output_tokens":5}}}"#;
    let file = Transcript::new("cache-ttl", &[TTL_LINE]);

    let session = file.load();

    assert_eq!(session.calls[0].usage.cache_write_5m, 100);
    assert_eq!(session.calls[0].usage.cache_write_1h, 200);
    assert_eq!(
        session.calls[0].usage.cache_write(),
        300,
        "the sum matches the flat field"
    );
}

#[test]
fn falls_back_to_the_flat_cache_field_as_five_minute() {
    let file = Transcript::new("cache-flat", &[ASSISTANT]);

    let session = file.load();

    assert_eq!(session.calls[0].usage.cache_write_5m, 0);
    assert_eq!(session.calls[0].usage.cache_write_1h, 0);
}

#[test]
fn counts_failed_calls_without_billing_them() {
    const API_ERROR: &str = r#"{"type":"assistant","timestamp":"t0","isApiErrorMessage":true,"apiErrorStatus":404,"message":{"id":"err","model":"opus","usage":{"input_tokens":0,"output_tokens":0}}}"#;
    let file = Transcript::new("api-error", &[ASSISTANT, API_ERROR]);

    let session = file.load();

    assert_eq!(
        session.calls.len(),
        1,
        "a failure that spent no tokens is not a call"
    );
    assert_eq!(session.failures, ["err"]);
}

#[test]
fn an_error_line_merges_into_the_call_it_repeats() {
    const ERROR_REPEAT: &str = r#"{"type":"assistant","timestamp":"t0","isApiErrorMessage":true,"message":{"id":"m1","model":"opus","usage":{"input_tokens":0,"output_tokens":0},"content":[{"type":"tool_use","id":"t2","name":"Read"}]}}"#;
    let file = Transcript::new("error-repeat", &[ASSISTANT, ERROR_REPEAT]);

    let session = file.load();

    assert_eq!(
        session.calls.len(),
        1,
        "one response must not become two calls"
    );
    assert_eq!(session.calls[0].error.as_deref(), Some("api_error"));
    assert_eq!(
        session.calls[0].usage.cache_read, 100,
        "a zero-usage error line must not erase real usage"
    );
    assert_eq!(
        session.calls[0].tools.len(),
        2,
        "an error line's tool_use is collected too"
    );
    assert_eq!(session.failures, ["m1"]);
}

#[test]
fn counts_one_failure_for_repeated_unbilled_error_lines() {
    const UNBILLED_ERROR: &str = r#"{"type":"assistant","timestamp":"t0","isApiErrorMessage":true,"message":{"id":"err","model":"opus","usage":{"input_tokens":0,"output_tokens":0}}}"#;
    let file = Transcript::new("unbilled-error-repeat", &[UNBILLED_ERROR, UNBILLED_ERROR]);

    let session = file.load();

    assert!(session.calls.is_empty(), "an unbilled line is not a call");
    assert_eq!(
        session.failures,
        ["err", "err"],
        "the same key twice; folding is the store's PK job"
    );
}

#[test]
fn counts_tool_result_content_in_characters() {
    let file = Transcript::new("result-chars", &[ASSISTANT, TOOL_RESULT]);

    let session = file.load();

    assert_eq!(session.calls[0].tools[0].result_chars, 5);
}

#[test]
fn reads_the_subagent_meta_sidecar() {
    let dir = std::env::temp_dir().join(format!("tp-meta-{}/subagents", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("agent-xyz.jsonl"), ASSISTANT).unwrap();
    std::fs::write(
        dir.join("agent-xyz.meta.json"),
        r#"{"agentType":"general-purpose","description":"토큰 낭비 조사","toolUseId":"toolu_01ABC","spawnDepth":1,"model":"opus"}"#,
    )
    .unwrap();

    let session = ClaudeCode.load(&dir.join("agent-xyz.jsonl")).unwrap();

    assert_eq!(session.agent_type.as_deref(), Some("general-purpose"));
    assert_eq!(session.parent_tool_use_id.as_deref(), Some("toolu_01ABC"));
    assert_eq!(session.spawn_depth, Some(1));
    assert_eq!(
        session.title, "토큰 낭비 조사",
        "with no title, the description is used"
    );

    std::fs::remove_dir_all(std::env::temp_dir().join(format!("tp-meta-{}", std::process::id())))
        .ok();
}

#[test]
fn leaves_a_half_written_tail_for_the_next_scan() {
    let path =
        std::env::temp_dir().join(format!("token-perf-partial-{}.jsonl", std::process::id()));
    let partial = format!("{ASSISTANT}\n{{\"type\":\"assist");
    std::fs::write(&path, &partial).unwrap();

    let f = ClaudeCode.load_from(&path, 0).unwrap();

    assert_eq!(f.session.calls.len(), 1, "only complete lines are read");
    assert_eq!(
        f.consumed,
        ASSISTANT.len() as u64 + 1,
        "the cursor stops at the last complete line"
    );

    std::fs::remove_file(&path).ok();
}

#[test]
fn resumes_from_the_cursor_without_redoing_work() {
    let path = std::env::temp_dir().join(format!(
        "token-perf-incremental-{}.jsonl",
        std::process::id()
    ));
    std::fs::write(&path, format!("{ASSISTANT}\n")).unwrap();
    let first = ClaudeCode.load_from(&path, 0).unwrap();
    std::fs::write(&path, format!("{ASSISTANT}\n{TOOL_RESULT}\n")).unwrap();

    let second = ClaudeCode.load_from(&path, first.consumed).unwrap().session;

    assert_eq!(first.session.calls.len(), 1);
    assert!(
        second.calls.is_empty(),
        "already-read lines are not read again"
    );
    assert_eq!(
        second.orphan_results.len(),
        1,
        "a tool_result whose pair is behind is handed off as an orphan"
    );

    std::fs::remove_file(&path).ok();
}

#[test]
fn rewinds_the_cursor_to_the_start_of_the_last_response() {
    const SECOND: &str = r#"{"type":"assistant","timestamp":"t2","message":{"id":"m2","model":"opus","usage":{"input_tokens":1,"output_tokens":1}}}"#;
    let file = Transcript::new("rewind", &[ASSISTANT, SECOND]);

    let f = ClaudeCode.load_from(&file.0, 0).unwrap();

    assert_eq!(f.consumed, (ASSISTANT.len() + SECOND.len() + 2) as u64);
    assert_eq!(f.resume_at, ASSISTANT.len() as u64 + 1);
    assert_eq!(
        f.session.calls.len(),
        2,
        "rewinding still yields every call"
    );
}

#[test]
fn counts_an_error_line_that_carries_no_usage() {
    const NO_USAGE: &str = r#"{"type":"assistant","timestamp":"t1","isApiErrorMessage":true,"message":{"id":"err","model":"opus"}}"#;
    let file = Transcript::new("error-no-usage", &[ASSISTANT, NO_USAGE]);

    let session = file.load();

    assert_eq!(session.calls.len(), 1);
    assert_eq!(session.failures, ["err"]);
}

#[test]
fn keeps_the_fields_it_can_parse_when_one_drifts() {
    const DRIFT: &str = r#"{"type":"assistant","timestamp":"t0","message":{"id":"m1","model":"opus","usage":{"input_tokens":12.7,"cache_read_input_tokens":100,"output_tokens":"5","cache_creation":null}}}"#;
    let file = Transcript::new("usage-drift", &[DRIFT]);

    let session = file.load();

    assert_eq!(session.calls.len(), 1, "the response must not vanish");
    assert_eq!(session.calls[0].usage.input, 12, "a float is cast");
    assert_eq!(
        session.calls[0].usage.cache_read, 100,
        "intact fields survive"
    );
    assert_eq!(
        session.calls[0].usage.output, 0,
        "only the unreadable field is 0"
    );
}

#[test]
fn drops_a_usage_that_reports_an_impossible_field() {
    const SPIKE: &str = r#"{"type":"assistant","timestamp":"t1","message":{"id":"m9","model":"opus","usage":{"input_tokens":1,"cache_read_input_tokens":2000000000,"output_tokens":5}}}"#;
    let file = Transcript::new("spike", &[ASSISTANT, SPIKE]);

    let session = file.load();

    assert_eq!(
        session.calls.len(),
        1,
        "an implausible usage is treated as a stub"
    );
    assert_eq!(session.calls[0].id, "m1");
}

#[test]
fn keeps_lines_without_an_id_apart() {
    const NO_ID: &str = r#"{"type":"assistant","timestamp":"t0","message":{"model":"opus","usage":{"input_tokens":7,"output_tokens":3}}}"#;
    let file = Transcript::new("no-id", &[NO_ID, NO_ID]);

    let session = file.load();

    assert_eq!(
        session.calls.len(),
        2,
        "a line with no identity is its own call"
    );
    assert_ne!(
        session.calls[0].id, session.calls[1].id,
        "keys must not collide"
    );
}

#[test]
fn keeps_the_tools_it_can_parse_when_one_block_drifts() {
    const MIXED: &str = r#"{"type":"assistant","timestamp":"t0","message":{"id":"m1","model":"opus","usage":{"input_tokens":1,"output_tokens":1},"content":[42,{"type":"tool_use","id":"t1","name":"Bash"}]}}"#;
    let file = Transcript::new("mixed-blocks", &[MIXED]);

    let session = file.load();

    assert_eq!(
        session.calls[0].tools.len(),
        1,
        "the intact block must survive"
    );
    assert_eq!(session.calls[0].tools[0].name, "Bash");
}

#[test]
fn hands_an_orphan_error_marker_to_the_store() {
    const ERR: &str = r#"{"type":"assistant","timestamp":"t0","isApiErrorMessage":true,"message":{"id":"m1","model":"opus"}}"#;
    let file = Transcript::new("orphan-error", &[ASSISTANT, ERR]);
    let first = ClaudeCode.load_from(&file.0, 0).unwrap();

    let second = ClaudeCode
        .load_from(&file.0, ASSISTANT.len() as u64 + 1)
        .unwrap()
        .session;

    assert!(
        second.calls.is_empty(),
        "an unbilled error line is not a call"
    );
    assert_eq!(second.orphan_errors, [("m1".into(), "api_error".into())]);
    assert_eq!(first.session.failures, ["m1"]);
}

#[test]
fn skips_lines_it_cannot_parse() {
    let file = Transcript::new(
        "garbage",
        &[
            ASSISTANT,
            r#"{"type":"summary","leafUuid":"x"}"#,
            "not json at all",
        ],
    );

    let session = file.load();

    assert_eq!(session.calls.len(), 1);
    assert_eq!(session.project, "/p");
}
