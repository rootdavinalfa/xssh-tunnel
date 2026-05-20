<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';

  import { createProfile } from '$lib/tauri';

  let label = $state('');
  let host = $state('');
  let port = $state(22);
  let username = $state('');
  let authType = $state('password');
  let password = $state('');
  let privateKey = $state('');
  let identityFilePath = $state('');
  let error = $state('');
  let saving = $state(false);

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    saving = true;
    error = '';

    try {
      await createProfile({
        label,
        host,
        port,
        username,
        auth_type: authType,
        password: authType === 'password' ? password : undefined,
        private_key: authType === 'key_inline' ? privateKey : undefined,
        identity_file_path: authType === 'key_file' ? identityFilePath : undefined,
      });
      window.location.href = '/';
    } catch (e: unknown) {
      error = String(e);
      saving = false;
    }
  }
</script>

<div class="container mx-auto p-6 max-w-lg">
  <h1 class="text-2xl font-bold mb-6">New Connection</h1>

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

    {#if authType === 'password'}
      <div>
        <label for="password">Password</label>
        <Input id="password" type="password" bind:value={password} />
      </div>
    {:else if authType === 'key_inline'}
      <div>
        <label for="privateKey">Private Key</label>
        <textarea id="privateKey" bind:value={privateKey} class="w-full border rounded-md p-2 h-32" placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"></textarea>
      </div>
    {:else if authType === 'key_file'}
      <div>
        <label for="identityFile">Key File Path</label>
        <Input id="identityFile" bind:value={identityFilePath} placeholder="~/.ssh/id_rsa" />
      </div>
    {/if}

    <div class="flex gap-2 pt-4">
      <Button type="submit" disabled={saving}>
        {saving ? 'Saving...' : 'Save'}
      </Button>
      <Button type="button" variant="outline" onclick={() => window.location.href = '/'}>
        Cancel
      </Button>
    </div>
  </form>
</div>