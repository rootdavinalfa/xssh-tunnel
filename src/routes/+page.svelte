<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { getProfiles, deleteProfile, connectTunnel, disconnectTunnel, syncConnectionState, getLogs, syncLogs } from '$lib/tauri';
  import { profiles } from '$lib/stores/profiles';
  import { connectionState } from '$lib/stores/connection';
  import { logEntries, clearStore } from '$lib/stores/logs';
  import { onMount } from 'svelte';

  let loading = $state(false);
  let error = $state('');

  onMount(() => {
    const unlisten = syncConnectionState();
    const unlistenLogs = syncLogs();
    loadProfiles();
    getLogs(undefined, 10).then(logs => {
      if (logs.length > 0) logEntries.set(logs);
    }).catch(() => {});
    return () => { 
      unlisten.then(fn => fn());
      unlistenLogs.then(fn => fn());
    };
  });

  async function loadProfiles() {
    try {
      const data = await getProfiles();
      profiles.set(data);
    } catch (e) {
      error = String(e);
    }
  }

  async function handleDelete(id: string) {
    if (!confirm('Delete this profile?')) return;
    try {
      await deleteProfile(id);
      await loadProfiles();
    } catch (e: unknown) {
      error = String(e);
    }
  }

  async function handleConnect(profileId: string) {
    loading = true;
    error = '';
    try {
      await connectTunnel(profileId);
    } catch (e: unknown) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function handleDisconnect() {
    loading = true;
    try {
      await disconnectTunnel();
    } catch (e: unknown) {
      error = String(e);
    } finally {
      loading = false;
    }
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

<div class="container mx-auto p-6 max-w-4xl">
  <div class="flex justify-between items-center mb-6">
    <h1 class="text-3xl font-bold">XSSH Tunnel</h1>
    <Button onclick={() => window.location.href = '/connections/new'}>
      + New Connection
    </Button>
  </div>

  {#if error}
    <p class="text-red-500 mb-4">{error}</p>
  {/if}

  <div class="space-y-4">
    {#each $profiles as profile (profile.id)}
      <div class="border rounded-lg p-4">
        <div class="pb-2">
          <div class="flex justify-between items-start">
            <div>
              <h3 class="font-semibold">{profile.label}</h3>
              <p class="text-sm text-muted-foreground">
                {profile.username}@{profile.host}:{profile.port}
              </p>
            </div>
            <div class="flex gap-2">
              {#if $connectionState === 'disconnected'}
                <Button 
                  onclick={() => handleConnect(profile.id)} 
                  disabled={loading}
                  size="sm"
                >
                  Connect
                </Button>
              {:else}
                <Button 
                  onclick={handleDisconnect} 
                  disabled={loading}
                  variant="destructive"
                  size="sm"
                >
                  Disconnect
                </Button>
              {/if}
              <Button 
                onclick={() => handleDelete(profile.id)} 
                variant="outline"
                size="sm"
              >
                Delete
              </Button>
            </div>
          </div>
        </div>
      </div>
    {/each}

    {#if $profiles.length === 0}
      <p class="text-center text-muted-foreground py-8">
        No connections yet. Click "New Connection" to add one.
      </p>
    {/if}
  </div>

  <!-- Logs Panel -->
  <div class="mt-8">
    <div class="flex justify-between items-center mb-2">
      <h2 class="text-lg font-semibold">Recent Activity</h2>
      <a href="/logs" class="text-sm text-blue-600 hover:underline">View All</a>
    </div>
    <div class="border rounded-lg divide-y max-h-48 overflow-y-auto">
      {#if $logEntries.length === 0}
        <p class="text-sm text-muted-foreground px-4 py-3">
          No activity yet. Connect to a server to see logs.
        </p>
      {:else}
        {#each $logEntries.slice(0, 10) as entry (entry.id)}
          <div class="px-4 py-2 flex gap-3 items-start">
            <span class="text-xs text-gray-400 font-mono whitespace-nowrap">
              {entry.timestamp.slice(11, 19)}
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
</div>