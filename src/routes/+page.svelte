<script>
  let name = $state('');
  let greeting = $state('');

  async function greet() {
    const { invoke } = await import('@tauri-apps/api/core');
    greeting = await invoke('greet', { name });
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
    <button
      onclick={greet}
      class="px-4 py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90"
    >
      Greet
    </button>
  </div>

  {#if greeting}
    <p class="mt-4 text-lg">{greeting}</p>
  {/if}
</div>