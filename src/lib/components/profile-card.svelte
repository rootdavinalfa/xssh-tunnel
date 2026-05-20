<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import type { Profile } from '$lib/stores/profiles';

  let {
    profile,
    connectionState,
    loading,
    onConnect,
    onDisconnect,
    onEdit,
    onDelete,
  }: {
    profile: Profile;
    connectionState: string;
    loading: boolean;
    onConnect: (id: string) => void;
    onDisconnect: () => void;
    onEdit: (id: string) => void;
    onDelete: (id: string) => void;
  } = $props();
</script>

<div class="border rounded-lg p-4">
  <div class="flex justify-between items-start">
    <div>
      <h3 class="font-semibold">{profile.label}</h3>
      <p class="text-sm text-muted-foreground">
        {profile.username}@{profile.host}:{profile.port}
      </p>
    </div>
    <div class="flex gap-2">
      {#if connectionState === 'disconnected'}
        <Button
          onclick={() => onConnect(profile.id)}
          disabled={loading}
          size="sm"
        >
          Connect
        </Button>
      {:else}
        <Button
          onclick={onDisconnect}
          disabled={loading}
          variant="destructive"
          size="sm"
        >
          Disconnect
        </Button>
      {/if}
      <Button
        onclick={() => onEdit(profile.id)}
        variant="outline"
        size="sm"
      >
        Edit
      </Button>
      <Button
        onclick={() => onDelete(profile.id)}
        variant="outline"
        size="sm"
      >
        Delete
      </Button>
    </div>
  </div>
</div>
