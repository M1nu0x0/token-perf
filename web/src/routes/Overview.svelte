<script lang="ts">
  import type { messages } from '../messages';
  import type { Tldr } from '../lib/api';
  import { dollars, human, pct } from '../lib/format';

  let { m, tldr }: { m: ReturnType<typeof messages>; tldr: Tldr | null } = $props();

  // An empty bucket has ratio 0, which would make the sentence below lie.
  const short = $derived(tldr?.amplification.at(0));
  const long = $derived(tldr?.amplification.at(-1));

  const usage = $derived.by(() => {
    const t = tldr?.totals;
    const items = [
      { label: m.labelInput, value: t?.input ?? 0, fill: 'fill-dim', chip: 'bg-dim' },
      {
        label: m.labelCacheWrite,
        value: (t?.cache_write_5m ?? 0) + (t?.cache_write_1h ?? 0),
        fill: 'fill-accent/55',
        chip: 'bg-accent/55',
      },
      { label: m.labelCacheRead, value: t?.cache_read ?? 0, fill: 'fill-accent', chip: 'bg-accent' },
      { label: m.labelOutput, value: t?.output ?? 0, fill: 'fill-dim/45', chip: 'bg-dim/45' },
    ];
    const total = items.reduce((sum, i) => sum + i.value, 0) || 1;
    let x = 0;
    return items.map((i) => {
      const w = (i.value / total) * 100;
      const part = { ...i, x, w };
      x += w;
      return part;
    });
  });
</script>

{#if tldr}
  <section>
    <p class="text-dim">{m.lead(tldr.sessions, human(tldr.calls))}</p>

    <div class="mt-4 grid gap-4 sm:grid-cols-3">
      <div class="rounded-md border border-border bg-surface p-4">
        <div class="text-dim">{m.cardCacheRead}</div>
        <div class="font-mono text-4xl font-bold tracking-tight tabular-nums">
          {pct(tldr.cache_read_pct)}
        </div>
        <div class="font-mono text-dim">{human(tldr.totals.cache_read)}</div>
      </div>
      <div class="rounded-md border border-border bg-surface p-4">
        <div class="text-dim">{m.cardAmplification}</div>
        <div class="font-mono text-4xl font-bold tracking-tight tabular-nums">
          {long?.sessions ? m.multiplier(long.ratio.toFixed(1)) : '—'}
        </div>
        <div class="text-dim">{long ? m.bucket(long) : ''}</div>
      </div>
      <div class="rounded-md border border-border bg-surface p-4">
        <div class="text-dim">{m.cardCost}</div>
        <div class="font-mono text-4xl font-bold tracking-tight tabular-nums">
          {dollars(tldr.cost)}
        </div>
        <div class="text-dim">{m.costNote}</div>
      </div>
    </div>

    <p class="mt-6 max-w-[62ch]">
      {@html m.cacheRead(
        tldr.cache_read_pct.toFixed(0),
        human(tldr.totals.output),
        human(tldr.totals.cache_read),
      )}
    </p>

    <h3 class="mt-6 mb-2 font-semibold">{m.usageHeading}</h3>
    <svg class="h-4 w-full" viewBox="0 0 100 4" preserveAspectRatio="none" role="img">
      <title
        >{usage.map((u) => `${u.label} ${u.w.toFixed(0)}%`).join(', ')}</title
      >
      {#each usage as u (u.label)}
        <rect class={u.fill} x={u.x} y="0" width={u.w} height="4" />
      {/each}
    </svg>
    <ul class="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-dim">
      {#each usage as u (u.label)}
        <li class="flex items-center gap-1.5">
          <span class="inline-block size-2 {u.chip}"></span>
          {u.label}
          <span class="font-mono tabular-nums">{pct(u.w)}</span>
        </li>
      {/each}
    </ul>

    <h3 class="mt-8 mb-2 font-semibold">{m.culpritHeading}</h3>
    <ul class="flex max-w-[62ch] flex-col gap-1">
      {#each tldr.top_tools as t (t.name)}
        <li class="flex items-center gap-2">
          <span class="w-40 truncate font-mono" title={t.name}>{t.name}</span>
          <span class="h-3 flex-1 bg-border">
            <span class="block h-3 bg-accent" style:width="{t.pct}%"></span>
          </span>
          <span class="w-20 text-right font-mono tabular-nums">{pct(t.pct)}</span>
          <span class="w-16 text-right font-mono text-dim tabular-nums">{human(t.added)}</span>
        </li>
      {/each}
    </ul>
    <p class="mt-2 max-w-[62ch] text-dim">{m.culpritTail}</p>

    <h3 class="mt-8 mb-1 font-semibold">{m.lengthHeading}</h3>
    <p class="max-w-[62ch]">
      {m.lengthIntro}
      {#if short?.sessions && long?.sessions}
        {@html m.lengthCompare(short.ratio.toFixed(1), long.ratio.toFixed(1))}
      {/if}
    </p>
    <div class="mt-2 overflow-x-auto">
      <table class="w-full max-w-[62ch] border-collapse">
      <thead
        ><tr><th scope="col">LENGTH</th><th scope="col" class="text-right">SESSIONS</th><th scope="col" class="text-right">RATIO</th></tr
        ></thead
      >
      <tbody>
        {#each tldr.amplification as b (b.min)}
          <tr>
            <td>{m.bucket(b)}</td>
            <td class="text-right font-mono tabular-nums">{b.sessions}</td>
            <td class="text-right font-mono tabular-nums">{m.times(b.ratio.toFixed(1))}</td>
          </tr>
        {/each}
      </tbody>
      </table>
    </div>

    <h3 class="mt-8 mb-1 font-semibold">{m.topHeading}</h3>
    <div class="overflow-x-auto">
      <table class="w-full max-w-[62ch] border-collapse">
      <thead
        ><tr
          ><th scope="col">SESSION</th><th scope="col" class="text-right">CALLS</th><th scope="col" class="text-right">RESIDUAL</th><th
            scope="col" class="text-right">SUB</th
          ></tr
        ></thead
      >
      <tbody>
        {#each tldr.top_sessions as s (s.session)}
          <tr class="hover:bg-surface">
            <td><a class="hover:text-accent hover:underline" href="#/s/{s.session}">{s.title}</a></td>
            <td class="text-right font-mono tabular-nums">{s.call_count}</td>
            <td class="text-right font-mono tabular-nums">{human(s.residual)}</td>
            <td class="text-right font-mono tabular-nums"
              >{s.sub_calls ? human(s.sub_residual) : ''}</td
            >
          </tr>
        {/each}
      </tbody>
      </table>
    </div>

    <h3 class="mt-8 mb-1 font-semibold">{m.soloHeading}</h3>
    <p class="max-w-[62ch]">{@html m.soloLine(tldr.solo_tool_pct.toFixed(0))}</p>
  </section>
{/if}
