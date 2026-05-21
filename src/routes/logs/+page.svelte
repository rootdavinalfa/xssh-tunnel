<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { logEntries, clearStore } from '$lib/stores/logs';
  import { getLogs, clearLogs, syncLogs } from '$lib/tauri';
  import { onMount } from 'svelte';

  let filterLevel = $state('');
  let searchQuery = $state('');
  let loading = $state(true);

  let filteredEntries = $derived.by(() => {
    let entries = $logEntries;
    if (filterLevel) {
      entries = entries.filter(e => e.level === filterLevel);
    }
    if (searchQuery) {
      const q = searchQuery.toLowerCase();
      entries = entries.filter(e => e.message.toLowerCase().includes(q));
    }
    return entries;
  });

  function formatTime(ts: string): string {
    return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  }

  function levelClass(level: string): string {
    switch (level) {
      case 'error': return 'text-red-600';
      case 'warn': return 'text-yellow-600';
      case 'info': return 'text-gray-600';
      case 'debug': return 'text-blue-600';
      default: return '';
    }
  }

  async function handleClear() {
    await clearLogs();
    clearStore();
  }

  onMount(() => {
    getLogs().then(logs => {
      logEntries.set(logs);
      loading = false;
    }).catch(e => {
      console.error('Failed to load logs:', e);
      loading = false;
    });
    const unlisten = syncLogs();
    return () => { unlisten.then(fn => fn()); };
  });
</script>

<div class="container mx-auto p-6 max-w-5xl">
  <div class="flex justify-between items-center mb-6">
    <h1 class="text-2xl font-bold">Connection Logs</h1>
    <Button variant="outline" onclick={() => window.location.href = '/'}>
      Back
    </Button>
  </div>

  <div class="flex gap-4 mb-4">
    <select
      bind:value={filterLevel}
      class="border rounded-md p-2"
    >
      <option value="">All Levels</option>
      <option value="info">Info</option>
      <option value="warn">Warning</option>
      <option value="error">Error</option>
      <option value="debug">Debug</option>
    </select>

    <Input
      placeholder="Search messages..."
      bind:value={searchQuery}
      class="flex-1"
    />

    <Button variant="outline" onclick={handleClear}>
      Clear Logs
    </Button>
  </div>

  {#if loading}
    <p class="text-center text-muted-foreground py-8">Loading logs...</p>
  {:else if filteredEntries.length === 0}
    <p class="text-center text-muted-foreground py-8">
      {#if $logEntries.length === 0}
        No logs yet. Connect to a server to see activity.
      {:else}
        No logs match your filter.
      {/if}
    </p>
  {:else}
    <div class="border rounded-lg divide-y">
      {#each filteredEntries as entry (entry.id)}
        <div class="px-4 py-3 flex gap-4 items-start">
          <span class="text-sm text-gray-400 font-mono whitespace-nowrap">
            {formatTime(entry.timestamp)}
          </span>
          <span class="text-xs font-medium uppercase w-12 {levelClass(entry.level)}">
            {entry.level}
          </span>
          <span class="text-sm flex-1">{entry.message}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>
