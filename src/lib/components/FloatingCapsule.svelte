<script lang="ts">
  interface Props {
    phaseLabel: string;
    remainingLabel: string;
    taskTitle: string | null;
    paused: boolean;
    busy: boolean;
    onOpenFocus: () => void;
    onPrimary: () => void;
    onExpand: () => void;
  }

  let {
    phaseLabel,
    remainingLabel,
    taskTitle,
    paused,
    busy,
    onOpenFocus,
    onPrimary,
    onExpand
  }: Props = $props();
</script>

<div class="capsule" data-tauri-drag-region="deep">
  <span class="star" aria-hidden="true">★</span>
  <button type="button" class="focus" onclick={onOpenFocus} aria-label="打开专注工作区" data-floating-focus-target="open-focus">
    <span class="phase">{phaseLabel}</span>
    <strong>{remainingLabel}</strong>
    <span class="task">{taskTitle ?? '未绑定任务'}</span>
  </button>
  <button type="button" class="primary" onclick={onPrimary} disabled={busy} data-floating-focus-target="primary">
    {busy ? '…' : paused ? '继续' : '暂停'}
  </button>
  <button type="button" class="expand" onclick={onExpand} aria-label="展开悬浮窗" data-floating-focus-target="expand">⌃</button>
</div>

<style>
  .capsule {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    padding: 7px 9px;
    box-sizing: border-box;
    color: var(--text, #f1f3f5);
  }

  .star {
    color: var(--accent, #f36b32);
    font-size: 13px;
  }

  button {
    min-width: 0;
    border: 1px solid var(--line, rgba(232, 235, 240, 0.13));
    border-radius: var(--radius-sm, 5px);
    color: inherit;
    background: var(--surface-raised, #202329);
    cursor: pointer;
  }

  .focus {
    display: grid;
    grid-template-columns: auto auto minmax(0, 1fr);
    align-items: baseline;
    gap: 7px;
    padding: 6px 8px;
    text-align: left;
  }

  .phase {
    color: var(--muted, #949ba6);
    font-size: 10px;
    letter-spacing: 0.08em;
  }

  strong {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 16px;
    font-variant-numeric: tabular-nums;
    line-height: 1;
  }

  .task {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted, #949ba6);
    font-size: 10px;
  }

  .primary,
  .expand {
    min-height: 34px;
    padding: 5px 9px;
    font-size: 11px;
  }

  .primary {
    border-color: var(--accent, #f36b32);
    color: var(--accent-ink, #251007);
    background: var(--accent, #f36b32);
    font-weight: 650;
  }

  .expand {
    width: 34px;
    padding-inline: 0;
    color: var(--text-soft, #c5c9d0);
  }

  button:hover:not(:disabled) {
    border-color: var(--text-soft, #c5c9d0);
    background: var(--surface-hover, #272b31);
  }

  .primary:hover:not(:disabled) {
    border-color: var(--accent-hover, #ff7a42);
    color: var(--accent-ink, #251007);
    background: var(--accent-hover, #ff7a42);
  }

  button:focus-visible {
    outline: 2px solid var(--info, #5ba9ff);
    outline-offset: 1px;
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.56;
  }
</style>
