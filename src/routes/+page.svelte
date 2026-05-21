<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import ProfileCard from '$lib/components/profile-card.svelte';
  import LogPanel from '$lib/components/log-panel.svelte';
  import ImportDialog from '$lib/components/import-dialog.svelte';
  import ConfirmDialog from '$lib/components/confirm-dialog.svelte';
  import ConnectionFormDialog from '$lib/components/connection-form-dialog.svelte';
  import ConnectionStats from '$lib/components/connection-stats.svelte';
  import {
    getProfiles, deleteProfile, connectTunnel, disconnectTunnel,
    syncConnectionState, syncConnectionStats, getConnectionState, getLogs, syncLogs,
    parseSshConfig, importSshConfig, getHelperStatus,
  } from '$lib/tauri';
  import type { ParseResult } from '$lib/tauri';
  import { profiles } from '$lib/stores/profiles';
  import { connectionState } from '$lib/stores/connection';
  import { logEntries } from '$lib/stores/logs';
  import { helperStatus } from '$lib/stores/helper';
  import { toast } from 'svelte-sonner';
  import { onMount } from 'svelte';

  let loading = $state(false);
  let disconnecting = $state(false);
  let dismissedBanner = $state(false);

  // Connection form dialog state
  let showConnForm = $state(false);
  let connFormMode = $state<'new' | 'edit'>('new');
  let editProfileId = $state('');

  // Import dialog state
  let showImportDialog = $state(false);
  let importLoading = $state(false);
  let importError = $state('');
  let parseResult = $state<ParseResult | null>(null);

  // Confirmation dialog state
  let showConfirm = $state(false);
  let confirmTitle = $state('');
  let confirmDescription = $state('');
  let confirmCallback = $state<() => void>(() => {});

  function showConfirmDialog(title: string, description: string, onConfirm: () => void) {
    confirmTitle = title;
    confirmDescription = description;
    confirmCallback = onConfirm;
    showConfirm = true;
  }

  onMount(() => {
    const unlisten = syncConnectionState();
    const unlistenStats = syncConnectionStats();
    const unlistenLogs = syncLogs();
    loadProfiles();
    getConnectionState().then(state => {
      if (state === 'connected') connectionState.set('tunnel-active');
    }).catch(() => {});
    getLogs(undefined, 10).then(logs => {
      if (logs.length > 0) logEntries.set(logs);
    }).catch(() => {});
    getHelperStatus().catch(() => {});
    return () => {
      unlisten.then(fn => fn());
      unlistenStats.then(fn => fn());
      unlistenLogs.then(fn => fn());
    };
  });

  async function loadProfiles() {
    try {
      const data = await getProfiles();
      profiles.set(data);
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function handleConnect(profileId: string) {
    loading = true;
    try {
      await connectTunnel(profileId);
    } catch (e: unknown) {
      toast.error(String(e));
    } finally {
      loading = false;
    }
  }

  async function handleDisconnect() {
    if (disconnecting) return;
    disconnecting = true;
    try {
      await disconnectTunnel();
    } catch (e: unknown) {
      toast.error(String(e));
    } finally {
      disconnecting = false;
    }
  }

  function handleNewClick() {
    connFormMode = 'new';
    editProfileId = '';
    showConnForm = true;
  }

  function handleEditClick(id: string) {
    if ($connectionState !== 'disconnected') {
      showConfirmDialog(
        'Disconnect to Edit?',
        'A tunnel is currently connected. The tunnel will be disconnected before editing.',
        async () => {
          try {
            await disconnectTunnel();
          } catch (e) { /* proceed anyway */ }
          editProfileId = id;
          connFormMode = 'edit';
          showConnForm = true;
        }
      );
    } else {
      editProfileId = id;
      connFormMode = 'edit';
      showConnForm = true;
    }
  }

  function handleDeleteClick(id: string) {
    if ($connectionState !== 'disconnected') {
      showConfirmDialog(
        'Disconnect and Delete?',
        'A tunnel is currently connected. Disconnect the tunnel and delete this profile?',
        async () => {
          try {
            await disconnectTunnel();
            await deleteProfile(id);
            await loadProfiles();
          } catch (e: unknown) {
            toast.error(String(e));
          }
        }
      );
    } else {
      showConfirmDialog(
        'Delete Profile?',
        'Are you sure you want to delete this profile? This cannot be undone.',
        async () => {
          try {
            await deleteProfile(id);
            await loadProfiles();
          } catch (e: unknown) {
            toast.error(String(e));
          }
        }
      );
    }
  }

  async function handleParseAndOpenImport() {
    importLoading = true;
    importError = '';
    try {
      parseResult = await parseSshConfig();
      showImportDialog = true;
    } catch (e) {
      importError = String(e);
      showImportDialog = true;
    } finally {
      importLoading = false;
    }
  }

  async function handleImport(selectedHosts: string[]) {
    importLoading = true;
    importError = '';
    try {
      await importSshConfig(selectedHosts);
      showImportDialog = false;
      parseResult = null;
      await loadProfiles();
    } catch (e) {
      importError = String(e);
    } finally {
      importLoading = false;
    }
  }

  function handleCloseImport() {
    showImportDialog = false;
    parseResult = null;
  }

  function handleFormSaved() {
    loadProfiles();
  }
</script>

<div class="container mx-auto p-6 max-w-4xl">
  <!-- Header -->
  <div class="flex justify-between items-center mb-6">
    <h1 class="text-3xl font-bold">XSSH Tunnel</h1>
    <div class="flex gap-2">
      <Button
        variant="outline"
        onclick={handleParseAndOpenImport}
        disabled={importLoading}
      >
        Import from SSH Config
      </Button>
      <Button
        variant="ghost"
        onclick={() => window.location.href = '/settings'}
        size="sm"
      >
        ⚙ Settings
      </Button>
      <Button onclick={handleNewClick}>
        + New Connection
      </Button>
    </div>
  </div>

  {#if !$helperStatus.installed && !dismissedBanner}
    <div class="bg-yellow-50 border border-yellow-200 rounded-lg px-4 py-3 mb-4 flex items-center justify-between">
      <div class="flex items-center gap-2">
        <span class="text-yellow-600 text-sm">
          ⚠ Privileged Helper not installed. TUN device creation requires it.
        </span>
        <Button variant="outline" size="sm" onclick={() => window.location.href = '/settings'}>
          Install
        </Button>
      </div>
      <button onclick={() => dismissedBanner = true} class="text-yellow-400 hover:text-yellow-600 text-lg leading-none">
        &times;
      </button>
    </div>
  {/if}

  <!-- Connection Stats Bar -->
  <ConnectionStats />

  <!-- Profile list -->
  <div class="space-y-4">
    {#each $profiles as profile (profile.id)}
      <ProfileCard
        {profile}
        connectionState={$connectionState}
        {loading}
        {disconnecting}
        onConnect={handleConnect}
        onDisconnect={handleDisconnect}
        onEdit={handleEditClick}
        onDelete={handleDeleteClick}
      />
    {/each}

    {#if $profiles.length === 0}
      <p class="text-center text-muted-foreground py-8">
        No connections yet. Click "New Connection" to add one.
      </p>
    {/if}
  </div>

  <!-- Logs Panel -->
  <LogPanel entries={$logEntries} />

  <!-- Connection Form Dialog (New / Edit) -->
  <ConnectionFormDialog
    bind:open={showConnForm}
    mode={connFormMode}
    profileId={editProfileId}
    onSave={handleFormSaved}
  />

  <!-- Import Dialog -->
  <ImportDialog
    bind:open={showImportDialog}
    {parseResult}
    {importLoading}
    {importError}
    onImport={handleImport}
  />

  <!-- Confirmation Dialog -->
  <ConfirmDialog
    bind:open={showConfirm}
    title={confirmTitle}
    description={confirmDescription}
    confirmLabel="Continue"
    variant="destructive"
    onConfirm={confirmCallback}
  />
</div>
