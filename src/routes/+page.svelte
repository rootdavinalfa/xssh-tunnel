<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { getProfiles, deleteProfile, connectTunnel, disconnectTunnel, syncConnectionState } from '$lib/tauri';
  import { profiles } from '$lib/stores/profiles';
  import { connectionState } from '$lib/stores/connection';
  import { onMount } from 'svelte';

  let loading = $state(false);
  let error = $state('');

  onMount(() => {
    const unlisten = syncConnectionState();
    loadProfiles();
    return () => { unlisten.then(fn => fn()); };
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
</div>