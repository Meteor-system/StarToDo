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
  .backdrop { position:fixed; inset:0; z-index:20; display:grid; place-items:center; padding:20px; background:rgba(8,11,16,.68); }
  .dialog { width:min(100%,420px); display:grid; gap:10px; padding:18px; border:1px solid rgba(255,180,180,.38); border-radius:7px; color:#edf1f6; background:#20242b; box-shadow:0 18px 56px rgba(0,0,0,.42); }
  h2,p { margin:0; } h2 { font-size:16px; } p { color:#d6dee8; font-size:13px; line-height:1.55; }.actions { display:flex; justify-content:flex-end; gap:8px; margin-top:2px; }.danger { color:#ffb4b4; }.quiet { border:0; color:var(--muted); background:transparent; }.error { color:#ffaeae; } button { border:1px solid var(--line); border-radius:4px; padding:6px 9px; color:inherit; background:rgba(255,255,255,.05); font-size:12px; } button:focus-visible { outline:2px solid var(--blue); outline-offset:2px; }
</style>
