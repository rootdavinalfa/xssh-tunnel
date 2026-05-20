<script>
  import { Button } from '$lib/components/ui/button';
  import { connectTunnel, disconnectTunnel, syncConnectionState } from '$lib/tauri';
  import { connectionState, connectionError } from '$lib/stores/connection';
  import { onMount } from 'svelte';

  let loading = $state(false);
  let error = $state('');

  onMount(() => {
    const unlisten = syncConnectionState();
    return () => { unlisten.then(fn => fn()); };
  });

  async function handleConnect() {
    loading = true;
    error = '';
    try {
      await connectTunnel();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      connectionError.set(error);
    } finally {
      loading = false;
    }
  }

  async function handleDisconnect() {
    loading = true;
    error = '';
    try {
      await disconnectTunnel();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  let stateColor = $derived(() => {
    switch ($connectionState) {
      case 'tunnel-active': return 'text-green-500';
      case 'connecting': return 'text-yellow-500';
      case 'authenticating': return 'text-blue-500';
      case 'error': return 'text-red-500';
      default: return 'text-gray-500';
    }
  });
</script>

<div class="flex flex-col items-center justify-center min-h-screen gap-4 p-8">
  <h1 class="text-3xl font-bold">XSSH Tunnel</h1>
  <p class="text-muted-foreground">Milestone 1 — Core Tunnel</p>

  <div class="flex flex-col items-center gap-2 mt-4">
    <p class="text-lg">
      Status: <span class={stateColor()}>{$connectionState}</span>
    </p>

    {#if $connectionState === 'disconnected'}
      <Button onclick={handleConnect} disabled={loading}>
        {loading ? 'Connecting...' : 'Connect'}
      </Button>
    {:else}
      <Button onclick={handleDisconnect} disabled={loading} variant="destructive">
        {loading ? 'Disconnecting...' : 'Disconnect'}
      </Button>
    {/if}
  </div>

  {#if error}
    <p class="mt-4 text-lg text-red-500">{error}</p>
  {/if}
</div>