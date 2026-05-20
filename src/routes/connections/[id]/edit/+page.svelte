<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { getProfileById, updateProfile } from '$lib/tauri';

  let label = $state('');
  let host = $state('');
  let port = $state(22);
  let username = $state('');
  let authType = $state('password');
  let identityFilePath = $state('');
  let changeCredentials = $state(false);
  let password = $state('');
  let privateKey = $state('');
  let keyPassphrase = $state('');
  let hasExistingCredentials = $state(false);
  let error = $state('');
  let saving = $state(false);
  let loading = $state(true);

  let profileId = $derived($page.params.id ?? '');

  onMount(async () => {
    if (!profileId) {
      error = 'Invalid profile ID';
      loading = false;
      return;
    }
    try {
      const profile = await getProfileById(profileId);
      label = profile.label;
      host = profile.host;
      port = profile.port;
      username = profile.username;
      authType = profile.auth_type;
      identityFilePath = profile.identity_file_path || '';
      hasExistingCredentials = profile.auth_type === 'password' || profile.auth_type === 'key_inline';
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    saving = true;
    error = '';

    const req: any = {
      id: profileId,
      label,
      host,
      port,
      username,
      auth_type: authType,
      identity_file_path: identityFilePath || null,
    };

    if (changeCredentials) {
      if (authType === 'password') {
        req.password = password;
      } else if (authType === 'key_inline') {
        req.private_key = privateKey;
        if (keyPassphrase) req.key_passphrase = keyPassphrase;
      }
    }

    try {
      await updateProfile(req);
      window.location.href = '/';
    } catch (e) {
      error = String(e);
      saving = false;
    }
  }
</script>

<div class="container mx-auto p-6 max-w-lg">
  <h1 class="text-2xl font-bold mb-6">Edit Connection</h1>

  {#if loading}
    <p class="text-center text-muted-foreground py-8">Loading profile...</p>
  {:else}
    {#if error}
      <p class="text-red-500 mb-4">{error}</p>
    {/if}

    <form onsubmit={handleSubmit} class="space-y-4">
      <div>
        <label for="label">Label</label>
        <Input id="label" bind:value={label} placeholder="My Server" required />
      </div>

      <div>
        <label for="host">Host</label>
        <Input id="host" bind:value={host} placeholder="192.168.1.1" required />
      </div>

      <div>
        <label for="port">Port</label>
        <Input id="port" type="number" bind:value={port} />
      </div>

      <div>
        <label for="username">Username</label>
        <Input id="username" bind:value={username} placeholder="root" required />
      </div>

      <div>
        <label for="authType">Authentication</label>
        <select id="authType" bind:value={authType} class="w-full border rounded-md p-2">
          <option value="password">Password</option>
          <option value="key_inline">Private Key (Inline)</option>
          <option value="key_file">Key File</option>
          <option value="agent">SSH Agent</option>
        </select>
      </div>

      {#if hasExistingCredentials}
        <div class="border rounded-lg p-4">
          <label class="flex items-center gap-2 cursor-pointer">
            <input type="checkbox" bind:checked={changeCredentials} />
            <span class="text-sm font-medium">Change credentials</span>
          </label>

          {#if !changeCredentials}
            <p class="text-sm text-muted-foreground mt-1">
              Existing credentials will be kept.
            </p>
          {/if}
        </div>
      {/if}

      {#if changeCredentials}
        {#if authType === 'password'}
          <div>
            <label for="password">Password</label>
            <Input id="password" type="password" bind:value={password} placeholder="Enter new password" />
          </div>
        {:else if authType === 'key_inline'}
          <div>
            <label for="privateKey">Private Key</label>
            <textarea id="privateKey" bind:value={privateKey} class="w-full border rounded-md p-2 h-32" placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"></textarea>
          </div>
          <div>
            <label for="keyPassphrase">Passphrase (optional)</label>
            <Input id="keyPassphrase" type="password" bind:value={keyPassphrase} placeholder="Key passphrase" />
          </div>
        {:else if authType === 'key_file'}
          <div>
            <label for="identityFile">Key File Path</label>
            <Input id="identityFile" bind:value={identityFilePath} placeholder="~/.ssh/id_rsa" />
          </div>
        {/if}
      {/if}

      <div class="flex gap-2 pt-4">
        <Button type="submit" disabled={saving || loading}>
          {saving ? 'Saving...' : 'Save Changes'}
        </Button>
        <Button type="button" variant="outline" onclick={() => window.location.href = '/'}>
          Cancel
        </Button>
      </div>
    </form>
  {/if}
</div>