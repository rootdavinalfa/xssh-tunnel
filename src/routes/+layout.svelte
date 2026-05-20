<script>
  import '../app.css';
  import Sonner from '$lib/components/ui/sonner/sonner.svelte';
  let { children } = $props();
</script>

<div class="min-h-screen bg-background text-foreground">
  {@render children()}
  <Sonner />
</div>