use super::*;
use crate::common::model::{Call, ToolUse};

fn call(cache_read: u64, cache_write: u64, output: u64, tool: &str, chars: usize) -> Call {
    Call {
        id: tool.into(),
        at: String::new(),
        model: "m".into(),
        effort: None,
        skill: None,
        plugin: None,
        agent: None,
        usage_json: None,
        usage: Usage {
            input: 0,
            cache_write_5m: cache_write,
            cache_write_1h: 0,
            cache_read,
            output,
            thinking: 0,
        },
        error: None,
        compacted: false,
        tools: if tool.is_empty() {
            Vec::new()
        } else {
            vec![ToolUse {
                id: tool.into(),
                name: tool.into(),
                result_chars: chars,
                input_chars: 0,
                input_preview: String::new(),
            }]
        },
    }
}

fn three_call_session() -> Session {
    Session {
        id: "s".into(),
        source: "test".into(),
        project: "/p".into(),
        title: String::new(),
        title_is_meta: false,
        parent: None,
        agent_type: None,
        parent_tool_use_id: None,
        spawn_depth: None,
        started_at: String::new(),
        calls: vec![
            call(0, 100, 10, "Bash", 4000),
            call(1000, 0, 0, "Read", 400),
            call(1100, 0, 0, "", 0),
        ],
        failures: Vec::new(),
        orphan_results: Vec::new(),
        orphan_errors: Vec::new(),
    }
}

#[test]
fn baseline_is_the_first_calls_context() {
    let s = three_call_session();

    let report = session(&s);

    assert_eq!(report.baseline, 100);
    assert_eq!(report.baseline_billed, 300, "baseline 100 x 3 calls");
}

#[test]
fn growth_is_context_delta_minus_previous_output() {
    let s = three_call_session();

    let report = session(&s);

    assert_eq!(
        report.calls[0].grew_by, 0,
        "the first call has no predecessor"
    );
    assert_eq!(report.calls[1].grew_by, 890, "1000 - 100 - output 10");
    assert_eq!(report.calls[2].grew_by, 100, "1100 - 1000 - output 0");
}

#[test]
fn residual_multiplies_growth_by_remaining_calls() {
    let s = three_call_session();

    let report = session(&s);

    assert_eq!(report.calls[1].residual, 890, "890 x 1 remaining call");
    assert_eq!(
        report.calls[2].residual, 0,
        "nothing is re-billed after the last call"
    );
}

#[test]
fn growth_is_attributed_to_the_tool_that_caused_it() {
    let s = three_call_session();

    let report = session(&s);

    assert_eq!(
        report.tools[0].name, "Bash",
        "growth attaches to the previous call's tool"
    );
    assert_eq!(report.tools[0].residual, 890);
    assert_eq!(report.tools[1].name, "Read");
    assert_eq!(report.tools[1].residual, 0);
}

#[test]
fn context_shrink_does_not_underflow() {
    let s = Session {
        id: "s".into(),
        source: "test".into(),
        project: "/p".into(),
        title: String::new(),
        title_is_meta: false,
        parent: None,
        agent_type: None,
        parent_tool_use_id: None,
        spawn_depth: None,
        started_at: String::new(),
        calls: vec![call(9000, 0, 50, "Bash", 10), call(200, 0, 0, "", 0)],
        failures: Vec::new(),
        orphan_results: Vec::new(),
        orphan_errors: Vec::new(),
    };

    let report = session(&s);

    assert_eq!(report.calls[1].grew_by, 0);
}

#[test]
fn residual_stops_at_the_next_compaction() {
    let mut s = Session {
        id: "s".into(),
        source: "test".into(),
        project: "/p".into(),
        title: String::new(),
        title_is_meta: false,
        parent: None,
        agent_type: None,
        parent_tool_use_id: None,
        spawn_depth: None,
        started_at: String::new(),
        calls: vec![
            call(100, 0, 0, "Bash", 10),
            call(1100, 0, 0, "Bash", 10),
            call(1200, 0, 0, "Bash", 10),
            call(0, 300, 0, "Bash", 10),
            call(400, 0, 0, "", 0),
        ],
        failures: Vec::new(),
        orphan_results: Vec::new(),
        orphan_errors: Vec::new(),
    };
    s.calls[3].compacted = true;

    let report = session(&s);

    assert_eq!(report.calls[1].grew_by, 1000);
    assert_eq!(
        report.calls[1].residual, 1000,
        "re-billed only up to the compaction, not the session end"
    );
    assert_eq!(report.calls[3].grew_by, 0, "a shrink is not growth");
}

#[test]
fn tldr_aggregates_across_sessions() {
    let sessions = vec![three_call_session(), three_call_session()];

    let t = tldr(&sessions);

    assert_eq!(t.sessions, 2);
    assert_eq!(t.calls, 6);
    assert!(
        (t.cache_read_pct - 95.02).abs() < 0.01,
        "4200 cache_read out of 4420 billed: {}",
        t.cache_read_pct
    );
    assert_eq!(
        t.amplification[0].sessions, 2,
        "a 3-call session lands in the <=5 bucket"
    );
    assert!((t.amplification[0].ratio - 890.0 / 990.0).abs() < 1e-9);
    assert_eq!(t.amplification[2].sessions, 0, "no session has 41+ calls");
    assert_eq!(t.top_tools[0].name, "Bash");
    assert_eq!(t.top_tools[0].added, 1780, "890 x 2 sessions");
    assert!((t.top_tools[0].pct - 89.898).abs() < 0.01);
    assert_eq!(t.top_sessions[0].residual, 890);
    assert_eq!(
        t.solo_tool_pct, 100.0,
        "every tool message requested one tool"
    );
}

fn drop_session(read: u64, write: u64) -> Session {
    Session {
        calls: vec![
            call(1000, 0, 0, "Bash", 10),
            call(2000, 0, 0, "Bash", 10),
            call(read, write, 0, "Bash", 10),
            call(read + write + 1000, 0, 0, "", 0),
        ],
        ..three_call_session()
    }
}

#[test]
fn a_marked_call_is_a_compaction_however_shallow_the_drop() {
    let mut s = drop_session(1900, 0);
    s.calls[2].compacted = true;

    let report = session(&s);

    assert_eq!(report.calls[1].grew_by, 1000);
    assert_eq!(report.calls[1].residual, 0, "the marker alone cuts it");
}

#[test]
fn an_unmarked_drop_is_not_a_compaction_however_deep() {
    let report = session(&drop_session(500, 700));

    assert_eq!(
        report.calls[1].residual, 2000,
        "re-billed to the session end"
    );
    assert_eq!(report.calls[2].grew_by, 0, "a drop is not growth");
}

fn child_of(parent: &str, id: &str, model: &str) -> Session {
    let mut s = Session {
        id: id.into(),
        parent: Some(parent.into()),
        agent_type: Some("general-purpose".into()),
        ..three_call_session()
    };
    for c in &mut s.calls {
        c.model = model.into();
    }
    s
}

#[test]
fn rollup_sums_the_children_into_the_parent() {
    let all = vec![
        three_call_session(),
        child_of("s", "c1", "opus"),
        child_of("s", "c2", "sonnet"),
    ];

    let r = rollup(&all[0], &all);

    assert_eq!(r.totals.cache_read, 2100, "the parent's own figure stands");
    assert_eq!(r.subagents.count, 2);
    assert_eq!(r.subagents.call_count, 6, "3 calls x 2 children");
    assert_eq!(r.subagents.totals.cache_read, 4200);
    assert_eq!(r.subagents.residual, 1780, "890 x 2 children");
    assert_eq!(r.subagents.by_model[0].model, "opus", "split per model");
    assert_eq!(r.subagents.by_model[0].calls, 3);
    assert_eq!(r.subagents.by_model[1].model, "sonnet");
    assert_eq!(r.subagents.children.len(), 2);
    assert_eq!(
        rollup(&all[1], &all).subagents.count,
        0,
        "a subagent spawns none of its own here"
    );
}

#[test]
fn tldr_charges_a_subagent_to_its_parent() {
    // The child comes first: a parent's row is filled in after the whole pass.
    let all = vec![
        child_of("s", "c1", "opus"),
        three_call_session(),
        child_of("s", "c2", "opus"),
    ];

    let t = tldr(&all);

    assert_eq!(t.top_sessions.len(), 1, "a child gets no row of its own");
    assert_eq!(t.top_sessions[0].session, "s");
    assert_eq!(t.top_sessions[0].residual, 890, "the parent's own");
    assert_eq!(t.top_sessions[0].sub_residual, 1780, "890 x 2 children");
    assert_eq!(t.top_sessions[0].sub_calls, 6);
}

#[test]
fn a_session_is_priced_only_when_every_model_is_known() {
    let mut s = three_call_session();
    for c in &mut s.calls {
        c.model = "claude-opus-5".into();
    }
    let known = session(&s).cost.expect("priced");
    assert!(known > 0.0);
    assert_eq!(
        session(&s).calls[1].residual_cost,
        Some(session(&s).calls[1].residual as f64 * 0.5 / 1e6)
    );

    s.calls[1].model = "unknown-model".into();
    assert_eq!(
        session(&s).cost,
        None,
        "a partial sum would read as the whole"
    );
    assert_eq!(session(&s).tools[0].residual_cost, None);
}
