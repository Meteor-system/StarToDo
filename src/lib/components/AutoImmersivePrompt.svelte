<script lang="ts">
  import { tick } from 'svelte';

  interface Props {
    open: boolean;
    onChoose: (preference: 'enabled' | 'disabled') => void;
    onCancel: () => void;
  }

  let { open, onChoose, onCancel }: Props = $props();
  let dialog = $state<HTMLElement>();
  let enabledButton = $state<HTMLButtonElement>();
  let conservativeButton = $state<HTMLButtonElement>();
  let returnFocus: HTMLElement | null = null;

  $effect(() => {
    if (!open) return;
    returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    void tick().then(() => conservativeButton?.focus());

    return () => {
      const target = returnFocus;
      void tick().then(() => {
        if (target?.isConnected) target.focus();
      });
    };
  });

  function buttons(): HTMLButtonElement[] {
    return [enabledButton, conservativeButton].filter(
      (button): button is HTMLButtonElement => Boolean(button && !button.disabled)
    );
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      onCancel();
      return;
    }

    if (event.key !== 'Tab') return;
    const controls = buttons();
    if (controls.length === 0) {
      event.preventDefault();
      return;
    }

    const first = controls[0];
    const last = controls[controls.length - 1];
    const active = document.activeElement;
    if (event.shiftKey && (active === first || !controls.includes(active as HTMLButtonElement))) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && (active === last || !controls.includes(active as HTMLButtonElement))) {
      event.preventDefault();
      first.focus();
    }
  }
</script>

{#if open}
  <div class="prompt-backdrop">
    <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
    <div
      bind:this={dialog}
      class="prompt"
      role="dialog"
      aria-modal="true"
      aria-labelledby="auto-immersive-title"
      aria-describedby="auto-immersive-description"
      tabindex="-1"
      onkeydown={handleKeydown}
    >
      <h2 id="auto-immersive-title">开始专注时进入沉浸模式？</h2>
      <p id="auto-immersive-description">
        启用后，每次开始新的专注阶段都会进入全屏；
        可在设置中随时修改。
      </p>
      <div class="actions">
        <button
          bind:this={enabledButton}
          type="button"
          class="primary"
          onclick={() => onChoose('enabled')}
        >
          进入并记住
        </button>
        <button
          bind:this={conservativeButton}
          type="button"
          onclick={() => onChoose('disabled')}
        >
          保持窗口模式
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .prompt-backdrop {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: grid;
    place-items: center;
    padding: 18px;
    background: rgb(5 6 8 / 0.74);
  }

  .prompt {
    width: min(100%, 440px);
    display: grid;
    gap: 12px;
    padding: 20px;
    border: 1px solid color-mix(in srgb, var(--accent) 44%, var(--line));
    border-radius: var(--radius-md);
    color: var(--text);
    background: var(--surface-raised);
    box-shadow: var(--shadow-overlay);
  }

  h2,
  p { margin: 0; }
  h2 { font-size: 18px; }
  p { color: var(--text-soft); font-size: 13px; line-height: 1.65; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 4px; }
  button { border: 1px solid var(--line-strong); border-radius: var(--radius-sm); padding: 8px 11px; color: var(--text-soft); background: transparent; }
  button.primary { border-color: var(--accent); color: #25110b; background: var(--accent); }

  @media (max-width: 420px) {
    .prompt-backdrop { align-items: end; padding: 12px; }
    .actions { flex-direction: column; }
    button { width: 100%; }
  }
</style>
