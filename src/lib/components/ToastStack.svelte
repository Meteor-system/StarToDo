<script lang="ts">
  import type { ToastMessage } from '$lib/ui-state';
  interface Props { messages: ToastMessage[]; }
  let { messages }: Props = $props();
</script>

<div class="toast-stack" aria-label="通知">
  {#each messages as toast (toast.id)}
    <div class="toast" class:success={toast.tone === 'success'} class:warning={toast.tone === 'warning'} class:danger={toast.tone === 'danger'} role={toast.tone === 'warning' || toast.tone === 'danger' ? 'alert' : 'status'}>
      {#if toast.tone === 'success'}<span aria-hidden="true">✦</span>{/if}
      <span>{toast.message}</span>
      {#if toast.actionLabel}<button type="button" onclick={() => toast.onAction?.()}>{toast.actionLabel}</button>{/if}
      <button type="button" aria-label="关闭通知" onclick={() => toast.onDismiss?.()}>×</button>
    </div>
  {/each}
</div>

<style>
  .toast-stack { position:fixed; top:14px; right:14px; z-index:60; display:grid; gap:8px; width:min(380px,calc(100vw - 28px)); pointer-events:none; }.toast { display:flex; align-items:center; gap:9px; padding:10px 11px; border:1px solid var(--line); border-radius:var(--radius-md); color:var(--text); background:var(--surface-raised); box-shadow:var(--shadow-overlay); pointer-events:auto; font-size:12px; }.toast button { border:0; border-radius:var(--radius-sm); padding:4px 7px; color:var(--text-soft); background:var(--surface-hover); }.toast > button:last-child { margin-left:auto; }.toast.success { border-color:rgba(155,217,173,.42); }.toast.warning { border-color:rgba(224,173,101,.5); }.toast.danger { border-color:rgba(255,170,161,.5); }
</style>