<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { getProfiles, deleteProfile, connectTunnel, disconnectTunnel, syncConnectionState, getLogs, syncLogs, parseSshConfig, importSshConfig } from '$lib/tauri';
  import type { ParseResult } from '$lib/tauri';
  import { profiles } from '$lib/stores/profiles';
  import { connectionState } from '$lib/stores/connection';
  import { logEntries, clearStore } from '$lib/stores/logs';
  import { onMount } from 'svelte';

  let loading = $state(false);
  let error = $state('');
  let importLoading = $state(false);
  let importError = $state('');
  let parseResult = $state<ParseResult | null>(null);
  let selectedHosts = $state<Set<string>>(new Set());

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

  let selectAll = $derived(
    parseResult ? selectedHosts.size === parseResult.entries.length : false
  );
  let selectedCount = $derived(selectedHosts.size);

  function toggleAll() {
    if (!parseResult) return;
    if (selectAll) {
      selectedHosts = new Set();
    } else {
      selectedHosts = new Set(parseResult.entries.map(e => e.host_aliases[0]));
    }
  }

  function toggleHost(host: string) {
    const next = new Set(selectedHosts);
    if (next.has(host)) {
      next.delete(host);
    } else {
      next.add(host);
    }
    selectedHosts = next;
  }

  async function handleParse() {
    importLoading = true;
    importError = '';
    try {
      parseResult = await parseSshConfig();
      selectedHosts = new Set(parseResult.entries.map(e => e.host_aliases[0]));
    } catch (e) {
      importError = String(e);
    } finally {
      importLoading = false;
    }
  }

  async function handleImport() {
    importLoading = true;
    importError = '';
    try {
      await importSshConfig(Array.from(selectedHosts));
      parseResult = null;
      await loadProfiles();
    } catch (e) {
      importError = String(e);
    } finally {
      importLoading = false;
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
    <div class="flex gap-2">
      <Button variant="outline" onclick={async () => { await handleParse(); }}>
        Import from SSH Config
      </Button>
      <Button onclick={() => window.location.href = '/connections/new'}>
        + New Connection
      </Button>
    </div>
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
                onclick={() => window.location.href = `/connections/${profile.id}/edit`} 
                variant="outline"
                size="sm"
              >
                Edit
              </Button>
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

  <!-- Import Dialog -->
  {#if parseResult}
    <div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onclick={() => parseResult = null}>
      <div class="bg-white rounded-lg shadow-xl max-w-3xl w-full mx-4 max-h-[80vh] overflow-y-auto" onclick={(e) => e.stopPropagation()}>
        <div class="p-6">
          <div class="flex justify-between items-center mb-4">
            <h2 class="text-xl font-bold">Import from ~/.ssh/config</h2>
            <button onclick={() => parseResult = null} class="text-gray-400 hover:text-gray-600 text-2xl">&times;</button>
          </div>

          {#if importError}
            <p class="text-red-500 mb-4">{importError}</p>
          {/if}

          {#if importLoading}
            <p class="text-center text-muted-foreground py-8">Importing...</p>
          {:else}
            <p class="text-sm text-muted-foreground mb-4">
              Found {parseResult.entries.length} hosts
              {#if parseResult.skipped.length > 0}
                (skipped: {parseResult.skipped.join(', ')})
              {/if}
            </p>

            {#if parseResult.entries.length > 0}
              <table class="w-full border-collapse">
                <thead>
                  <tr class="border-b">
                    <th class="text-left py-2 px-2">
                      <input type="checkbox" checked={selectAll} onchange={toggleAll} />
                    </th>
                    <th class="text-left py-2 px-2 text-sm font-medium">Host</th>
                    <th class="text-left py-2 px-2 text-sm font-medium">Hostname</th>
                    <th class="text-left py-2 px-2 text-sm font-medium">User</th>
                    <th class="text-left py-2 px-2 text-sm font-medium">Port</th>
                    <th class="text-left py-2 px-2 text-sm font-medium">Key File</th>
                  </tr>
                </thead>
                <tbody>
                  {#each parseResult.entries as entry}
                    <tr class="border-b hover:bg-gray-50">
                      <td class="py-2 px-2">
                        <input
                          type="checkbox"
                          checked={selectedHosts.has(entry.host_aliases[0])}
                          onchange={() => toggleHost(entry.host_aliases[0])}
                        />
                      </td>
                      <td class="py-2 px-2 text-sm">{entry.host_aliases[0]}</td>
                      <td class="py-2 px-2 text-sm">{entry.hostname}</td>
                      <td class="py-2 px-2 text-sm">{entry.user || '-'}</td>
                      <td class="py-2 px-2 text-sm">{entry.port || 22}</td>
                      <td class="py-2 px-2 text-sm">{entry.identity_file ? 'Yes' : 'No'}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>

              <div class="flex justify-end gap-2 mt-6">
                <Button variant="outline" onclick={() => { parseResult = null; }}>
                  Cancel
                </Button>
                <Button onclick={handleImport} disabled={selectedCount === 0}>
                  Import {selectedCount} {selectedCount === 1 ? 'profile' : 'profiles'}
                </Button>
              </div>
            {:else}
              <p class="text-center text-muted-foreground py-8">
                No importable hosts found.
              </p>
              <div class="flex justify-end">
                <Button variant="outline" onclick={() => { parseResult = null; }}>
                  Close
                </Button>
              </div>
            {/if}
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>