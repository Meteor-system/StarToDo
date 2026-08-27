<script lang="ts">
  import { tick } from 'svelte';

  interface Props {
    open: boolean;
    dialogId: string;
    title: string;
    description: string;
    confirmLabel: string;
    busy: boolean;
    error?: string | null;
    onConfirm: () => void | Promise<void>;
    onCancel: () => void | Promise<void>;
  }

  let {
    open,
    dialogId,
    title,
    description,
    confirmLabel,
    busy,
    error = null,
    onConfirm,
    onCancel
  }: Props = $props();
  let confirmButton = $state<HTMLButtonElement>();
  let cancelButton = $state<HTMLButtonElement>();

  $effect(() => {
    if (!open) return;
    void tick().then(() => cancelButton?.focus());
  });

  function focusableButtons(): HTMLButtonElement[] {
    return [confirmButton, cancelButton].filter(
      (button): button is HTMLButtonElement => Boolean(button && !button.disabled)
    );
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape' && !busy) {
      event.preventDefault();
      void onCancel();
      return;
    }

    if (event.key !== 'Tab' || busy) return;
    const buttons = focusableButtons();
    if (buttons.length === 0) {
      event.preventDefault();
      return;
    }

    const first = buttons[0];
    const last = buttons[buttons.length - 1];
    const active = document.activeElement;
    if (event.shiftKey && (active === first || !buttons.includes(active as HTMLButtonElement))) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && (active === last || !buttons.includes(active as HTMLButtonElement))) {
      event.preventDefault();
      first.focus();
    }
  }
</script>

{#if open}
  <div class="backdrop">
    <div
      class="dialog"
      role="alertdialog"
      aria-modal="true"
      aria-busy={busy}
      aria-labelledby={`${dialogId}-title`}
      aria-describedby={`${dialogId}-description`}
      tabindex="-1"
      onkeydown={handleKeydown}
    >
      <h2 id={`${dialogId}-title`}>{title}</h2>
      <p id={`${dialogId}-description`}>{description}</p>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <div class="actions">
        <button bind:this={confirmButton} type="button" class="danger" onclick={() => void onConfirm()} disabled={busy}>{busy ? '处理中…' : confirmLabel}</button>
        <button bind:this={cancelButton} type="button" class="quiet" onclick={() => void onCancel()} disabled={busy}>取消</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop { position:fixed; inset:0; z-index:20; display:grid; place-items:center; padding:20px; background:rgba(5,6,8,.74); }
  .dialog { width:min(100%,420px); display:grid; gap:11px; padding:18px; border:1px solid rgba(255,170,161,.42); border-radius:var(--radius-md); color:var(--text); background:var(--surface-raised); box-shadow:var(--shadow-overlay); }
  h2,p { margin:0; }
  h2 { font-size:16px; font-weight:650; }
  p { color:var(--text-soft); font-size:13px; line-height:1.55; }
  .actions { display:flex; justify-content:flex-end; gap:8px; margin-top:3px; }
  .danger { border-color:rgba(255,170,161,.58); color:#35100e; background:#ffaaa1; }
  .quiet { border-color:transparent; color:var(--muted); background:transparent; }
  .error { color:var(--danger); }
  button { border:1px solid var(--line-strong); border-radius:var(--radius-sm); padding:7px 10px; color:var(--text-soft); background:transparent; font-size:12px; }
  button:hover:not(:disabled) { border-color:var(--text-soft); background:var(--surface-hover); color:var(--text); }
  button.danger:hover:not(:disabled) { border-color:#ffc1ba; background:#ffc1ba; color:#35100e; }
  @media (max-width:420px) { .backdrop { align-items:end; padding:12px; } .dialog { width:100%; } .actions > button { flex:1; } }
</style>
