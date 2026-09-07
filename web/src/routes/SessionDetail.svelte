<script lang="ts">
  import type { messages } from '../messages';
  import { getSession, type Report } from '../lib/api';
  import { dollars, human } from '../lib/format';

  let { m, id }: { m: ReturnType<typeof messages>; id: string } = $props();

  let report = $state<Report | null>(null);
  let error = $state('');

  $effect(() => {
    const wanted = id;
    [report, error] = [null, ''];
    // A later route may already have won; a slow response must not overwrite it.
    getSession(wanted)
      .then((r) => wanted === id && (report = r))
      .catch((e) => {
        if (wanted === id) error = String(e).includes('404') ? m.notFound : String(e);
      });
  });

  const models = $derived([...new Set(report?.calls.map((c) => c.model) ?? [])]);
  const topTools = $derived(report?.tools.slice(0, 15) ?? []);
  const maxResidual = $derived(Math.max(...topTools.map((t) => t.residual), 1));

  // Growth curve. The y axis is context; dots are the calls that grew it most.
  const chart = $derived.by(() => {
    const calls = report?.calls ?? [];
    if (calls.length < 2) return null;
    const max = Math.max(...calls.map((c) => c.context), 1);
    const x = (i: number) => ((i / (calls.length - 1)) * 1000).toFixed(1);
    const y = (v: number) => (235 - (v / max) * 230).toFixed(1);
    const sorted = calls.map((c) => c.grew_by).sort((a, b) => b - a);
    const cut = sorted[Math.max(0, Math.ceil(sorted.length * 0.1) - 1)] ?? 0;
    return {
      line: calls.map((c, i) => `${x(i)},${y(c.context)}`).join(' '),
      first: calls[0].context,
      last: calls[calls.length - 1].context,
      marks: calls
        .map((c, i) => ({ c, i }))
        .filter(({ c }) => c.grew_by > 0 && c.grew_by >= cut)
        .map(({ c, i }) => ({ cx: x(i), cy: y(c.context), call: c })),
    };
  });
</script>

<section>
  <nav class="mb-3 flex flex-wrap gap-x-3 text-dim">
    <a class="hover:text-accent hover:underline" href="#/sessions">← {m.backToSessions}</a>
    {#if report?.parent}
      <a class="hover:text-accent hover:underline" href="#/s/{report.parent}"
        >↑ {m.parentSession}</a
      >
    {/if}
  </nav>

  {#if error}
    <p class="text-warn">{error}</p>
  {:else if !report}
    <p class="text-dim">{m.loading}</p>
  {:else}
    <h2 class="text-lg font-semibold">{report.title || report.project}</h2>
    <p class="font-mono text-dim">
      {report.started_at.slice(0, 16).replace('T', ' ')} · {report.project}
      {#if models.length}· {models.join(', ')}{/if}
      {#if report.parent}
        · {report.agent_type ?? m.subagent}{report.spawn_depth ? m.depth(report.spawn_depth) : ''}
      {/if}
    </p>

    <div class="mt-4 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
      <div class="rounded-md border border-border bg-surface p-4">
        <div class="text-dim">{m.cardCalls}</div>
        <div class="font-mono text-3xl font-bold tabular-nums">{report.call_count}</div>
        {#if report.failed_calls > 0}
          <div class="text-warn">{m.failedCalls(report.failed_calls)}</div>
        {/if}
      </div>
      <div class="rounded-md border border-border bg-surface p-4">
        <div class="text-dim">{m.cardCacheRead}</div>
        <div class="font-mono text-3xl font-bold tabular-nums">
          {human(report.totals.cache_read)}
        </div>
        <div class="font-mono text-dim">{human(report.totals.output)} {m.labelOutput}</div>
      </div>
      <div class="rounded-md border border-border bg-surface p-4">
        <div class="text-dim">{m.cardCost}</div>
        <div class="font-mono text-3xl font-bold tabular-nums">{dollars(report.cost)}</div>
        <div class="text-dim">{m.costNote}</div>
      </div>
      <div class="rounded-md border border-border bg-surface p-4">
        <div class="text-dim">{m.cardSubCost}</div>
        <div class="font-mono text-3xl font-bold tabular-nums">
          {report.subagents.count ? dollars(report.subagents.cost) : '—'}
        </div>
        <div class="font-mono text-dim">
          {report.subagents.count
            ? `${report.subagents.count} · ${report.subagents.call_count} calls`
            : ''}
        </div>
      </div>
    </div>

    <p class="mt-4 max-w-[62ch]">{m.baseline(human(report.baseline), human(report.baseline_billed))}</p>
    {#if report.totals.cache_write_1h > 0}
      <p class="max-w-[62ch] text-dim">
        {m.cacheWrites(human(report.totals.cache_write_5m), human(report.totals.cache_write_1h))}
      </p>
    {/if}
    {#if report.error_kinds.length}
      <p class="max-w-[62ch] text-dim">
        {m.errorKinds}: <span class="font-mono">{report.error_kinds.join(', ')}</span>
      </p>
    {/if}

    {#if chart}
      <h3 class="mt-8 mb-2 font-semibold">{m.growthHeading}</h3>
      <svg class="w-full text-accent" viewBox="0 0 1000 240" role="img">
        <title>{m.growthSummary(report.call_count, human(chart.first), human(chart.last))}</title>
        <polygon class="fill-accent/10" points="{chart.line} 1000,240 0,240" />
        <polyline class="fill-none stroke-accent" stroke-width="2" points={chart.line} />
        {#each chart.marks as mark (mark.call.index)}
          <circle class="fill-accent" cx={mark.cx} cy={mark.cy} r="4">
            <title
              >#{mark.call.index} +{human(mark.call.grew_by)}{mark.call.tools.length
                ? ` · ${mark.call.tools.join(', ')}`
                : ''}</title
            >
          </circle>
        {/each}
      </svg>
      <p class="text-dim">{m.spikeNote}</p>
    {/if}

    <h3 class="mt-8 mb-2 font-semibold">{m.toolResidual}</h3>
    <ul class="flex max-w-[62ch] flex-col gap-1">
      {#each topTools as t (t.name)}
        <li class="flex items-center gap-2">
          <span class="w-40 truncate font-mono" title={t.name}>{t.name}</span>
          <span class="h-3 flex-1 bg-border">
            <span class="block h-3 bg-accent" style:width="{(t.residual / maxResidual) * 100}%"
            ></span>
          </span>
          <span class="w-16 text-right font-mono tabular-nums">{human(t.residual)}</span>
          <span class="w-16 text-right font-mono text-dim tabular-nums">{dollars(t.residual_cost)}</span>
        </li>
      {/each}
    </ul>

    {#if report.subagents.children.length}
      <h3 class="mt-8 mb-2 font-semibold">{m.subagentsHeading}</h3>
      <div class="max-w-[62ch] overflow-x-auto">
        <table class="w-full border-collapse">
          <thead>
            <tr>
              <th scope="col">AGENT</th>
              <th scope="col" class="text-right">CALLS</th>
              <th scope="col" class="text-right">RESIDUAL</th>
              <th scope="col" class="text-right">$</th>
            </tr>
          </thead>
          <tbody>
            {#each report.subagents.children as c (c.session)}
              <tr class="hover:bg-surface">
                <td>
                  <a class="hover:text-accent hover:underline" href="#/s/{c.session}"
                    >{c.agent_type ?? m.subagent}</a
                  >
                </td>
                <td class="text-right font-mono tabular-nums">{c.call_count}</td>
                <td class="text-right font-mono tabular-nums">{human(c.residual)}</td>
                <td class="text-right font-mono tabular-nums">{dollars(c.cost)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  {/if}
</section>
