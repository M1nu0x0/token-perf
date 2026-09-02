//! Call n's context minus call n-1's context and output is what entered in
//! between. A token that entered never leaves: every remaining call re-bills it
//! as cache_read. That is [`CallCost::residual`].

use std::cmp::Reverse;

use serde::Serialize;

use super::model::{Session, Usage};

#[derive(Debug, Clone, Serialize)]
pub struct CallCost {
    pub index: usize,
    pub at: String,
    pub model: String,
    pub context: u64,
    pub output: u64,
    pub grew_by: u64,
    /// Total re-billing of `grew_by` across the remaining calls.
    pub residual: u64,
    /// Requested by the previous call.
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCost {
    pub name: String,
    pub calls: usize,
    pub added: u64,
    pub residual: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub session: String,
    pub source: String,
    pub project: String,
    pub title: String,
    pub parent: Option<String>,
    pub agent_type: Option<String>,
    pub parent_tool_use_id: Option<String>,
    pub spawn_depth: Option<u32>,
    pub failed_calls: usize,
    pub error_kinds: Vec<String>,
    pub started_at: String,
    pub call_count: usize,
    pub totals: Usage,
    /// First call's context: system prompt, tool definitions, rule files.
    pub baseline: u64,
    pub baseline_billed: u64,
    pub calls: Vec<CallCost>,
    pub tools: Vec<ToolCost>,
}

pub fn session(s: &Session) -> Report {
    let n = s.calls.len();
    let mut totals = Usage::default();
    let mut calls = Vec::with_capacity(n);

    // A shrinking context is a compaction: tokens before it are re-billed only up
    // to that point, not to the end of the session.
    let mut grew = vec![0u64; n];
    let mut shrank = vec![false; n];
    for i in 1..n {
        let (prev, call) = (&s.calls[i - 1], &s.calls[i]);
        let before = prev.usage.context() + prev.usage.output;
        let after = call.usage.context();
        grew[i] = after.saturating_sub(before);
        // A small dip is a spike settling back or a broken record; calling that a
        // compaction would truncate every residual.
        // ponytail: without a marker the constants are arbitrary; a compaction the
        // fallback misses inflates every residual before it.
        let cw_ratio = call.usage.cache_write() as f64 / after.max(1) as f64;
        shrank[i] = call.compacted || (after < prev.usage.context() * 3 / 4 && cw_ratio >= 0.4);
    }
    // First shrink after each call (n if none).
    let mut next_shrink = vec![n; n];
    let mut next = n;
    for i in (0..n).rev() {
        next_shrink[i] = next;
        if shrank[i] {
            next = i;
        }
    }

    for (i, call) in s.calls.iter().enumerate() {
        totals += call.usage;
        let grew_by = grew[i];

        calls.push(CallCost {
            index: i,
            at: call.at.clone(),
            model: call.model.clone(),
            context: call.usage.context(),
            output: call.usage.output,
            grew_by,
            // Every call up to the next compaction carries these tokens again.
            residual: grew_by * (next_shrink[i] - 1 - i) as u64,
            tools: if i == 0 {
                Vec::new()
            } else {
                s.calls[i - 1]
                    .tools
                    .iter()
                    .map(|t| t.name.clone())
                    .collect()
            },
        });
    }

    let baseline = s.calls.first().map_or(0, |c| c.usage.context());
    let mut error_kinds: Vec<String> = s.calls.iter().filter_map(|c| c.error.clone()).collect();
    error_kinds.sort();
    error_kinds.dedup();

    Report {
        session: s.id.clone(),
        source: s.source.clone(),
        project: s.project.clone(),
        title: s.title.clone(),
        parent: s.parent.clone(),
        agent_type: s.agent_type.clone(),
        parent_tool_use_id: s.parent_tool_use_id.clone(),
        spawn_depth: s.spawn_depth,
        failed_calls: s.failures.len(),
        error_kinds,
        started_at: s.started_at.clone(),
        call_count: n,
        totals,
        baseline,
        baseline_billed: baseline * n as u64,
        tools: attribute_tools(s, &calls),
        calls,
    }
}

/// Weighted by result chars. Summed `added` falls short of summed `grew_by`:
/// growth after a call that used no tools has nothing to attribute to and is
/// skipped entirely, and floored shares lose a little more.
fn attribute_tools(s: &Session, calls: &[CallCost]) -> Vec<ToolCost> {
    let mut acc: std::collections::HashMap<String, ToolCost> = std::collections::HashMap::new();

    for cost in calls.iter().skip(1) {
        let tools = &s.calls[cost.index - 1].tools;
        if tools.is_empty() {
            continue;
        }
        let total_chars: usize = tools.iter().map(|t| t.result_chars).sum();

        for tool in tools {
            let share = if total_chars == 0 {
                1.0 / tools.len() as f64
            } else {
                tool.result_chars as f64 / total_chars as f64
            };
            let entry = acc.entry(tool.name.clone()).or_insert_with(|| ToolCost {
                name: tool.name.clone(),
                calls: 0,
                added: 0,
                residual: 0,
            });
            entry.calls += 1;
            entry.added += (cost.grew_by as f64 * share) as u64;
            entry.residual += (cost.residual as f64 * share) as u64;
        }
    }

    let mut out: Vec<_> = acc.into_values().collect();
    out.sort_by_key(|t| (Reverse(t.residual), t.name.clone()));
    out
}

#[derive(Debug, Default, Serialize)]
pub struct Tldr {
    pub sessions: usize,
    pub calls: usize,
    pub totals: Usage,
    pub cache_read_pct: f64,
    pub amplification: Vec<Bucket>,
    pub top_tools: Vec<TldrTool>,
    pub top_sessions: Vec<TldrSession>,
    pub solo_tool_pct: f64,
}

/// Call-count range; the display label is the caller's business.
#[derive(Debug, Serialize)]
pub struct Bucket {
    pub min: usize,
    pub max: Option<usize>,
    pub sessions: usize,
    /// sum(residual) / sum(grew_by): how often the same token was bought again.
    pub ratio: f64,
}

#[derive(Debug, Serialize)]
pub struct TldrTool {
    pub name: String,
    pub added: u64,
    pub pct: f64,
}

#[derive(Debug, Serialize)]
pub struct TldrSession {
    pub session: String,
    pub title: String,
    pub call_count: usize,
    pub residual: u64,
}

/// Inclusive call-count ranges; `None` is open-ended.
const BUCKETS: [(usize, Option<usize>); 3] = [(0, Some(5)), (6, Some(40)), (41, None)];

pub fn tldr(sessions: &[Session]) -> Tldr {
    let mut out = Tldr {
        sessions: sessions.len(),
        ..Default::default()
    };
    let mut buckets = [(0usize, 0u64, 0u64); BUCKETS.len()];
    let mut tools: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let mut top: Vec<TldrSession> = Vec::new();
    let (mut tool_msgs, mut solo_msgs) = (0usize, 0usize);

    for s in sessions {
        let r = session(s);
        out.calls += r.call_count;
        out.totals += r.totals;

        let residual: u64 = r.calls.iter().map(|c| c.residual).sum();
        let grew: u64 = r.calls.iter().map(|c| c.grew_by).sum();
        let bucket = BUCKETS
            .iter()
            .position(|(_, max)| max.is_none_or(|m| r.call_count <= m))
            .unwrap_or(BUCKETS.len() - 1);
        buckets[bucket].0 += 1;
        buckets[bucket].1 += residual;
        buckets[bucket].2 += grew;

        for t in &r.tools {
            *tools.entry(t.name.clone()).or_default() += t.added;
        }
        for c in &s.calls {
            if !c.tools.is_empty() {
                tool_msgs += 1;
                solo_msgs += usize::from(c.tools.len() == 1);
            }
        }

        // A subagent is one tool call of its parent; listing both double-counts.
        if s.parent.is_some() {
            continue;
        }
        top.push(TldrSession {
            session: r.session,
            title: if r.title.is_empty() {
                r.project.rsplit('/').next().unwrap_or_default().to_string()
            } else {
                r.title
            },
            call_count: r.call_count,
            residual,
        });
    }

    let billed =
        out.totals.input + out.totals.cache_write() + out.totals.cache_read + out.totals.output;
    out.cache_read_pct = pct(out.totals.cache_read, billed);
    out.solo_tool_pct = pct(solo_msgs as u64, tool_msgs as u64);

    out.amplification = BUCKETS
        .iter()
        .zip(buckets)
        .map(|(&(min, max), (n, residual, grew))| Bucket {
            min,
            max,
            sessions: n,
            ratio: if grew == 0 {
                0.0
            } else {
                residual as f64 / grew as f64
            },
        })
        .collect();

    let total_added: u64 = tools.values().sum();
    let mut tools: Vec<_> = tools.into_iter().collect();
    tools.sort_by_key(|(name, added)| (Reverse(*added), name.clone()));
    out.top_tools = tools
        .into_iter()
        .take(3)
        .map(|(name, added)| TldrTool {
            name,
            added,
            pct: pct(added, total_added),
        })
        .collect();

    top.sort_by_key(|s| Reverse(s.residual));
    top.truncate(3);
    out.top_sessions = top;
    out
}

fn pct(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 * 100.0 / whole as f64
    }
}

#[cfg(test)]
mod tests;
