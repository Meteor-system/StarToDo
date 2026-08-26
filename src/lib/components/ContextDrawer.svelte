<script lang="ts">
  import { tick, type Snippet } from 'svelte';
  import type { DrawerSize } from '$lib/ui-state';

  interface Props {
    open: boolean;
    drawerId: string;
    title: string;
    size?: DrawerSize;
    onClose: () => void;
    children: Snippet;
  }

  let { open, drawerId, title, size = 'normal', onClose, children }: Props = $props();
  let panel = $state<HTMLElement>();
  let returnFocus: HTMLElement | null = null;

  $effect(() => {
    if (!open) return;
    returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    void tick().then(() => panel?.focus());
    return () => {
      const target = returnFocus;
      void tick().then(() => {
        if (target?.isConnected) target.focus();
      });
    };
  });

  function focusableElements(): HTMLElement[] {
    if (!panel) return [];
    return [...panel.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), a[href], [tabindex]:not([tabindex="-1"])')];
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key !== 'Tab') return;
    const controls = focusableElements();
    if (controls.length === 0) {
      event.preventDefault();
      return;
    }
    const first = controls[0];
    const last = controls[controls.length - 1];
    const active = document.activeElement;
    if (event.shiftKey && (active === first || !controls.includes(active as HTMLElement))) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && (active === last || !controls.includes(active as HTMLElement))) {
      event.preventDefault();
      first.focus();
    }
  }
</script>

{#if open}
  <div class="drawer-backdrop" role="presentation" onclick={(event) => { if (event.currentTarget === event.target) onClose(); }} onkeydown={(event) => { if (event.key === 'Escape') onClose(); }}>
    <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
    <aside bind:this={panel} id={drawerId} class:wide={size === 'wide'} class="context-drawer" role="dialog" aria-modal="true" aria-labelledby={`${drawerId}-title`} tabindex="-1" onkeydown={handleKeydown}>
      <header>
        <h2 id={`${drawerId}-title`}>{title}</h2>
        <button type="button" aria-label={`关闭${title}`} onclick={onClose}>×</button>
      </header>
      <div class="drawer-body">{@render children()}</div>
    </aside>
  </div>
{/if}

<style>
  .drawer-backdrop { position:fixed; inset:0; z-index:40; display:flex; justify-content:flex-end; background:rgb(5 6 8 / .68); }
  .context-drawer { width:min(420px,100%); height:100%; min-height:0; display:grid; grid-template-rows:auto minmax(0,1fr); color:var(--text); background:var(--surface-raised); box-shadow:-18px 0 44px rgb(0 0 0 / .35); }
  .context-drawer.wide { width:min(980px,calc(100% - 48px)); }
  .context-drawer header { display:flex; align-items:center; justify-content:space-between; gap:12px; padding:16px; border-bottom:1px solid var(--line); }
  .context-drawer h2 { margin:0; font-size:16px; }
  .context-drawer header button { border:1px solid var(--line); border-radius:var(--radius-sm); padding:4px 9px; color:var(--muted); background:transparent; font-size:18px; }
  .drawer-body { min-height:0; overflow:auto; overscroll-behavior:contain; padding:0 16px 20px; }
</style>
