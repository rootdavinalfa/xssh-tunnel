<script lang="ts">
  import { connectionState, connectionStats } from '$lib/stores/connection';

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  function formatDuration(secs: number): string {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = secs % 60;
    if (h > 0) return `${h}h ${m}m ${s}s`;
    if (m > 0) return `${m}m ${s}s`;
    return `${s}s`;
  }
</script>

{#if $connectionState === 'tunnel-active' && $connectionStats}
  <div class="bg-muted rounded-lg px-4 py-3 mb-4">
    <div class="flex items-center justify-between text-sm">
      <div class="flex items-center gap-3">
        <span class="inline-block w-2 h-2 rounded-full bg-green-500"></span>
        <span class="font-medium">Connected</span>
        <span class="text-muted-foreground">|</span>
        <span>↑ {formatBytes($connectionStats.bytes_up)}</span>
        <span>↓ {formatBytes($connectionStats.bytes_down)}</span>
      </div>
      <span class="text-muted-foreground">{formatDuration($connectionStats.uptime_secs)}</span>
    </div>
  </div>
{:else if $connectionState === 'reconnecting'}
  <div class="bg-yellow-50 border border-yellow-200 rounded-lg px-4 py-3 mb-4">
    <span class="text-sm text-yellow-700">⟳ Reconnecting...</span>
  </div>
{/if}