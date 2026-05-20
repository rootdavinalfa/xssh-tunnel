<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { createProfile, getProfileById, updateProfile } from '$lib/tauri';

  let {
    open = $bindable(false),
    mode = 'new' as 'new' | 'edit',
    profileId = '',
    onSave,
  }: {
    open?: boolean;
    mode?: 'new' | 'edit';
    profileId?: string;
    onSave?: () => void;
  } = $props();

  // Form state
  let title = $derived(mode === 'new' ? 'New Connection' : 'Edit Connection');
  let label = $state('');
  let host = $state('');
  let port = $state(22);
  let username = $state('');
  let authType = $state('password');
  let identityFilePath = $state('');

  // Credentials state (edit mode)
  let changeCredentials = $state(false);
  let password = $state('');
  let privateKey = $state('');
  let keyPassphrase = $state('');
  let hasExistingCredentials = $state(false);

  // UI state
  let error = $state('');
  let saving = $state(false);
  let loading = $state(false);

  // Load existing profile data in edit mode
  async function loadProfile() {
    if (mode !== 'edit' || !profileId) return;
    loading = true;
    error = '';
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
  }

  // Load profile data when dialog opens in edit mode
  $effect(() => {
    if (open && mode === 'edit' && profileId) {
      loadProfile();
    } else if (open && mode === 'new') {
      loading = false;
      changeCredentials = false;
      error = '';
    }
  });

  // Reset form when dialog opens
  function resetForm() {
    if (mode === 'new') {
      label = '';
      host = '';
      port = 22;
      username = '';
      authType = 'password';
      identityFilePath = '';
      password = '';
      privateKey = '';
      keyPassphrase = '';
    }
    changeCredentials = false;
    error = '';
    saving = false;
  }

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    saving = true;
    error = '';

    try {
      if (mode === 'new') {
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
      } else {
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
        await updateProfile(req);
      }
      open = false;
      onSave?.();
    } catch (e) {
      error = String(e);
      saving = false;
    }
  }
</script>

<Dialog.Root bind:open onOpenChange={() => { if (!open) resetForm(); }}>
  <Dialog.Content class="sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>{title}</Dialog.Title>
      <Dialog.Description>
        {mode === 'new' ? 'Add a new SSH connection profile.' : 'Update the connection profile details.'}
      </Dialog.Description>
    </Dialog.Header>

    {#if loading}
      <div class="px-6 py-8 text-center text-muted-foreground">
        Loading profile...
      </div>
    {:else}
      {#if error}
        <div class="px-6">
          <p class="text-red-500 text-sm mb-4">{error}</p>
        </div>
      {/if}

      <form onsubmit={handleSubmit} id="connection-form" class="space-y-4 px-6">
        <div>
          <label for="cf-label" class="text-sm font-medium">Label</label>
          <Input id="cf-label" bind:value={label} placeholder="My Server" required />
        </div>

        <div>
          <label for="cf-host" class="text-sm font-medium">Host</label>
          <Input id="cf-host" bind:value={host} placeholder="192.168.1.1" required />
        </div>

        <div>
          <label for="cf-port" class="text-sm font-medium">Port</label>
          <Input id="cf-port" type="number" bind:value={port} />
        </div>

        <div>
          <label for="cf-username" class="text-sm font-medium">Username</label>
          <Input id="cf-username" bind:value={username} placeholder="root" required />
        </div>

        <div>
          <label for="cf-auth-type" class="text-sm font-medium">Authentication</label>
          <select
            id="cf-auth-type"
            bind:value={authType}
            class="w-full border rounded-md p-2 text-sm"
          >
            <option value="password">Password</option>
            <option value="key_inline">Private Key (Inline)</option>
            <option value="key_file">Key File</option>
            <option value="agent">SSH Agent</option>
          </select>
        </div>

        {#if mode === 'edit' && hasExistingCredentials && !changeCredentials}
          <div class="border rounded-lg p-4">
            <label class="flex items-center gap-2 cursor-pointer">
              <input type="checkbox" bind:checked={changeCredentials} class="accent-primary" />
              <span class="text-sm font-medium">Change credentials</span>
            </label>
            <p class="text-sm text-muted-foreground mt-1">
              Existing credentials will be kept.
            </p>
          </div>
        {/if}

        {#if mode === 'new' || changeCredentials}
          {#if authType === 'password'}
            <div>
              <label for="cf-password" class="text-sm font-medium">Password</label>
              <Input id="cf-password" type="password" bind:value={password} placeholder="Enter password" />
            </div>
          {:else if authType === 'key_inline'}
            <div>
              <label for="cf-private-key" class="text-sm font-medium">Private Key</label>
              <textarea
                id="cf-private-key"
                bind:value={privateKey}
                class="w-full border rounded-md p-2 h-32 text-sm"
                placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
              ></textarea>
            </div>
            <div>
              <label for="cf-passphrase" class="text-sm font-medium">Passphrase (optional)</label>
              <Input id="cf-passphrase" type="password" bind:value={keyPassphrase} placeholder="Key passphrase" />
            </div>
          {:else if authType === 'key_file'}
            <div>
              <label for="cf-id-file" class="text-sm font-medium">Key File Path</label>
              <Input id="cf-id-file" bind:value={identityFilePath} placeholder="~/.ssh/id_rsa" />
            </div>
          {/if}
        {/if}
      </form>
    {/if}

    <Dialog.Footer>
      <Dialog.Close>
        <Button variant="outline" type="button">Cancel</Button>
      </Dialog.Close>
      {#if !loading}
        <Button type="submit" form="connection-form" disabled={saving}>
          {saving ? 'Saving...' : mode === 'new' ? 'Save' : 'Save Changes'}
        </Button>
      {/if}
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
