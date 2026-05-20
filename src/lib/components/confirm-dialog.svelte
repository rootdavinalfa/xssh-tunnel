<script lang="ts">
  import * as AlertDialog from '$lib/components/ui/alert-dialog';

  let {
    open = $bindable(false),
    title = 'Confirm',
    description = 'Are you sure?',
    confirmLabel = 'Continue',
    cancelLabel = 'Cancel',
    variant = 'destructive' as 'default' | 'destructive',
    onConfirm,
  }: {
    open?: boolean;
    title?: string;
    description?: string;
    confirmLabel?: string;
    cancelLabel?: string;
    variant?: 'default' | 'destructive';
    onConfirm: () => void;
  } = $props();
</script>

<AlertDialog.Root bind:open>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Title>{title}</AlertDialog.Title>
      <AlertDialog.Description>{description}</AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel variant="outline">{cancelLabel}</AlertDialog.Cancel>
      <AlertDialog.Action variant={variant} onclick={onConfirm}>{confirmLabel}</AlertDialog.Action>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
