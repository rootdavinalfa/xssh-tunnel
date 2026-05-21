<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
  import { Badge } from '$lib/components/ui/badge';
  import { getHelperStatus, installHelper, uninstallHelper } from '$lib/tauri';
  import { helperStatus } from '$lib/stores/helper';

  let installing = $state(false);
  let uninstalling = $state(false);
  let error = $state('');

  async function loadStatus() {
    try {
      await getHelperStatus();
    } catch (e) {
      error = String(e);
    }
  }

  async function handleInstall() {
    installing = true;
    error = '';
    try {
      await installHelper();
    } catch (e) {
      error = String(e);
    } finally {
      installing = false;
    }
  }

  async function handleUninstall() {
    if (!confirm('Uninstall the privileged helper? This will not affect existing profiles.')) return;
    uninstalling = true;
    error = '';
    try {
      await uninstallHelper();
    } catch (e) {
      error = String(e);
    } finally {
      uninstalling = false;
    }
  }

  onMount(loadStatus);
</script>

<div class="container mx-auto p-6 max-w-2xl">
  <div class="flex justify-between items-center mb-6">
    <h1 class="text-2xl font-bold">Settings</h1>
    <Button variant="outline" onclick={() => window.location.href = '/'}>
      Back
    </Button>
  </div>

  {#if error}
    <p class="text-red-500 mb-4">{error}</p>
  {/if}

  <Card>
    <CardHeader>
      <CardTitle>Privileged Helper</CardTitle>
    </CardHeader>
    <CardContent>
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <span class="text-sm font-medium">Status</span>
          {#if $helperStatus.installed}
            <Badge variant={$helperStatus.running ? 'default' : 'secondary'}>
              {$helperStatus.running ? 'Running' : 'Installed'}
            </Badge>
          {:else}
            <Badge variant="outline">Not Installed</Badge>
          {/if}
        </div>

        <p class="text-sm text-muted-foreground">
          The privileged helper creates TUN devices and manages network routes.
          It runs as a root daemon via SMAppService. Installing requires an admin password.
        </p>

        <div class="flex gap-2 pt-2">
          <Button
            onclick={handleInstall}
            disabled={installing || $helperStatus.installed}
          >
            {installing ? 'Installing...' : 'Install Helper'}
          </Button>
          <Button
            variant="outline"
            onclick={handleUninstall}
            disabled={uninstalling || !$helperStatus.installed}
          >
            {uninstalling ? 'Uninstalling...' : 'Uninstall'}
          </Button>
        </div>
      </div>
    </CardContent>
  </Card>

  <Card class="mt-6">
    <CardHeader>
      <CardTitle>About</CardTitle>
    </CardHeader>
    <CardContent>
      <div class="space-y-2 text-sm">
        <p><span class="font-medium">Version:</span> 0.1.0</p>
        <p><span class="font-medium">Identifier:</span> xyz.dvnlabs.xsshtunnel</p>
        <p class="text-muted-foreground">
          XSSH Tunnel — SSH-based VPN tunnel for macOS.
        </p>
      </div>
    </CardContent>
  </Card>
</div>