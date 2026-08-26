<script lang="ts">
  import ContextDrawer from './ContextDrawer.svelte';
  import FocusStage from './FocusStage.svelte';
  import {
    pomodoroPhaseLabel,
    pomodoroPrimaryAction,
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
    tasks?: PomodoroTaskSummary[];
    taskSummaries?: PomodoroTaskSummary[];
    selectedTaskId?: number | null;
    busy: boolean;
    warning: string | null;
    immersive: boolean;
    onReturnToTasks: () => void | Promise<void>;
    onEnterImmersive: () => void | Promise<void>;
    onExitImmersive: () => void | Promise<void>;
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
    immersive,
    onReturnToTasks,
    onEnterImmersive,
    onExitImmersive,
    onStart,
    onPause,
    onResume,
    onSkip,
    onReset,
    onUpdateSettings
  }: Props = $props();

  let selectedPhase = $state<PomodoroPhase>('focus');
  let nowUnixMs = $state(Date.now());
  let contextOpen = $state(false);
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
      void onStart({
        phase: selectedPhase,
        taskId: selectedPhase === 'focus' ? selectedTask?.taskId ?? null : null
      });
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

<section class="focus-scene" aria-labelledby="focus-heading" aria-busy={busy}>
  <header class="focus-header">
    <div>
      <p class="eyebrow">专注场景</p>
      <h2 id="focus-heading">把下一段时间给一件事</h2>
    </div>
    <div class="scene-actions">
      <button type="button" onclick={() => void onReturnToTasks()}>返回任务</button>
      <button type="button" aria-controls="focus-context-drawer" onclick={() => contextOpen = true}>专注上下文</button>
      {#if immersive}
        <button type="button" class="primary" onclick={() => void onExitImmersive()}>退出沉浸</button>
      {:else}
        <button type="button" class="primary" onclick={() => void onEnterImmersive()}>进入沉浸</button>
      {/if}
    </div>
  </header>

  <div class="notices">
    {#if !tauriAvailable}
      <p class="browser-notice" role="status">浏览器预览不具备桌面持久化计时和阶段通知能力；专注控制已禁用。</p>
    {/if}
    {#if warning}<p class="warning" role="alert">{warning}</p>{/if}
  </div>

  <div class="stage-canvas">
    <FocusStage
      {snapshot}
      {selectedPhase}
      selectedTaskTitle={selectedTask?.title ?? null}
      {nowUnixMs}
      {busy}
      {tauriAvailable}
      onChoosePhase={choosePhase}
      onPrimary={runPrimaryAction}
      onSkip={() => void onSkip()}
      onReset={() => void onReset()}
    />
  </div>

  <ContextDrawer open={contextOpen} drawerId="focus-context-drawer" title="专注上下文" onClose={() => contextOpen = false}>
    <div class="context-content">
      <section>
        <p class="label">绑定任务</p>
        {#if active && session?.taskTitleSnapshot}
          <p class="bound-task">{session.taskTitleSnapshot}</p>
        {:else}
          <label class="task-select">
            <span class="sr-only">选择要绑定的任务</span>
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

      <section><p class="label">下一阶段</p><p>{pomodoroPhaseLabel(snapshot?.recommendedPhase ?? 'focus')}</p></section>

      <button type="button" class="quiet settings-toggle" aria-expanded={settingsOpen} onclick={() => settingsOpen = !settingsOpen}>设置{settingsOpen ? '收起' : '展开'}</button>
      {#if settingsOpen}
        <form class="settings" onsubmit={(event) => { event.preventDefault(); submitSettings(); }}>
          <p>新设置仅应用于下一阶段。</p>
          <label>专注（分钟）<input type="number" min="1" max="180" bind:value={settingsDraft.focusMinutes} disabled={!tauriAvailable || busy} /></label>
          <label>短休息（分钟）<input type="number" min="1" max="60" bind:value={settingsDraft.shortBreakMinutes} disabled={!tauriAvailable || busy} /></label>
          <label>长休息（分钟）<input type="number" min="1" max="60" bind:value={settingsDraft.longBreakMinutes} disabled={!tauriAvailable || busy} /></label>
          <label>长休息间隔<input type="number" min="2" max="12" bind:value={settingsDraft.longBreakInterval} disabled={!tauriAvailable || busy} /></label>
          <button type="submit" class="primary" disabled={!tauriAvailable || busy}>保存设置</button>
        </form>
      {/if}
    </div>
  </ContextDrawer>
  <p class="sr-only" aria-live="polite" aria-atomic="true">{announcement}</p>
</section>

<style>
  .focus-scene {
    height: 100%;
    min-width: 0;
    min-height: 0;
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr);
    overflow: hidden;
    color: var(--text);
  }

  .focus-header {
    min-width: 0;
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--line);
  }

  .eyebrow,
  .label { margin: 0; color: var(--muted); font-size: 10px; letter-spacing: 0.14em; text-transform: uppercase; }
  h2 { margin: 4px 0 0; font-size: clamp(18px, 3vw, 25px); }
  .scene-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 6px; }
  button { border: 1px solid var(--line); border-radius: var(--radius-sm); padding: 7px 10px; color: var(--muted); background: transparent; font-size: 12px; }
  button.primary { border-color: var(--accent); color: #25110b; background: var(--accent); }
  button:disabled { cursor: not-allowed; opacity: 0.48; }
  .notices { min-height: 0; }
  .browser-notice,
  .warning { margin: 10px 0 0; padding: 7px 10px; border-left: 2px solid var(--info); color: var(--muted); background: color-mix(in srgb, var(--info) 9%, transparent); font-size: 12px; line-height: 1.45; }
  .warning { border-color: var(--warning); color: var(--warning-text); }
  .stage-canvas { min-width: 0; min-height: 0; display: grid; place-items: center; overflow: hidden; }
  .context-content { display: grid; gap: 18px; padding-top: 16px; }
  .context-content section { padding-bottom: 16px; border-bottom: 1px solid var(--line); }
  .context-content p { margin: 6px 0 0; }
  .task-select select { width: 100%; margin-top: 8px; border: 1px solid var(--line); border-radius: var(--radius-sm); padding: 8px; color: var(--text); background: var(--surface); }
  .task-meta { color: var(--muted); font-size: 11px; }
  .metrics { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px; }
  .metrics strong { display: block; margin-top: 6px; color: var(--text); font-size: 28px; }
  .metrics small { color: var(--muted); font-size: 12px; }
  .settings-toggle { justify-self: start; }
  .settings { display: grid; gap: 10px; }
  .settings > p { color: var(--muted); font-size: 12px; }
  .settings label { display: grid; grid-template-columns: 1fr 92px; align-items: center; gap: 12px; color: var(--text-soft); font-size: 12px; }
  .settings input { min-width: 0; border: 1px solid var(--line); border-radius: var(--radius-sm); padding: 7px; color: var(--text); background: var(--surface); }

  @media (max-width: 620px) {
    .focus-header { align-items: stretch; flex-direction: column; gap: 8px; }
    .scene-actions { justify-content: flex-start; }
  }

  @media (max-height: 559px) {
    .focus-header { align-items: center; padding-bottom: 6px; }
    .focus-header > div:first-child { display: none; }
    .browser-notice,
    .warning { margin-top: 5px; padding-block: 4px; font-size: 10px; }
    .scene-actions { justify-content: center; }
    button { padding-block: 5px; }
  }

  @media (max-width: 420px) and (max-height: 559px) {
    .scene-actions { width: 100%; }
    .scene-actions button { flex: 1; padding-inline: 5px; }
  }
</style>
