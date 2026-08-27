<script lang="ts">
  import {
    formatPomodoroDuration,
    pomodoroPhaseLabel,
    pomodoroPrimaryAction,
    remainingPomodoroSeconds,
    type PomodoroSnapshot
  } from '$lib/pomodoro';

  interface Props {
    tauriAvailable: boolean;
    snapshot: PomodoroSnapshot | null;
    busy: boolean;
    warning?: string | null;
    onOpenFocus: () => void;
    onPause: () => void | Promise<void>;
    onResume: () => void | Promise<void>;
  }

  let { tauriAvailable, snapshot, busy, warning = null, onOpenFocus, onPause, onResume }: Props = $props();
  let nowUnixMs = $state(Date.now());

  $effect(() => {
    const timer = window.setInterval(() => nowUnixMs = Date.now(), 1_000);
    return () => window.clearInterval(timer);
  });

  let session = $derived(snapshot?.currentSession ?? null);
  let active = $derived(session?.status === 'running' || session?.status === 'paused');
  let action = $derived(pomodoroPrimaryAction(snapshot));
  let remaining = $derived(remainingPomodoroSeconds(session, nowUnixMs));

  function runPrimaryAction(): void {
    if (!tauriAvailable || busy) return;
    if (action === 'pause') void onPause();
    if (action === 'resume') void onResume();
  }
</script>

<section class="focus-mini-bar" aria-label="专注计时" aria-busy={busy}>
  {#if active && session}
    <button type="button" class="status" onclick={onOpenFocus} aria-label="进入专注工作区">
      <span class="indicator" aria-hidden="true"></span>
      <span class="phase">{pomodoroPhaseLabel(session.phase)}</span>
      <strong>{formatPomodoroDuration(remaining)}</strong>
      {#if session.taskTitleSnapshot}<span class="task">{session.taskTitleSnapshot}</span>{/if}
    </button>
    <button type="button" class="control" onclick={runPrimaryAction} disabled={!tauriAvailable || busy}>{busy ? '…' : action === 'pause' ? '暂停' : '继续'}</button>
  {:else}
    <button type="button" class="idle" onclick={onOpenFocus}><span aria-hidden="true">+</span>尚无活动专注，进入专注工作区</button>
  {/if}
  {#if warning}<span class="warning" role="status">专注状态需要注意</span>{/if}
</section>

<style>
  .focus-mini-bar { display:flex; align-items:center; min-width:0; min-height:34px; border:1px solid var(--line, rgba(255,255,255,.11)); border-radius:4px; background:color-mix(in srgb, var(--surface, #171a1f) 82%, transparent); }
  .status,.idle { display:flex; align-items:center; min-width:0; gap:7px; flex:1; border:0; padding:7px 9px; color:var(--muted, #9aa6b2); background:transparent; text-align:left; font-size:12px; }
  .indicator { width:7px; height:7px; flex:none; border-radius:50%; background:var(--accent, #ec6945); }
  .phase { flex:none; }
  .status strong { color:var(--text, #e7edf2); font-family:ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size:12px; font-variant-numeric:tabular-nums; }
  .task { min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; color:var(--muted, #9aa6b2); }
  .control { align-self:stretch; border:0; border-left:1px solid var(--line, rgba(255,255,255,.11)); padding:0 10px; color:var(--accent, #ffb09a); background:transparent; font-size:11px; }
  .warning { flex:none; padding-right:9px; color:var(--warning-text, #e8ca91); font-size:10px; }
  .idle span { color:var(--accent, #ec6945); font-size:16px; line-height:1; }
  button:focus-visible { outline:2px solid var(--info, #5ba9ff); outline-offset:2px; }
  button:disabled { cursor:not-allowed; opacity:.48; }

  @media (max-height: 559px) {
    .task { display: none; }
  }

  @media (prefers-reduced-motion: reduce) {
    * { transition:none !important; }
  }
</style>
