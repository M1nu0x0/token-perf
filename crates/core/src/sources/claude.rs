//! The schema drifts often, so only the needed fields are pulled out. Unknown
//! fields are dropped and unparseable lines skipped whole.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{Fragment, Source, home, session_id, walk};
use crate::common::model::{Call, Session, ToolUse, Usage};

pub struct ClaudeCode;

impl Source for ClaudeCode {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn discover(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if let Some(root) = home().map(|h| h.join(".claude/projects")) {
            walk(&root, "jsonl", &mut out);
        }
        out
    }

    fn load_from(&self, path: &Path, offset: u64) -> std::io::Result<Fragment> {
        let mut file = std::fs::File::open(path)?;
        if offset > 0 {
            file.seek(SeekFrom::Start(offset))?;
        }
        let mut reader = BufReader::new(file);
        let mut consumed = offset;

        let mut session = Session {
            id: session_id(path),
            source: self.name().into(),
            project: String::new(),
            title: String::new(),
            title_is_meta: false,
            parent: subagent_parent(path),
            agent_type: None,
            parent_tool_use_id: None,
            spawn_depth: None,
            started_at: String::new(),
            calls: Vec::new(),
            failures: Vec::new(),
            orphan_results: Vec::new(),
            orphan_errors: Vec::new(),
        };
        // A subagent's sidecar .meta.json names the parent tool call it came from.
        if let Some(meta) = subagent_meta(path) {
            session.agent_type = Some(meta.agent_type).filter(|s| !s.is_empty());
            session.parent_tool_use_id = Some(meta.tool_use_id).filter(|s| !s.is_empty());
            session.spawn_depth = meta.spawn_depth;
            // Full read only: on every fragment it would keep fighting the real title.
            if offset == 0 && !meta.description.is_empty() {
                session.title = meta.description;
                session.title_is_meta = true;
            }
        }
        // One response spans several lines, one per content block kind, each repeating
        // the same usage. Summing over-counts, but tool_use usually sits on the last
        // line — so count usage once and collect tools from every line.
        let mut seen: HashMap<String, usize> = HashMap::new();
        // tool_use is on an assistant line, its result on a later user line.
        let mut tool_slot: HashMap<String, (usize, usize)> = HashMap::new();
        let mut open: Option<u64> = None;
        let mut started: HashSet<String> = HashSet::new();

        let mut raw_line = Vec::new();
        loop {
            raw_line.clear();
            if reader.read_until(b'\n', &mut raw_line)? == 0 {
                break;
            }
            if raw_line.last() != Some(&b'\n') {
                break;
            }
            let line_start = consumed;
            consumed += raw_line.len() as u64;
            let Ok(line) = std::str::from_utf8(&raw_line[..raw_line.len() - 1]) else {
                continue;
            };
            let Ok(mut raw) = serde_json::from_str::<RawLine>(line) else {
                continue;
            };

            if session.project.is_empty() && !raw.cwd.is_empty() {
                session.project = raw.cwd.clone();
            }
            if session.started_at.is_empty() && !raw.timestamp.is_empty() {
                session.started_at = raw.timestamp.clone();
            }

            if raw.kind == "ai-title" && !raw.ai_title.is_empty() {
                session.title = raw.ai_title;
                session.title_is_meta = false;
                continue;
            }

            let error_kind = raw.error_kind();
            let message = raw.message.take();
            let message_id = message
                .as_ref()
                .map(|m| m.id.clone())
                .filter(|id| !id.is_empty());
            // Append-only, so the start offset is a stable identity across re-reads.
            let key = message_id
                .clone()
                .unwrap_or_else(|| format!("@{line_start}"));

            if raw.kind == "assistant"
                && let Some(id) = &message_id
                && started.insert(id.clone())
            {
                open = Some(line_start);
            }

            // Before usage or message: a call that spent no tokens is still a failure.
            if let Some(kind) = &error_kind {
                session.failures.push(key.clone());
                if let Some(id) = &message_id {
                    session.orphan_errors.push((id.clone(), kind.clone()));
                }
            }

            let Some(mut message) = message else {
                continue;
            };

            match raw.kind.as_str() {
                "assistant" => {
                    let Some(usage_value) = message.usage.take() else {
                        continue;
                    };
                    let usage = parse_usage(&usage_value);
                    // All-zero stubs and failed calls would sink the context series to
                    // 0 and wreck the differencing; an implausible usage does the same.
                    let billed = (usage.context() > 0 || usage.output > 0) && !usage.implausible();
                    let index = match message_id.as_ref().and_then(|id| seen.get(id).copied()) {
                        Some(index) => {
                            // Cumulative snapshots: the last line holds the final usage.
                            if billed {
                                session.calls[index].usage = usage;
                                session.calls[index].usage_json = Some(usage_value.to_string());
                            }
                            if error_kind.is_some() {
                                session.calls[index].error = error_kind;
                            }
                            Some(index)
                        }
                        None if billed => {
                            let index = session.calls.len();
                            if let Some(id) = &message_id {
                                seen.insert(id.clone(), index);
                            }
                            session.calls.push(Call {
                                id: key.clone(),
                                at: raw.timestamp,
                                model: message.model,
                                effort: raw.effort.take(),
                                skill: raw.attribution_skill.take(),
                                plugin: raw.attribution_plugin.take(),
                                agent: raw.attribution_agent.take(),
                                usage,
                                usage_json: Some(usage_value.to_string()),
                                error: error_kind,
                                tools: Vec::new(),
                            });
                            Some(index)
                        }
                        None => None,
                    };

                    // Error and stub lines carry tool_use too.
                    let Some(index) = index else { continue };
                    for block in message.content.blocks() {
                        if block.kind == "tool_use" {
                            // Snapshots re-send tool_use blocks already seen.
                            if tool_slot.contains_key(&block.id) {
                                continue;
                            }
                            let slot = session.calls[index].tools.len();
                            tool_slot.insert(block.id.clone(), (index, slot));
                            let input = block.input.map_or_else(String::new, |v| v.to_string());
                            session.calls[index].tools.push(ToolUse {
                                id: block.id,
                                name: block.name,
                                result_chars: 0,
                                input_chars: input.chars().count(),
                                input_preview: preview(&input),
                            });
                        }
                    }
                }
                "user" => {
                    for block in message.content.blocks() {
                        if block.kind != "tool_result" {
                            continue;
                        }
                        let chars = block.content.as_ref().map_or(0, result_chars);
                        match tool_slot.get(&block.tool_use_id) {
                            Some(&(call, slot)) => {
                                session.calls[call].tools[slot].result_chars = chars;
                            }
                            None => session.orphan_results.push((block.tool_use_id, chars)),
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(Fragment {
            session,
            consumed,
            resume_at: open.unwrap_or(consumed),
        })
    }
}

#[derive(Deserialize)]
struct RawLine {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    timestamp: String,
    #[serde(default)]
    cwd: String,
    #[serde(default, rename = "aiTitle")]
    ai_title: String,
    #[serde(default)]
    effort: Option<String>,
    #[serde(default, rename = "attributionSkill")]
    attribution_skill: Option<String>,
    #[serde(default, rename = "attributionPlugin")]
    attribution_plugin: Option<String>,
    #[serde(default, rename = "attributionAgent")]
    attribution_agent: Option<String>,
    #[serde(default, rename = "isApiErrorMessage")]
    is_api_error: bool,
    #[serde(default, rename = "isAbortedMidStream")]
    is_aborted: bool,
    #[serde(default, rename = "apiErrorStatus")]
    api_error_status: Option<u16>,
    #[serde(default)]
    message: Option<RawMessage>,
}

impl RawLine {
    fn error_kind(&self) -> Option<String> {
        match (self.api_error_status, self.is_api_error, self.is_aborted) {
            (Some(status), _, _) => Some(format!("http_{status}")),
            (_, true, _) => Some("api_error".into()),
            (_, _, true) => Some("aborted".into()),
            _ => None,
        }
    }
}

#[derive(Deserialize, Default)]
struct RawAgentMeta {
    #[serde(default, rename = "agentType")]
    agent_type: String,
    #[serde(default)]
    description: String,
    #[serde(default, rename = "toolUseId")]
    tool_use_id: String,
    #[serde(default, rename = "spawnDepth")]
    spawn_depth: Option<u32>,
}

fn result_chars(content: &serde_json::Value) -> usize {
    match content {
        serde_json::Value::String(s) => s.chars().count(),
        serde_json::Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b.get("text")?.as_str())
            .map(|t| t.chars().count())
            .sum(),
        other => other.to_string().chars().count(),
    }
}

fn subagent_meta(path: &Path) -> Option<RawAgentMeta> {
    let meta = path.with_extension("meta.json");
    serde_json::from_str(&std::fs::read_to_string(meta).ok()?).ok()
}

/// Subagent transcripts live at `<parent uuid>/subagents/agent-*.jsonl`.
fn subagent_parent(path: &Path) -> Option<String> {
    let dir = path.parent()?;
    if dir.file_name()? != "subagents" {
        return None;
    }
    Some(dir.parent()?.file_name()?.to_string_lossy().into_owned())
}

#[derive(Deserialize)]
struct RawMessage {
    #[serde(default)]
    id: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    usage: Option<serde_json::Value>,
    #[serde(default)]
    content: RawContent,
}

/// A bare string is dropped. The array is parsed per element so one drifted block
/// cannot cost the whole response's tools.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawContent {
    Blocks(Vec<serde_json::Value>),
    Other(serde::de::IgnoredAny),
}

impl Default for RawContent {
    fn default() -> Self {
        Self::Other(serde::de::IgnoredAny)
    }
}

impl RawContent {
    fn blocks(self) -> Vec<RawBlock> {
        match self {
            RawContent::Blocks(b) => b
                .into_iter()
                .filter_map(|v| serde_json::from_value(v).ok())
                .collect(),
            RawContent::Other(_) => Vec::new(),
        }
    }
}

#[derive(Deserialize)]
struct RawBlock {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    tool_use_id: String,
    #[serde(default)]
    content: Option<serde_json::Value>,
    #[serde(default)]
    input: Option<serde_json::Value>,
}

fn preview(input: &str) -> String {
    match input.char_indices().nth(200) {
        Some((end, _)) => input[..end].to_string(),
        None => input.to_string(),
    }
}

/// A float, null or string zeroes only that field. Parsing the block as a struct
/// would silently lose the whole response's tokens.
fn num(v: &serde_json::Value, key: &str) -> u64 {
    let Some(field) = v.get(key) else { return 0 };
    field
        .as_u64()
        .or_else(|| field.as_f64().map(|f| f.max(0.0) as u64))
        .unwrap_or(0)
}

fn parse_usage(v: &serde_json::Value) -> Usage {
    let (write_5m, write_1h) = match v.get("cache_creation").filter(|c| c.is_object()) {
        Some(c) => (
            num(c, "ephemeral_5m_input_tokens"),
            num(c, "ephemeral_1h_input_tokens"),
        ),
        None => (num(v, "cache_creation_input_tokens"), 0),
    };

    Usage {
        input: num(v, "input_tokens"),
        cache_write_5m: write_5m,
        cache_write_1h: write_1h,
        cache_read: num(v, "cache_read_input_tokens"),
        output: num(v, "output_tokens"),
        thinking: v
            .get("output_tokens_details")
            .map_or(0, |d| num(d, "thinking_tokens")),
    }
}

#[cfg(test)]
mod tests;
