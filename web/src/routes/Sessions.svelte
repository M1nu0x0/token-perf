<script lang="ts">
  import type { messages } from '../messages';
  import type { Report } from '../lib/api';
  import { dollars, human } from '../lib/format';
  import { go, route } from '../lib/router.svelte';

  let { m, sessions }: { m: ReturnType<typeof messages>; sessions: Report[] } = $props();

  // A session and its subagents are billed separately; either unpriced makes the row unpriced.
  const total = (s: Report) =>
    s.cost === null || s.subagents.cost === null ? null : s.cost + s.subagents.cost;

  const keyOf: Record<string, (s: Report) => number | string> = {
    date: (s) => s.started_at,
    calls: (s) => s.call_count,
    cache_read: (s) => s.totals.cache_read,
    sub_read: (s) => s.subagents.totals.cache_read,
    output: (s) => s.totals.output,
    cost: (s) => total(s) ?? -1,
  };

  const sortKey = $derived((route.sort ?? '').replace(/^-/, ''));
  const asc = $derived((route.sort ?? '').startsWith('-'));

  // The URL is the only state: sort, filter and the subagent toggle all live there.
  function apply(patch: { q?: string; sort?: string; sub?: boolean }, push = false) {
    const p = new URLSearchParams();
    const q = patch.q ?? route.q ?? '';
    const sort = patch.sort ?? route.sort ?? '';
    const sub = patch.sub ?? route.sub ?? false;
    if (q) p.set('q', q);
    if (sort) p.set('sort', sort);
    if (sub) p.set('sub', '1');
    const hash = `#/sessions${p.size ? `?${p}` : ''}`;
    if (push) go(hash);
    else location.replace(hash);
  }

  const sortBy = (k: string) => apply({ sort: sortKey === k && !asc ? `-${k}` : k }, true);

  const rows = $derived.by(() => {
    const q = (route.q ?? '').toLowerCase();
    const list = sessions.filter(
      (s) =>
        (route.sub || !s.parent) &&
        (!q || `${s.title} ${s.project} ${s.session}`.toLowerCase().includes(q)),
    );
    const key = Object.hasOwn(keyOf, sortKey) ? keyOf[sortKey] : undefined;
    if (!key) return list;
    return list.sort((a, b) => {
      const [x, y] = [key(a), key(b)];
      return (x < y ? -1 : x > y ? 1 : 0) * (asc ? 1 : -1);
    });
  });
</script>

{#snippet col(key: string, label: string, right = true)}
  <th scope="col" class={right ? 'text-right' : ''} aria-sort={sortKey === key ? (asc ? 'ascending' : 'descending') : 'none'}>
    <button class="hover:text-accent" onclick={() => sortBy(key)}>
      {label}<span aria-hidden="true">{sortKey === key ? (asc ? ' ↑' : ' ↓') : ''}</span>
    </button>
  </th>
{/snippet}

<section>
  <h2 class="mb-2 flex flex-wrap items-baseline justify-between gap-2">
    <span class="font-semibold">
      {m.sessionsHeading} <small class="font-normal text-dim">{m.sessionCount(rows.length)}</small>
    </span>
    <span class="flex items-center gap-4">
      <input
        class="rounded-md border border-border bg-surface px-2 py-1 font-normal"
        type="search"
        placeholder={m.searchPlaceholder}
        aria-label={m.searchPlaceholder}
        value={route.q ?? ''}
        oninput={(e) => apply({ q: e.currentTarget.value })}
      />
      <label class="font-normal text-dim">
        <input
          type="checkbox"
          checked={route.sub ?? false}
          onchange={(e) => apply({ sub: e.currentTarget.checked })}
        />
        {m.includeSubagents}
      </label>
    </span>
  </h2>

  <div class="overflow-x-auto">
    <table class="w-full border-collapse">
    <thead>
      <tr>
        {@render col('date', 'DATE', false)}
        <th scope="col">SESSION</th>
        {@render col('calls', 'CALLS')}
        {@render col('cache_read', 'CACHE_RD')}
        {@render col('sub_read', 'SUB_RD')}
        {@render col('output', 'OUTPUT')}
        {@render col('cost', '$')}
      </tr>
    </thead>
    <tbody>
      {#each rows as s (s.session)}
        <tr class="hover:bg-surface">
          <td class="font-mono whitespace-nowrap text-dim">{s.started_at.slice(0, 10)}</td>
          <td title={s.session}>
            {#if s.parent}<span class="text-dim">↳</span>{/if}
            <a class="hover:text-accent hover:underline" href="#/s/{s.session}">
              {s.title || s.project.split('/').pop()}
            </a>
            <span class="font-mono text-dim">{s.title ? s.project.split('/').pop() : ''}</span>
          </td>
          <td class="text-right font-mono tabular-nums">{s.call_count}</td>
          <td class="text-right font-mono tabular-nums">{human(s.totals.cache_read)}</td>
          <td class="text-right font-mono tabular-nums"
            >{s.subagents.count ? human(s.subagents.totals.cache_read) : ''}</td
          >
          <td class="text-right font-mono tabular-nums">{human(s.totals.output)}</td>
          <td class="text-right font-mono tabular-nums">{dollars(total(s))}</td>
        </tr>
      {/each}
    </tbody>
    </table>
  </div>
  {#if !rows.length}<p class="mt-2 text-dim">{m.noMatch}</p>{/if}
</section>
