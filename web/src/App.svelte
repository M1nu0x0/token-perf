<script lang="ts">
  import { messages, type Bucket } from './messages';

  type Usage = {
    input: number; cache_write_5m: number; cache_write_1h: number;
    cache_read: number; output: number; thinking: number;
  };
  type Tool = { name: string; calls: number; added: number; residual: number };
  type Call = { index: number; context: number; grew_by: number; residual: number; tools: string[] };
  type Report = {
    session: string; source: string; project: string; started_at: string;
    title: string; parent: string | null; agent_type: string | null;
    parent_tool_use_id: string | null; spawn_depth: number | null; failed_calls: number;
    call_count: number; totals: Usage; baseline: number; baseline_billed: number;
    calls: Call[]; tools: Tool[];
  };

  type Tldr = {
    sessions: number; calls: number; totals: Usage; cache_read_pct: number;
    amplification: (Bucket & { sessions: number; ratio: number })[];
    top_tools: { name: string; added: number; pct: number }[];
    top_sessions: { session: string; title: string; call_count: number; residual: number }[];
    solo_tool_pct: number;
  };

  let m = $state(messages('en'));
  let tab = $state<'tldr' | 'sessions'>('tldr');
  let tldr = $state<Tldr | null>(null);
  let sessions = $state<Report[]>([]);
  let selected = $state<Report | null>(null);
  let showAll = $state(false);
  let error = $state('');

  const visible = $derived(sessions.filter((s) => showAll || !s.parent));

  // An empty bucket has ratio 0, which would make the sentence below lie.
  const short = $derived(tldr?.amplification.at(0));
  const long = $derived(tldr?.amplification.at(-1));

  const human = (n: number) =>
    n >= 1e9 ? (n / 1e9).toFixed(1) + 'B'
    : n >= 1e6 ? (n / 1e6).toFixed(1) + 'M'
    : n >= 1e3 ? (n / 1e3).toFixed(1) + 'K'
    : String(n);

  async function load() {
    try {
      const res = await Promise.all([fetch('/api/tldr'), fetch('/api/sessions'), fetch('/api/config')]);
      const bad = res.find((r) => !r.ok);
      if (bad) throw new Error(`${bad.status} ${bad.statusText}`);
      const [t, s, cfg] = await Promise.all(res.map((r) => r.json()));
      [tldr, sessions, m] = [t, s, messages(cfg.lang)];
    } catch (e) {
      error = String(e);
    }
  }

  async function open(id: string) {
    tab = 'sessions';
    try {
      const resp = await fetch(`/api/sessions/${id}`);
      if (!resp.ok) throw new Error(`${resp.status} ${resp.statusText}`);
      selected = await resp.json();
      error = '';
    } catch (e) {
      error = String(e);
    }
  }

  load();
</script>

<h1>
  token-perf
  <nav>
    <button class:on={tab === 'tldr'} onclick={() => (tab = 'tldr')}>{m.tabTldr}</button>
    <button class:on={tab === 'sessions'} onclick={() => (tab = 'sessions')}>{m.tabSessions}</button>
  </nav>
</h1>
{#if error}<p class="err">{error}</p>{/if}

{#if tab === 'tldr' && tldr}
  <section class="tldr">
    <p class="lead">{m.lead(tldr.sessions, human(tldr.calls))}</p>

    <div class="big">{tldr.cache_read_pct.toFixed(0)}%</div>
    <p>
      {@html m.cacheRead(
        tldr.cache_read_pct.toFixed(0),
        human(tldr.totals.output),
        human(tldr.totals.cache_read),
      )}
    </p>

    <h3>{m.lengthHeading}</h3>
    <p>
      {m.lengthIntro}
      {#if short?.sessions && long?.sessions}
        {@html m.lengthCompare(short.ratio.toFixed(1), long.ratio.toFixed(1))}
      {/if}
    </p>
    <table>
      <thead><tr><th>LENGTH</th><th>SESSIONS</th><th>RATIO</th></tr></thead>
      <tbody>
        {#each tldr.amplification as b (b.min)}
          <tr><td>{m.bucket(b)}</td><td class="n">{b.sessions}</td><td class="n">{m.times(b.ratio.toFixed(1))}</td></tr>
        {/each}
      </tbody>
    </table>

    <h3>{m.culpritHeading}</h3>
    <p>
      {#each tldr.top_tools as t, i (t.name)}{i ? ', ' : ''}<b>{t.name}</b> {t.pct.toFixed(0)}%{/each}{m.culpritTail}
    </p>

    <h3>{m.topHeading}</h3>
    <table>
      <thead><tr><th>SESSION</th><th>CALLS</th><th>RESIDUAL</th></tr></thead>
      <tbody>
        {#each tldr.top_sessions as s (s.session)}
          <tr onclick={() => open(s.session)}><td>{s.title}</td><td class="n">{s.call_count}</td><td class="n">{human(s.residual)}</td></tr>
        {/each}
      </tbody>
    </table>

    <h3>{m.soloHeading}</h3>
    <p>{@html m.soloLine(tldr.solo_tool_pct.toFixed(0))}</p>
  </section>
{/if}

{#if tab === 'sessions'}
<div class="cols">
  <section>
    <h2>
      {m.sessionsHeading} <small>{m.byCacheRead}</small>
      <label><input type="checkbox" bind:checked={showAll} /> {m.includeSubagents}</label>
    </h2>
    <table>
      <thead><tr><th>DATE</th><th>SESSION</th><th>CALLS</th><th>CACHE_RD</th><th>OUTPUT</th></tr></thead>
      <tbody>
        {#each visible.slice(0, 50) as s (s.session)}
          <tr class:active={selected?.session === s.session} onclick={() => open(s.session)}>
            <td class="dim">{s.started_at.slice(0, 10)}</td>
            <td title={s.session}>
              {#if s.parent}<span class="dim">↳</span>{/if}
              {s.title || s.project.split('/').pop()}
              <span class="dim">{s.title ? s.project.split('/').pop() : ''}</span>
            </td>
            <td class="n">{s.call_count}</td>
            <td class="n">{human(s.totals.cache_read)}</td>
            <td class="n">{human(s.totals.output)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </section>

  <section>
    {#if selected}
      <h2>{selected.title || selected.project}</h2>
      <p class="dim">
        {selected.project}
        {#if selected.parent}
          · {selected.agent_type ?? m.subagent}{selected.spawn_depth ? m.depth(selected.spawn_depth) : ''}
        {/if}
      </p>
      <p>
        {@html m.totals(
          selected.call_count,
          human(selected.totals.output),
          human(selected.totals.cache_read),
        )}
      </p>
      <p>{m.baseline(human(selected.baseline), human(selected.baseline_billed))}</p>
      {#if selected.totals.cache_write_1h > 0}
        <p class="dim">
          {m.cacheWrites(human(selected.totals.cache_write_5m), human(selected.totals.cache_write_1h))}
        </p>
      {/if}
      {#if selected.failed_calls > 0}
        <p class="dim">{m.failedCalls(selected.failed_calls)}</p>
      {/if}

      <h3>{m.toolResidual}</h3>
      <table>
        <thead><tr><th>TOOL</th><th>CALLS</th><th>ADDED</th><th>RESIDUAL</th></tr></thead>
        <tbody>
          {#each selected.tools.slice(0, 15) as t (t.name)}
            <tr><td>{t.name}</td><td class="n">{t.calls}</td><td class="n">{human(t.added)}</td><td class="n">{human(t.residual)}</td></tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p>{m.pickSession}</p>
    {/if}
  </section>
</div>
{/if}

<style>
  :global(body) { font: 13px/1.5 ui-monospace, SFMono-Regular, Menlo, monospace; margin: 1.5rem; }
  .cols { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr); gap: 2rem; align-items: start; }
  h1 { font-size: 1.2rem; }
  h1 nav { float: right; }
  nav button { font: inherit; background: none; border: 0; padding: .2rem .6rem; opacity: .55; cursor: pointer; color: inherit; }
  nav button.on { opacity: 1; font-weight: 600; border-bottom: 2px solid currentColor; }
  .tldr { max-width: 46rem; }
  .lead { opacity: .6; }
  .big { font-size: 3.5rem; font-weight: 700; line-height: 1.1; margin: 1rem 0 .3rem; font-variant-numeric: tabular-nums; }
  .tldr p { line-height: 1.7; }
  h2 { font-size: 1rem; }
  h3 { font-size: .9rem; margin-top: 1.5rem; }
  small { font-weight: normal; opacity: .6; }
  table { border-collapse: collapse; width: 100%; }
  th, td { text-align: left; padding: .2rem .5rem; border-bottom: 1px solid color-mix(in srgb, currentColor 15%, transparent); }
  th { opacity: .6; font-weight: normal; }
  .n { text-align: right; font-variant-numeric: tabular-nums; }
  tbody tr { cursor: pointer; }
  tbody tr:hover { background: color-mix(in srgb, currentColor 6%, transparent); }
  .active, .active:hover { background: color-mix(in srgb, currentColor 16%, transparent); font-weight: 600; }
  .dim { opacity: .55; }
  tbody td:first-child { white-space: nowrap; }
  h2 label { float: right; font-weight: normal; opacity: .7; }
  .err { color: crimson; }
</style>
