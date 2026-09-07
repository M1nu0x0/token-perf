<script lang="ts">
  import { messages } from './messages';
  import { getConfig, getSessions, getTldr, type Report, type Tldr } from './lib/api';
  import { route } from './lib/router.svelte';
  import Overview from './routes/Overview.svelte';
  import Sessions from './routes/Sessions.svelte';
  import SessionDetail from './routes/SessionDetail.svelte';

  let m = $state(messages('en'));
  let tldr = $state<Tldr | null>(null);
  let sessions = $state<Report[]>([]);
  let error = $state('');
  let loading = $state(true);

  const tab = 'px-3 py-1 rounded-md hover:text-accent aria-[current=page]:font-semibold' +
    ' aria-[current=page]:text-accent aria-[current=page]:border-b-2';

  async function load() {
    [error, loading] = ['', true];
    try {
      const [t, s, cfg] = await Promise.all([getTldr(), getSessions(), getConfig()]);
      [tldr, sessions, m] = [t, s, messages(cfg.lang)];
    } catch (e) {
      error = String(e);
    }
    loading = false;
  }

  load();
</script>

<div class="mx-auto max-w-[1280px] px-6 py-6">
  <header class="mb-6 flex items-baseline justify-between border-b border-border pb-3">
    <h1 class="font-mono text-lg font-semibold">token-perf</h1>
    <nav class="flex gap-1">
      <a class={tab} href="#/" aria-current={route.name === 'overview' ? 'page' : undefined}>{m.tabTldr}</a>
      <a
        class={tab}
        href="#/sessions"
        aria-current={route.name === 'overview' ? undefined : 'page'}>{m.tabSessions}</a
      >
    </nav>
  </header>

  {#if error}
    <p class="text-warn">
      {error}
      <button class="ml-2 underline hover:text-accent" onclick={load}>{m.retry}</button>
    </p>
  {:else if loading}
    <p class="text-dim">{m.loading}</p>
  {:else if !sessions.length}
    <p class="text-dim">{m.noSessions}</p>
  {:else if route.name === 'session' && route.id}
    <SessionDetail {m} id={route.id} />
  {:else if route.name === 'sessions'}
    <Sessions {m} {sessions} />
  {:else}
    <Overview {m} {tldr} />
  {/if}
</div>
