// Mirrors the serde structs in crates/core/src/common/analyze.rs.
export type Usage = {
  input: number; cache_write_5m: number; cache_write_1h: number;
  cache_read: number; output: number; thinking: number;
};
export type Tool = {
  name: string; calls: number; added: number; residual: number; residual_cost: number | null;
};
export type Call = {
  index: number; at: string; model: string; context: number; output: number;
  grew_by: number; residual: number; tools: string[];
  cost: number | null; residual_cost: number | null;
};
export type Subagents = {
  count: number; call_count: number; totals: Usage; residual: number; cost: number | null;
  by_model: { model: string; calls: number; totals: Usage }[];
  children: {
    session: string; agent_type: string | null; call_count: number;
    totals: Usage; residual: number; cost: number | null;
  }[];
};
export type Report = {
  session: string; source: string; project: string; started_at: string;
  title: string; parent: string | null; agent_type: string | null;
  parent_tool_use_id: string | null; spawn_depth: number | null;
  failed_calls: number; error_kinds: string[];
  call_count: number; totals: Usage; cost: number | null;
  baseline: number; baseline_billed: number;
  calls: Call[]; tools: Tool[]; subagents: Subagents;
};
export type Tldr = {
  sessions: number; calls: number; totals: Usage; cost: number | null; cache_read_pct: number;
  amplification: { min: number; max: number | null; sessions: number; ratio: number }[];
  top_tools: { name: string; added: number; pct: number }[];
  top_sessions: {
    session: string; title: string; call_count: number; residual: number;
    sub_calls: number; sub_residual: number;
  }[];
  solo_tool_pct: number;
};

async function get<T>(path: string): Promise<T> {
  const res = await fetch(path);
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  return res.json() as Promise<T>;
}

export const getTldr = () => get<Tldr>('/api/tldr');
export const getSessions = () => get<Report[]>('/api/sessions');
export const getSession = (id: string) => get<Report>(`/api/sessions/${encodeURIComponent(id)}`);
export const getConfig = () => get<{ lang: string }>('/api/config');
