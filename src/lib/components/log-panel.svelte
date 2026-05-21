<script lang="ts">
  import type { LogEntry } from '$lib/stores/logs';

  let {
    entries = [] as LogEntry[],
  }: {
    entries?: LogEntry[];
  } = $props();

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
</script>

<div class="mt-8">
  <div class="flex justify-between items-center mb-2">
    <h2 class="text-lg font-semibold">Recent Activity</h2>
    <a href="/logs" class="text-sm text-blue-600 hover:underline">View All</a>
  </div>
  <div class="border rounded-lg divide-y max-h-48 overflow-y-auto">
    {#if entries.length === 0}
      <p class="text-sm text-muted-foreground px-4 py-3">
        No activity yet. Connect to a server to see logs.
      </p>
    {:else}
      {#each entries.slice(0, 10) as entry (entry.id)}
        <div class="px-4 py-2 flex gap-3 items-start">
          <span class="text-xs text-gray-400 font-mono whitespace-nowrap">
            {formatTime(entry.timestamp)}
          </span>
          <span class="text-xs font-medium uppercase w-10 {levelClass(entry.level)}">
            {entry.level}
          </span>
          <span class="text-sm flex-1">{entry.message}</span>
        </div>
      {/each}
    {/if}
  </div>
</div>
