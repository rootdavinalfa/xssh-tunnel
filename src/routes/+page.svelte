<script>
  import { Button } from '$lib/components/ui/button';
  import { greet } from '$lib/tauri';

  let name = $state('');
  let greeting = $state('');
  let loading = $state(false);
  let error = $state('');

  async function handleGreet() {
    loading = true;
    error = '';
    try {
      greeting = await greet(name);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }
</script>

<div class="flex flex-col items-center justify-center min-h-screen gap-4 p-8">
  <h1 class="text-3xl font-bold">XSSH Tunnel</h1>
  <p class="text-muted-foreground">Milestone 0 — Skeleton</p>

  <div class="flex gap-2 mt-4 w-full max-w-sm">
    <input
      type="text"
      bind:value={name}
      placeholder="Enter your name..."
      class="px-4 py-2 border rounded-md"
    />
    <Button
      onclick={handleGreet}
      disabled={loading || !name}
    >
      {loading ? 'Loading...' : 'Greet'}
    </Button>
  </div>

  {#if error}
    <p class="mt-4 text-lg text-red-500">{error}</p>
  {/if}
  {#if greeting}
    <p class="mt-4 text-lg">{greeting}</p>
  {/if}
</div>