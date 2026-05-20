<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import * as Table from '$lib/components/ui/table';
  import { Button } from '$lib/components/ui/button';
  import Alert from '$lib/components/ui/alert/alert.svelte';
  import AlertDescription from '$lib/components/ui/alert/alert-description.svelte';
  import type { ParseResult, SshConfigEntry } from '$lib/tauri';

  let {
    open = $bindable(false),
    parseResult = null as ParseResult | null,
    importLoading = false,
    importError = '',
    onImport,
  }: {
    open?: boolean;
    parseResult?: ParseResult | null;
    importLoading?: boolean;
    importError?: string;
    onImport: (selected: string[]) => void;
  } = $props();

  let selectedHosts = $state<Set<string>>(new Set());

  let selectAll = $derived(
    parseResult ? selectedHosts.size === parseResult.entries.length : false
  );
  let selectedCount = $derived(selectedHosts.size);

  function toggleAll() {
    if (!parseResult) return;
    if (selectAll) {
      selectedHosts = new Set();
    } else {
      selectedHosts = new Set(parseResult.entries.map((e: SshConfigEntry) => e.host_aliases[0]));
    }
  }

  function toggleHost(host: string) {
    const next = new Set(selectedHosts);
    if (next.has(host)) {
      next.delete(host);
    } else {
      next.add(host);
    }
    selectedHosts = next;
  }

  function handleImport() {
    onImport(Array.from(selectedHosts));
  }

  function handleOpenChange() {
    if (parseResult && parseResult.entries.length > 0) {
      selectedHosts = new Set(parseResult.entries.map((e: SshConfigEntry) => e.host_aliases[0]));
    }
  }
</script>

<Dialog.Root bind:open onOpenChange={handleOpenChange}>
  <Dialog.Content class="max-w-3xl">
    <Dialog.Header>
      <Dialog.Title>Import from ~/.ssh/config</Dialog.Title>
      <Dialog.Description>
        Select hosts to import as connection profiles.
      </Dialog.Description>
    </Dialog.Header>

    {#if importError}
      <div class="px-6 py-2">
        <Alert variant="destructive">
          <AlertDescription>{importError}</AlertDescription>
        </Alert>
      </div>
    {/if}

    {#if importLoading}
      <div class="px-6 py-8 text-center text-muted-foreground">
        Importing...
      </div>
    {:else if parseResult}
      <div class="px-6 py-2">
        <p class="text-sm text-muted-foreground mb-4">
          Found {parseResult.entries.length} hosts
          {#if parseResult.skipped.length > 0}
            (skipped: {parseResult.skipped.join(', ')})
          {/if}
        </p>

        {#if parseResult.entries.length > 0}
          <Table.Root>
            <Table.Header>
              <Table.Row>
                <Table.Head class="w-10">
                  <input
                    type="checkbox"
                    checked={selectAll}
                    onchange={toggleAll}
                    class="accent-primary"
                  />
                </Table.Head>
                <Table.Head>Host</Table.Head>
                <Table.Head>Hostname</Table.Head>
                <Table.Head>User</Table.Head>
                <Table.Head>Port</Table.Head>
                <Table.Head>Key File</Table.Head>
              </Table.Row>
            </Table.Header>
            <Table.Body>
              {#each parseResult.entries as entry (entry.host_aliases[0])}
                <Table.Row>
                  <Table.Cell>
                    <input
                      type="checkbox"
                      checked={selectedHosts.has(entry.host_aliases[0])}
                      onchange={() => toggleHost(entry.host_aliases[0])}
                      class="accent-primary"
                    />
                  </Table.Cell>
                  <Table.Cell class="font-medium">{entry.host_aliases[0]}</Table.Cell>
                  <Table.Cell>{entry.hostname}</Table.Cell>
                  <Table.Cell>{entry.user || '-'}</Table.Cell>
                  <Table.Cell>{entry.port || 22}</Table.Cell>
                  <Table.Cell>{entry.identity_file ? 'Yes' : 'No'}</Table.Cell>
                </Table.Row>
              {/each}
            </Table.Body>
          </Table.Root>
        {:else}
          <p class="text-sm text-muted-foreground py-4">
            No importable hosts found.
          </p>
        {/if}
      </div>
    {:else}
      <div class="px-6 py-8 text-center text-muted-foreground">
        Click "Parse Config" to scan your ~/.ssh/config file.
      </div>
    {/if}

    <Dialog.Footer>
      <Dialog.Close>
        <Button variant="outline">Cancel</Button>
      </Dialog.Close>
      {#if parseResult && parseResult.entries.length > 0}
        <Button onclick={handleImport} disabled={selectedCount === 0 || importLoading}>
          Import {selectedCount} {selectedCount === 1 ? 'profile' : 'profiles'}
        </Button>
      {/if}
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
