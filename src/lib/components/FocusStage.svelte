<script lang="ts">
  import {
    formatPomodoroDuration,
    pomodoroPhaseDurationSeconds,
    pomodoroPhaseLabel,
    pomodoroPrimaryAction,
    pomodoroProgress,
    remainingPomodoroSeconds,
    type PomodoroPhase,
    type PomodoroSnapshot
  } from '$lib/pomodoro';

  interface Props {
    snapshot: PomodoroSnapshot | null;
    selectedPhase: PomodoroPhase;
    selectedTaskTitle: string | null;
    nowUnixMs: number;
    busy: boolean;
    tauriAvailable: boolean;
    onChoosePhase: (phase: PomodoroPhase) => void;
    onPrimary: () => void;
    onSkip: () => void;
    onReset: () => void;
  }

  let {
    snapshot,
    selectedPhase,
    selectedTaskTitle,
    nowUnixMs,
    busy,
    tauriAvailable,
    onChoosePhase,
    onPrimary,
    onSkip,
    onReset
  }: Props = $props();

  let session = $derived(snapshot?.currentSession ?? null);
  let active = $derived(session?.status === 'running' || session?.status === 'paused');
  let running = $derived(session?.status === 'running');
  let phaseForDisplay = $derived(active && session ? session.phase : selectedPhase);
  let displayedSeconds = $derived(
    active
      ? remainingPomodoroSeconds(session, nowUnixMs)
      : snapshot
        ? pomodoroPhaseDurationSeconds(snapshot.settings, selectedPhase)
        : 0
  );
  let progress = $derived(active ? pomodoroProgress(session, nowUnixMs) : 0);
  let primaryAction = $derived(pomodoroPrimaryAction(snapshot));
  let primaryLabel = $derived(
    busy
      ? '处理中…'
      : primaryAction === 'pause'
        ? '暂停'
        : primaryAction === 'resume'
          ? '继续'
          : `开始${pomodoroPhaseLabel(selectedPhase)}`
  );
</script>

<div class="focus-stage" data-focus-stage>
  <div class="phase-switch" aria-label="选择阶段">
    {#each ['focus', 'shortBreak', 'longBreak'] as phase}
      <button
        type="button"
        class:active={phaseForDisplay === phase}
        aria-pressed={phaseForDisplay === phase}
        disabled={active || busy || !tauriAvailable}
        onclick={() => onChoosePhase(phase as PomodoroPhase)}
      >
        {pomodoroPhaseLabel(phase as PomodoroPhase)}
      </button>
    {/each}
  </div>

  <div
    class:running
    class="focus-ring"
    style={`--progress: ${progress}`}
    aria-hidden="true"
  >
    <div class="focus-ring-core"></div>
  </div>

  <p
    class="timer"
    role="timer"
    aria-label={`${active ? '剩余时间' : '阶段时长'} ${formatPomodoroDuration(displayedSeconds)}`}
  >
    {formatPomodoroDuration(displayedSeconds)}
  </p>
  <p class="timer-context">
    {active
      ? session?.taskTitleSnapshot ?? '独立专注'
      : selectedTaskTitle ?? `准备开始${pomodoroPhaseLabel(selectedPhase)}`}
  </p>

  <div class="controls" aria-label="番茄控制">
    <button
      type="button"
      class="primary"
      onclick={onPrimary}
      disabled={!tauriAvailable || busy || primaryAction === 'none'}
    >
      {primaryLabel}
    </button>
    {#if active}
      <button type="button" onclick={onSkip} disabled={!tauriAvailable || busy}>跳过</button>
      <button type="button" class="quiet danger" onclick={onReset} disabled={!tauriAvailable || busy}>重置</button>
    {/if}
  </div>
</div>

<style>
  .focus-stage {
    position: relative;
    width: min(100%, 640px);
    min-height: 0;
    display: grid;
    place-items: center;
    align-content: center;
    gap: clamp(10px, 2.4vh, 20px);
    padding: clamp(8px, 2vh, 20px);
  }

  .phase-switch,
  .controls {
    position: relative;
    z-index: 1;
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 6px;
  }

  button {
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    padding: 7px 11px;
    color: var(--muted);
    background: transparent;
    font-size: 12px;
  }

  button.active {
    border-color: color-mix(in srgb, var(--accent) 68%, var(--line));
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }

  button.primary {
    border-color: var(--accent);
    color: #25110b;
    background: var(--accent);
    font-weight: 650;
  }

  button.danger {
    color: var(--danger);
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.48;
  }

  .focus-ring {
    --progress-turn: calc(var(--progress) * 1turn);
    position: absolute;
    width: clamp(190px, min(45vw, 42vh), 330px);
    aspect-ratio: 1;
    border-radius: 50%;
    background:
      conic-gradient(
        var(--accent) var(--progress-turn),
        var(--line) 0
      );
    padding: 3px;
    opacity: 0.92;
  }

  .focus-ring-core {
    width: 100%;
    height: 100%;
    border-radius: inherit;
    background:
      radial-gradient(circle, color-mix(in srgb, var(--accent) 9%, transparent), transparent 62%),
      var(--surface, #171a1f);
    box-shadow: inset 0 0 36px rgb(0 0 0 / 0.18);
  }

  .focus-ring.running .focus-ring-core {
    animation: focus-breathe 3.2s ease-in-out infinite;
  }

  .timer {
    position: relative;
    z-index: 1;
    margin: clamp(34px, 7vh, 68px) 0 0;
    color: var(--text);
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: clamp(42px, min(10vw, 11vh), 94px);
    font-variant-numeric: tabular-nums;
    font-weight: 560;
    letter-spacing: -0.055em;
    line-height: 1;
  }

  .timer-context {
    position: relative;
    z-index: 1;
    max-width: min(80vw, 360px);
    margin: 0;
    overflow: hidden;
    color: var(--muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  @keyframes focus-breathe {
    0%, 100% { transform: scale(0.985); opacity: 0.86; }
    50% { transform: scale(1); opacity: 1; }
  }

  @media (max-height: 559px) {
    .focus-stage { height: 100%; gap: 3px; padding: 0; }
    .focus-ring { width: clamp(104px, min(30vw, 28vh), 148px); }
    .timer { margin-top: 15px; font-size: clamp(28px, min(7vw, 8vh), 44px); }
    .timer-context { font-size: 11px; }
    button { padding: 3px 7px; }
  }

  @media (max-width: 420px) {
    .phase-switch { gap: 3px; }
    .phase-switch button { padding-inline: 7px; }
  }

  @media (prefers-reduced-motion: reduce) {
    .focus-ring.running .focus-ring-core {
      animation: none;
    }
  }
</style>
