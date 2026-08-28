<script lang="ts">
  import {
    formatPomodoroDuration,
    pomodoroPhaseDurationSeconds,
    pomodoroPhaseLabel,
    pomodoroPrimaryAction,
    pomodoroProgress,
    pomodoroStatusLabel,
    remainingPomodoroSeconds,
    type PomodoroPhase,
    type PomodoroSettings,
    type PomodoroSnapshot,
    type PomodoroTaskSummary,
    type StartPomodoroInput,
    type UpdatePomodoroSettingsInput
  } from '$lib/pomodoro';

  interface Props {
    tauriAvailable: boolean;
    snapshot: PomodoroSnapshot | null;
    /** `tasks` remains for the current page integration; use taskSummaries for new callers. */
    tasks?: PomodoroTaskSummary[];
    taskSummaries?: PomodoroTaskSummary[];
    selectedTaskId?: number | null;
    busy: boolean;
    warning: string | null;
    onStart: (input: StartPomodoroInput) => void | Promise<void>;
    onPause: () => void | Promise<void>;
    onResume: () => void | Promise<void>;
    onSkip: () => void | Promise<void>;
    onReset: () => void | Promise<void>;
    onUpdateSettings: (input: UpdatePomodoroSettingsInput) => void | Promise<void>;
  }

  let {
    tauriAvailable,
    snapshot,
    tasks = [],
    taskSummaries,
    selectedTaskId = $bindable<number | null>(null),
    busy,
    warning,
    onStart,
    onPause,
    onResume,
    onSkip,
    onReset,
    onUpdateSettings
  }: Props = $props();

  let selectedPhase = $state<PomodoroPhase>('focus');
  let nowUnixMs = $state(Date.now());
  let settingsOpen = $state(false);
  let settingsDraft = $state<PomodoroSettings>({
    focusMinutes: 25,
    shortBreakMinutes: 5,
    longBreakMinutes: 15,
    longBreakInterval: 4,
    updatedAtUnixMs: 0
  });
  let announcedMinute = -1;

  $effect(() => {
    if (snapshot?.currentSession) selectedPhase = snapshot.currentSession.phase;
    else if (snapshot) selectedPhase = snapshot.recommendedPhase;
  });

  $effect(() => {
    if (!settingsOpen && snapshot) settingsDraft = { ...snapshot.settings };
  });

  $effect(() => {
    const timer = window.setInterval(() => nowUnixMs = Date.now(), 1_000);
    return () => window.clearInterval(timer);
  });

  let session = $derived(snapshot?.currentSession ?? null);
  let active = $derived(session?.status === 'running' || session?.status === 'paused');
  let remainingSeconds = $derived(remainingPomodoroSeconds(session, nowUnixMs));
  let primaryAction = $derived(pomodoroPrimaryAction(snapshot));
  let availableTasks = $derived(taskSummaries ?? tasks);
  let selectedTask = $derived(availableTasks.find((task) => task.taskId === selectedTaskId) ?? null);
  let taskSelectionPending = $derived(selectedTaskId !== null && selectedTask === null);
  let phaseForStart = $derived(active && session ? session.phase : selectedPhase);
  let phaseAllowsTask = $derived(phaseForStart === 'focus');
  let displayedSeconds = $derived(active
    ? remainingSeconds
    : snapshot
      ? pomodoroPhaseDurationSeconds(snapshot.settings, selectedPhase)
      : 0);
  let progress = $derived(active ? pomodoroProgress(session, nowUnixMs) : 0);
  let announcement = $derived.by(() => {
    if (!session) return `待开始：${pomodoroPhaseLabel(selectedPhase)}。`;
    const minute = Math.floor(remainingSeconds / 60);
    const shouldAnnounceMinute = session.status === 'running' && minute !== announcedMinute;
    if (shouldAnnounceMinute) announcedMinute = minute;
    return shouldAnnounceMinute
      ? `${pomodoroPhaseLabel(session.phase)}，剩余约 ${minute} 分钟。`
      : `${pomodoroPhaseLabel(session.phase)}${pomodoroStatusLabel(session.status)}。`;
  });

  function choosePhase(phase: PomodoroPhase): void {
    if (active || busy) return;
    selectedPhase = phase;
    if (phase !== 'focus') selectedTaskId = null;
  }

  function chooseTask(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value;
    const taskId = Number(value);
    selectedTaskId = value !== '' && Number.isSafeInteger(taskId) && taskId > 0 ? taskId : null;
  }

  function runPrimaryAction(): void {
    if (!tauriAvailable || busy) return;
    if (primaryAction === 'pause') void onPause();
    else if (primaryAction === 'resume') void onResume();
    else if (primaryAction === 'start') {
      if (selectedPhase === 'focus' && taskSelectionPending) return;
      void onStart({ phase: selectedPhase, taskId: selectedPhase === 'focus' ? selectedTask?.taskId ?? null : null });
    }
  }

  function submitSettings(): void {
    if (!tauriAvailable || busy) return;
    void onUpdateSettings({
      focusMinutes: Number(settingsDraft.focusMinutes),
      shortBreakMinutes: Number(settingsDraft.shortBreakMinutes),
      longBreakMinutes: Number(settingsDraft.longBreakMinutes),
      longBreakInterval: Number(settingsDraft.longBreakInterval)
    });
  }
</script>

<section class="focus-workspace" aria-labelledby="focus-heading" aria-busy={busy}>
  <header class="workspace-header">
    <div>
      <p class="eyebrow">专注工作区</p>
      <h2 id="focus-heading">把下一段时间给一件事</h2>
    </div>
    {#if session}<p class="session-status"><span aria-hidden="true"></span>{pomodoroPhaseLabel(session.phase)} · {pomodoroStatusLabel(session.status)}</p>{/if}
  </header>

  {#if !tauriAvailable}<p class="browser-notice" role="status">浏览器预览不具备桌面持久化计时和阶段通知能力；专注控制已禁用。</p>{/if}
  {#if warning}<p class="warning" role="alert">{warning}</p>{/if}

  <div class="focus-grid">
    <div class="main-stage">
      <div class="phase-switch" aria-label="选择阶段">
        {#each ['focus', 'shortBreak', 'longBreak'] as phase}
          <button type="button" class:active={phaseForStart === phase} aria-pressed={phaseForStart === phase} disabled={active || busy || !tauriAvailable} onclick={() => choosePhase(phase as PomodoroPhase)}>{pomodoroPhaseLabel(phase as PomodoroPhase)}</button>
        {/each}
      </div>

      <div class="timer-readout" style={`--progress: ${progress}`}>
        <p class="timer" aria-label={`${active ? '剩余时间' : '阶段时长'} ${formatPomodoroDuration(displayedSeconds)}`}>{formatPomodoroDuration(displayedSeconds)}</p>
      </div>
      <p class="timer-context">{active ? (session?.taskTitleSnapshot ?? '独立专注') : `准备开始${pomodoroPhaseLabel(selectedPhase)}`}</p>

      <div class="controls" aria-label="番茄控制">
        <button type="button" class="primary" onclick={runPrimaryAction} disabled={!tauriAvailable || busy || primaryAction === 'none' || (primaryAction === 'start' && selectedPhase === 'focus' && taskSelectionPending)}>{busy ? '处理中…' : taskSelectionPending && primaryAction === 'start' ? '请重新选择任务' : primaryAction === 'pause' ? '暂停' : primaryAction === 'resume' ? '继续' : `开始${pomodoroPhaseLabel(selectedPhase)}`}</button>
        {#if active}
          <button type="button" onclick={() => void onSkip()} disabled={!tauriAvailable || busy}>跳过</button>
          <button type="button" class="quiet danger" onclick={() => void onReset()} disabled={!tauriAvailable || busy}>重置</button>
        {/if}
      </div>
    </div>

    <aside class="context" aria-label="专注上下文">
      <section>
        <p class="label">绑定任务</p>
        {#if active && session?.taskTitleSnapshot}
          <p class="bound-task">{session.taskTitleSnapshot}</p>
        {:else}
          <label class="task-select"><span class="sr-only">选择要绑定的任务</span>
            <select value={taskSelectionPending ? 'unavailable' : selectedTask?.taskId ?? ''} onchange={chooseTask} disabled={!phaseAllowsTask || active || busy || !tauriAvailable}>
              {#if taskSelectionPending}<option value="unavailable" disabled>所选任务不可用</option>{/if}
              <option value="">无关联任务</option>
              {#each availableTasks as task}<option value={task.taskId}>{task.title}</option>{/each}
            </select>
          </label>
          {#if selectedTask}<p class="task-meta">累计 {selectedTask.completedFocusCount} · 今日 {selectedTask.completedFocusTodayCount}</p>{/if}
        {/if}
      </section>

      <section class="metrics">
        <div><p class="label">本周期</p><strong>{snapshot?.completedFocusesInCycle ?? 0}<small> / {snapshot?.settings.longBreakInterval ?? 4}</small></strong></div>
        <div><p class="label">今日完成</p><strong>{snapshot?.completedFocusTodayCount ?? 0}</strong></div>
      </section>

      <section class="next-phase"><p class="label">下一阶段</p><p>{pomodoroPhaseLabel(snapshot?.recommendedPhase ?? 'focus')}</p></section>

      <div class="settings-toggle"><button type="button" class="quiet" aria-expanded={settingsOpen} onclick={() => settingsOpen = !settingsOpen}>设置{settingsOpen ? '收起' : '展开'}</button></div>
      {#if settingsOpen}
        <form class="settings" onsubmit={(event) => { event.preventDefault(); submitSettings(); }}>
          <p>新设置仅应用于下一阶段。</p>
          <label>专注（分钟）<input type="number" min="1" max="180" bind:value={settingsDraft.focusMinutes} disabled={!tauriAvailable || busy} /></label>
          <label>短休息（分钟）<input type="number" min="1" max="60" bind:value={settingsDraft.shortBreakMinutes} disabled={!tauriAvailable || busy} /></label>
          <label>长休息（分钟）<input type="number" min="1" max="60" bind:value={settingsDraft.longBreakMinutes} disabled={!tauriAvailable || busy} /></label>
          <label>长休息间隔<input type="number" min="2" max="12" bind:value={settingsDraft.longBreakInterval} disabled={!tauriAvailable || busy} /></label>
          <button type="submit" disabled={!tauriAvailable || busy}>保存设置</button>
        </form>
      {/if}
    </aside>
  </div>
  <p class="sr-only" aria-live="polite" aria-atomic="true">{announcement}</p>
</section>

<style>
  .focus-workspace { padding:20px 0; color:var(--text, #e7edf2); } .workspace-header { display:flex; align-items:flex-start; justify-content:space-between; gap:16px; padding-bottom:18px; border-bottom:1px solid var(--line, rgba(255,255,255,.11)); }.eyebrow,.label { margin:0; color:var(--muted, #9aa6b2); font-size:10px; letter-spacing:.14em; text-transform:uppercase; } h2 { margin:4px 0 0; font-size:clamp(19px,3vw,25px); font-weight:600; }.session-status { display:flex; align-items:center; gap:7px; margin:3px 0 0; color:var(--muted, #9aa6b2); font-size:12px; white-space:nowrap; }.session-status span { width:7px; height:7px; border-radius:50%; background:var(--tomato, #ec6945); }.browser-notice,.warning { margin:14px 0 0; padding:9px 11px; border-left:2px solid var(--blue, #5ba9ff); color:var(--muted, #9aa6b2); background:color-mix(in srgb, var(--blue, #5ba9ff) 9%, transparent); font-size:12px; line-height:1.5; }.warning { border-color:var(--warning, #e0ad65); color:var(--warning-text, #e8ca91); background:color-mix(in srgb, var(--warning, #e0ad65) 9%, transparent); }.focus-grid { display:grid; grid-template-columns:minmax(0,1fr) minmax(220px, .42fr); gap:clamp(22px,5vw,54px); padding-top:24px; }.main-stage { display:grid; align-content:start; min-height:320px; padding-right:clamp(0px,4vw,48px); border-right:1px solid var(--line, rgba(255,255,255,.11)); }.phase-switch { display:flex; flex-wrap:wrap; gap:4px; }.phase-switch button,.controls button,.settings button { border:1px solid var(--line, rgba(255,255,255,.11)); border-radius:4px; padding:6px 9px; color:var(--muted, #9aa6b2); background:transparent; font-size:12px; }.phase-switch button.active { border-color:color-mix(in srgb, var(--tomato, #ec6945) 65%, var(--line, transparent)); color:var(--tomato-light, #ffb09a); background:color-mix(in srgb, var(--tomato, #ec6945) 15%, transparent); }.timer-readout { margin:clamp(38px,8vw,78px) 0 0; border-bottom:2px solid color-mix(in srgb, var(--tomato, #ec6945) calc(var(--progress) * 100%), var(--line, rgba(255,255,255,.11))); }.timer { margin:0 0 8px; font-family:ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size:clamp(64px,14vw,132px); font-weight:500; letter-spacing:0; line-height:.9; font-variant-numeric:tabular-nums; }.timer-context { margin:18px 0 0; color:var(--muted, #9aa6b2); font-size:13px; }.controls { display:flex; flex-wrap:wrap; gap:8px; margin-top:30px; }.controls .primary { border-color:var(--tomato, #ec6945); color:#17110f; background:var(--tomato, #ec6945); font-weight:650; }.controls .danger { color:var(--danger, #ffaeae); }.context { display:grid; align-content:start; gap:22px; }.context section { padding-bottom:18px; border-bottom:1px solid var(--line, rgba(255,255,255,.11)); }.task-select select { width:100%; margin-top:8px; border:1px solid var(--line, rgba(255,255,255,.11)); border-radius:4px; padding:8px; color:inherit; background:var(--surface, #171a1f); font-size:13px; }.bound-task,.next-phase p:not(.label) { margin:8px 0 0; font-size:14px; }.task-meta { margin:7px 0 0; color:var(--muted, #9aa6b2); font-size:11px; }.metrics { display:grid; grid-template-columns:1fr 1fr; gap:12px; }.metrics div + div { padding-left:12px; border-left:1px solid var(--line, rgba(255,255,255,.11)); }.metrics strong { display:block; margin-top:6px; font-family:ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size:24px; font-weight:500; }.metrics small { color:var(--muted, #9aa6b2); font-size:12px; }.settings-toggle { display:flex; justify-content:flex-start; }.quiet { border:0 !important; padding-left:0 !important; color:var(--muted, #9aa6b2) !important; background:transparent !important; }.settings { display:grid; gap:9px; padding-top:2px; }.settings p { margin:0; color:var(--muted, #9aa6b2); font-size:11px; line-height:1.45; }.settings label { display:grid; gap:4px; color:var(--muted, #9aa6b2); font-size:11px; }.settings input { width:100%; box-sizing:border-box; border:1px solid var(--line, rgba(255,255,255,.11)); border-radius:4px; padding:7px 8px; color:inherit; background:rgba(0,0,0,.14); }.settings button { justify-self:start; margin-top:3px; color:var(--text, #e7edf2); }.sr-only { position:absolute; width:1px; height:1px; overflow:hidden; clip:rect(0,0,0,0); white-space:nowrap; } button:focus-visible,select:focus-visible,input:focus-visible { outline:2px solid var(--blue, #5ba9ff); outline-offset:2px; } button:disabled,select:disabled,input:disabled { cursor:not-allowed; opacity:.48; } @media (max-width:620px) { .focus-grid { grid-template-columns:1fr; gap:26px; }.main-stage { min-height:0; padding-right:0; padding-bottom:24px; border-right:0; border-bottom:1px solid var(--line, rgba(255,255,255,.11)); }.timer-readout { margin-top:50px; }.workspace-header { display:block; }.session-status { margin-top:12px; } } @media (prefers-reduced-motion: no-preference) { .timer { transition:color 140ms ease; } }
</style>
