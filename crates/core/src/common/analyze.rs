//! Call n's context minus call n-1's context and output is what entered in
//! between. A token that entered never leaves: every remaining call re-bills it
//! as cache_read. That is [`CallCost::residual`].

use std::cmp::Reverse;

use serde::Serialize;

use super::model::{Session, Usage};
use super::pricing;

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
    /// Dollars; `None` for a model without a known price.
    pub cost: Option<f64>,
    /// `residual` priced at this call's cache-read rate.
    // ponytail: the re-reads happen on later calls, which may run another model;
    // sessions rarely switch, so the entering call's rate stands in.
    pub residual_cost: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCost {
    pub name: String,
    pub calls: usize,
    pub added: u64,
    pub residual: u64,
    pub residual_cost: Option<f64>,
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
    /// Dollars for every call; `None` if any call's model has no known price.
    pub cost: Option<f64>,
    /// First call's context: system prompt, tool definitions, rule files.
    pub baseline: u64,
    pub baseline_billed: u64,
    pub calls: Vec<CallCost>,
    pub tools: Vec<ToolCost>,
    /// Children rolled up. Empty unless the report came from [`rollup`].
    pub subagents: Subagents,
}

impl Report {
    pub fn residual(&self) -> u64 {
        self.calls.iter().map(|c| c.residual).sum()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelUsage {
    pub model: String,
    pub calls: usize,
    pub totals: Usage,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubagentChild {
    pub session: String,
    pub agent_type: Option<String>,
    pub call_count: usize,
    pub totals: Usage,
    pub residual: u64,
    pub cost: Option<f64>,
}

/// One parent's subagents, summed.
#[derive(Debug, Default, Clone, Serialize)]
pub struct Subagents {
    pub count: usize,
    pub call_count: usize,
    pub totals: Usage,
    pub residual: u64,
    pub cost: Option<f64>,
    /// A spawned agent often runs on a different model than its parent.
    pub by_model: Vec<ModelUsage>,
    /// Worst cache re-read first.
    pub children: Vec<SubagentChild>,
}

pub fn session(s: &Session) -> Report {
    let n = s.calls.len();
    let mut totals = Usage::default();
    let mut calls = Vec::with_capacity(n);

    // A compaction is the source's marker only. Tokens before it are re-billed
    // up to that point, not to the end of the session. A drop in context
    // without a marker is a spike settling or a broken record, not a compaction;
    // in 23k logged calls no unmarked drop ever looked like one.
    let grew: Vec<u64> = std::iter::once(0)
        .chain(s.calls.windows(2).map(|w| {
            let before = w[0].usage.context() + w[0].usage.output;
            w[1].usage.context().saturating_sub(before)
        }))
        .collect();
    // First compaction after each call (n if none).
    let mut next_shrink = vec![n; n];
    let mut next = n;
    for i in (0..n).rev() {
        next_shrink[i] = next;
        if s.calls[i].compacted {
            next = i;
        }
    }

    for (i, call) in s.calls.iter().enumerate() {
        totals += call.usage;
        let grew_by = grew[i];
        let residual = grew_by * (next_shrink[i] - 1 - i) as u64;
        let price = pricing::price(&call.model);

        calls.push(CallCost {
            index: i,
            at: call.at.clone(),
            model: call.model.clone(),
            context: call.usage.context(),
            output: call.usage.output,
            grew_by,
            // Every call up to the next compaction carries these tokens again.
            residual,
            tools: if i == 0 {
                Vec::new()
            } else {
                s.calls[i - 1]
                    .tools
                    .iter()
                    .map(|t| t.name.clone())
                    .collect()
            },
            cost: price.map(|p| p.cost(&call.usage)),
            residual_cost: price.map(|p| p.reread(residual)),
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
        cost: pricing::sum(calls.iter().map(|c| c.cost)),
        baseline,
        baseline_billed: baseline * n as u64,
        tools: attribute_tools(s, &calls),
        calls,
        subagents: Subagents::default(),
    }
}

/// [`session`] plus the usage of every subagent it spawned. A subagent is one
/// tool call of its parent, so its tokens are part of the parent's bill; on its
/// own the parent reads as a fraction of what it cost.
///
/// A nested spawn still lands in the top-level session's `subagents/`
/// directory, so one pass over `all` catches every depth.
// ponytail: rescans `all` per parent, fine at four figures; index by parent if
// session counts reach five.
pub fn rollup(s: &Session, all: &[Session]) -> Report {
    Report {
        subagents: subagents(&s.id, all),
        ..session(s)
    }
}

fn subagents(parent: &str, all: &[Session]) -> Subagents {
    let mut out = Subagents::default();
    let mut by_model: std::collections::HashMap<String, ModelUsage> =
        std::collections::HashMap::new();

    for child in all.iter().filter(|s| s.parent.as_deref() == Some(parent)) {
        let r = session(child);
        let residual = r.residual();
        out.count += 1;
        out.call_count += r.call_count;
        out.totals += r.totals;
        out.residual += residual;

        for call in &child.calls {
            let entry = by_model
                .entry(call.model.clone())
                .or_insert_with(|| ModelUsage {
                    model: call.model.clone(),
                    calls: 0,
                    totals: Usage::default(),
                });
            entry.calls += 1;
            entry.totals += call.usage;
        }
        out.children.push(SubagentChild {
            session: r.session,
            agent_type: r.agent_type,
            call_count: r.call_count,
            totals: r.totals,
            residual,
            cost: r.cost,
        });
    }

    out.cost = pricing::sum(out.children.iter().map(|c| c.cost));
    out.by_model = by_model.into_values().collect();
    out.by_model
        .sort_by_key(|m| (Reverse(m.totals.cache_read), m.model.clone()));
    out.children
        .sort_by_key(|c| (Reverse(c.totals.cache_read), c.session.clone()));
    out
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
                residual_cost: Some(0.0),
            });
            entry.calls += 1;
            entry.added += (cost.grew_by as f64 * share) as u64;
            entry.residual += (cost.residual as f64 * share) as u64;
            entry.residual_cost =
                pricing::sum([entry.residual_cost, cost.residual_cost.map(|c| c * share)]);
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
    pub cost: Option<f64>,
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
    /// Summed over this session's subagents.
    pub sub_calls: usize,
    pub sub_residual: u64,
}

/// Inclusive call-count ranges; `None` is open-ended.
const BUCKETS: [(usize, Option<usize>); 3] = [(0, Some(5)), (6, Some(40)), (41, None)];

pub fn tldr(sessions: &[Session]) -> Tldr {
    let mut out = Tldr {
        sessions: sessions.len(),
        cost: Some(0.0),
        ..Default::default()
    };
    let mut buckets = [(0usize, 0u64, 0u64); BUCKETS.len()];
    let mut tools: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let mut top: Vec<TldrSession> = Vec::new();
    let mut sub: std::collections::HashMap<String, (usize, u64)> = std::collections::HashMap::new();
    let (mut tool_msgs, mut solo_msgs) = (0usize, 0usize);

    for s in sessions {
        let r = session(s);
        out.calls += r.call_count;
        out.totals += r.totals;
        out.cost = pricing::sum([out.cost, r.cost]);

        let residual = r.residual();
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

        // A subagent is one tool call of its parent; listing both double-counts,
        // so it is charged to the parent's row instead of getting its own.
        if let Some(parent) = &s.parent {
            let entry = sub.entry(parent.clone()).or_default();
            entry.0 += r.call_count;
            entry.1 += residual;
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
            sub_calls: 0,
            sub_residual: 0,
        });
    }

    // Children can be listed before their parent, so they are charged after the pass.
    for t in &mut top {
        if let Some(&(calls, residual)) = sub.get(&t.session) {
            (t.sub_calls, t.sub_residual) = (calls, residual);
        }
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

    top.sort_by_key(|s| Reverse(s.residual + s.sub_residual));
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
