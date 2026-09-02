use serde::Serialize;

#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct Usage {
    pub input: u64,
    /// 5m-TTL cache write. 1.25x the input price.
    pub cache_write_5m: u64,
    /// 1h-TTL cache write. 2x the input price.
    pub cache_write_1h: u64,
    pub cache_read: u64,
    pub output: u64,
    pub thinking: u64,
}

pub const MAX_PLAUSIBLE_TOKENS: u64 = 2_000_000;

impl Usage {
    /// Clamping one field leaves the sum skewed, which later reads as a fake
    /// compaction, so a single bad field discards the whole usage.
    pub fn implausible(&self) -> bool {
        [
            self.input,
            self.cache_write_5m,
            self.cache_write_1h,
            self.cache_read,
            self.output,
            self.thinking,
        ]
        .iter()
        .any(|v| *v > MAX_PLAUSIBLE_TOKENS)
    }

    pub fn cache_write(&self) -> u64 {
        self.cache_write_5m + self.cache_write_1h
    }

    pub fn context(&self) -> u64 {
        self.input + self.cache_write() + self.cache_read
    }
}

impl std::ops::AddAssign for Usage {
    fn add_assign(&mut self, o: Self) {
        self.input += o.input;
        self.cache_write_5m += o.cache_write_5m;
        self.cache_write_1h += o.cache_write_1h;
        self.cache_read += o.cache_read;
        self.output += o.output;
        self.thinking += o.thinking;
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Call {
    pub id: String,
    pub at: String,
    pub model: String,
    pub effort: Option<String>,
    pub skill: Option<String>,
    pub plugin: Option<String>,
    pub agent: Option<String>,
    pub usage: Usage,
    /// Raw usage JSON, so a new field can be recovered after the transcript is gone.
    #[serde(skip)]
    pub usage_json: Option<String>,
    pub error: Option<String>,
    /// First call after a `compact_boundary` marker.
    pub compacted: bool,
    pub tools: Vec<ToolUse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub result_chars: usize,
    pub input_chars: usize,
    pub input_preview: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub id: String,
    pub source: String,
    pub project: String,
    pub title: String,
    #[serde(skip)]
    pub title_is_meta: bool,
    pub parent: Option<String>,
    pub agent_type: Option<String>,
    pub parent_tool_use_id: Option<String>,
    /// Delegation depth. 1 if the parent spawned it directly.
    pub spawn_depth: Option<u32>,
    pub started_at: String,
    pub calls: Vec<Call>,
    #[serde(skip)]
    pub failures: Vec<String>,
    /// tool_results whose tool_use fell in an earlier fragment. The store links them.
    #[serde(skip)]
    pub orphan_results: Vec<(String, usize)>,
    /// Error markers whose call fell in an earlier fragment. The store links them.
    #[serde(skip)]
    pub orphan_errors: Vec<(String, String)>,
}
