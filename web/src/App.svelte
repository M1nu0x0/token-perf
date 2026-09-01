<script lang="ts">
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
    amplification: { label: string; sessions: number; ratio: number }[];
    top_tools: { name: string; added: number; pct: number }[];
    top_sessions: { session: string; title: string; call_count: number; residual: number }[];
    solo_tool_pct: number;
  };

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
      const res = await Promise.all([fetch('/api/tldr'), fetch('/api/sessions')]);
      const bad = res.find((r) => !r.ok);
      if (bad) throw new Error(`${bad.status} ${bad.statusText}`);
      [tldr, sessions] = await Promise.all(res.map((r) => r.json()));
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
    <button class:on={tab === 'tldr'} onclick={() => (tab = 'tldr')}>세 줄 요약</button>
    <button class:on={tab === 'sessions'} onclick={() => (tab = 'sessions')}>세션</button>
  </nav>
</h1>
{#if error}<p class="err">{error}</p>{/if}

{#if tab === 'tldr' && tldr}
  <section class="tldr">
    <p class="lead">
      세션 {tldr.sessions}개, 호출 {human(tldr.calls)}회를 훑어봤어요.
    </p>

    <div class="big">{tldr.cache_read_pct.toFixed(0)}%</div>
    <p>
      당신 토큰의 <b>{tldr.cache_read_pct.toFixed(0)}%</b>는 이미 읽은 걸 다시 읽는 데 쓰였어요.
      새로 말한 건 {human(tldr.totals.output)}밖에 안 되는데,
      다시 읽은 건 {human(tldr.totals.cache_read)}이에요.
    </p>

    <h3>대화가 길수록 같은 걸 여러 번 삽니다</h3>
    <p>
      한 번 들어온 내용은 대화가 끝날 때까지 매번 다시 실려 갑니다.
      {#if short?.sessions && long?.sessions}
        짧은 대화는 {short.ratio.toFixed(1)}번,
        긴 대화는 <b>{long.ratio.toFixed(1)}번</b> 다시 사는 셈이에요.
      {/if}
    </p>
    <table>
      <thead><tr><th>대화 길이</th><th>개수</th><th>같은 내용을 다시 산 횟수</th></tr></thead>
      <tbody>
        {#each tldr.amplification as b (b.label)}
          <tr><td>{b.label}</td><td class="n">{b.sessions}</td><td class="n">{b.ratio.toFixed(1)}번</td></tr>
        {/each}
      </tbody>
    </table>

    <h3>덩치를 키운 범인</h3>
    <p>
      {#each tldr.top_tools as t, i (t.name)}{i ? ', ' : ''}<b>{t.name}</b> {t.pct.toFixed(0)}%{/each}
      — 툴이 대화에 밀어 넣은 양 중 이만큼을 차지해요. 결과가 큰 툴을 덜 부르거나 잘라 쓰면 바로 줄어듭니다.
    </p>

    <h3>제일 비쌌던 대화 셋</h3>
    <table>
      <thead><tr><th>대화</th><th>호출</th><th>다시 청구된 토큰</th></tr></thead>
      <tbody>
        {#each tldr.top_sessions as s (s.session)}
          <tr onclick={() => open(s.session)}><td>{s.title}</td><td class="n">{s.call_count}</td><td class="n">{human(s.residual)}</td></tr>
        {/each}
      </tbody>
    </table>

    <h3>한 번에 하나씩</h3>
    <p>
      툴을 쓴 메시지의 <b>{tldr.solo_tool_pct.toFixed(0)}%</b>가 툴을 딱 하나만 불렀어요.
      한 번에 여러 개를 같이 부르면 그만큼 왕복이 줄고, 왕복이 줄면 다시 읽는 양도 줄어요.
    </p>
  </section>
{/if}

{#if tab === 'sessions'}
<div class="cols">
  <section>
    <h2>
      세션 <small>캐시 재읽기 순</small>
      <label><input type="checkbox" bind:checked={showAll} /> 서브에이전트 포함</label>
    </h2>
    <table>
      <thead><tr><th>날짜</th><th>세션</th><th>호출</th><th>캐시 재읽기</th><th>출력</th></tr></thead>
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
          · {selected.agent_type ?? '서브에이전트'}{selected.spawn_depth ? ` · 깊이 ${selected.spawn_depth}` : ''}
        {/if}
      </p>
      <p>
        호출 {selected.call_count}회 · 출력 {human(selected.totals.output)} ·
        캐시 재읽기 <b>{human(selected.totals.cache_read)}</b>
      </p>
      <p>기저 컨텍스트 {human(selected.baseline)} → 세션 전체에서 {human(selected.baseline_billed)} 청구</p>
      {#if selected.totals.cache_write_1h > 0}
        <p class="dim">
          캐시 쓰기 — 5분 {human(selected.totals.cache_write_5m)} ·
          1시간 {human(selected.totals.cache_write_1h)} (1시간은 입력 단가의 2배)
        </p>
      {/if}
      {#if selected.failed_calls > 0}
        <p class="dim">실패·중단된 호출 {selected.failed_calls}회</p>
      {/if}

      <h3>툴별 잔류 비용</h3>
      <table>
        <thead><tr><th>툴</th><th>호출</th><th>추가</th><th>재청구</th></tr></thead>
        <tbody>
          {#each selected.tools.slice(0, 15) as t (t.name)}
            <tr><td>{t.name}</td><td class="n">{t.calls}</td><td class="n">{human(t.added)}</td><td class="n">{human(t.residual)}</td></tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p>세션을 고르세요.</p>
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
