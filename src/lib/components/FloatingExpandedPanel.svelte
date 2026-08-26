<script lang="ts">
  import { localDateForEpochMs, todayLocalDate, type Project, type Task } from '$lib/tasks';

  interface Props {
    focusActive: boolean;
    phaseLabel: string;
    statusLabel: string;
    remainingLabel: string;
    taskTitle: string | null;
    tasks: Task[];
    projects: Project[];
    busy: boolean;
    alwaysExpanded: boolean;
    userResized: boolean;
    onPrimary: () => void;
    onOpenFocus: () => void;
    onOpenTask: (taskId: number) => void;
    onHide: () => void;
    onAlwaysExpandedChange: (value: boolean) => void;
    onResetAutoSize: () => void;
  }

  let {
    focusActive,
    phaseLabel,
    statusLabel,
    remainingLabel,
    taskTitle,
    tasks,
    projects,
    busy,
    alwaysExpanded,
    userResized,
    onPrimary,
    onOpenFocus,
    onOpenTask,
    onHide,
    onAlwaysExpandedChange,
    onResetAutoSize
  }: Props = $props();

  function projectName(task: Task): string {
    if (task.projectId === null) return '收件箱';
    return projects.find((project) => project.id === task.projectId)?.name ?? `项目 #${task.projectId}`;
  }

  function formatClockTime(epochMs: number): string {
    const date = new Date(epochMs);
    return `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
  }

  function taskTimeLabel(task: Task): string {
    const now = Date.now();
    const today = todayLocalDate();
    const dueDate = task.dueAtUnixMs === null ? null : localDateForEpochMs(task.dueAtUnixMs);
    if (task.dueAtUnixMs !== null && task.dueAtUnixMs < now) return '已逾期';
    if (dueDate === today) return `今天 ${formatClockTime(task.dueAtUnixMs!)}`;
    if (task.plannedDate === today) return '今天计划';
    return '未安排';
  }
</script>

<div class="expanded-panel" data-tauri-drag-region="deep">
  <header>
    <button type="button" class="timer" onclick={onOpenFocus} aria-label="打开专注工作区" data-floating-focus-primary>
      <span class="phase">{phaseLabel}</span>
      <strong>{remainingLabel}</strong>
      <span class="status">{statusLabel} · {taskTitle ?? '未绑定任务'}</span>
    </button>
    <div class="actions">
      <button type="button" class="primary" onclick={onPrimary} disabled={busy}>
        {busy ? '…' : focusActive ? (statusLabel === '已暂停' ? '继续' : '暂停') : `开始${phaseLabel}`}
      </button>
      <button type="button" onclick={onHide} aria-label="隐藏悬浮窗">×</button>
    </div>
  </header>

  <div class="content-body">
    <div class="tasks-heading">
      <span>今天 + 逾期</span>
      <span class="count">{tasks.length}</span>
    </div>

    {#if tasks.length === 0}
      <p class="empty">今天没有待办任务</p>
    {:else}
      <ul>
        {#each tasks.slice(0, 5) as task (task.id)}
          <li>
            <button type="button" class="task" onclick={() => onOpenTask(task.id)} aria-label={`打开任务：${task.title}`}>
              <span class:overdue={task.dueAtUnixMs !== null && task.dueAtUnixMs < Date.now()} class="task-title">{task.title}</span>
              <span class="task-meta">{projectName(task)} · {taskTimeLabel(task)}</span>
            </button>
          </li>
        {/each}
      </ul>
      {#if tasks.length > 5}
        <p class="more">还有 {tasks.length - 5} 个任务</p>
      {/if}
    {/if}
  </div>

  <footer>
    <label>
      <input
        type="checkbox"
        checked={alwaysExpanded}
        onchange={(event) => onAlwaysExpandedChange(event.currentTarget.checked)}
      />
      始终展开
    </label>
    {#if userResized}
      <button type="button" onclick={onResetAutoSize}>恢复自动尺寸</button>
    {/if}
  </footer>
</div>

<style>
  .expanded-panel {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    gap: 8px;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    padding: 9px 11px 10px;
    box-sizing: border-box;
    color: var(--text, #f1f3f5);
  }

  header {
    display: flex;
    gap: 8px;
    min-width: 0;
  }

  button {
    border: 1px solid var(--line, rgba(232, 235, 240, 0.13));
    border-radius: var(--radius-sm, 5px);
    color: inherit;
    background: var(--surface-raised, #202329);
    cursor: pointer;
  }

  .timer {
    display: grid;
    min-width: 0;
    flex: 1;
    padding: 6px 9px;
    text-align: left;
  }

  .phase,
  .status,
  .tasks-heading,
  .task-meta,
  .empty,
  .more,
  footer {
    color: var(--muted, #949ba6);
    font-size: 10px;
  }

  .phase,
  .tasks-heading {
    letter-spacing: 0.1em;
  }

  .timer strong {
    margin-top: 2px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 23px;
    font-variant-numeric: tabular-nums;
    line-height: 1;
  }

  .status {
    min-width: 0;
    margin-top: 3px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    display: grid;
    gap: 5px;
  }

  .actions button,
  footer button {
    min-height: 29px;
    padding: 4px 8px;
    font-size: 11px;
  }

  .actions .primary {
    min-width: 58px;
    border-color: var(--accent, #f36b32);
    color: var(--accent-ink, #251007);
    background: var(--accent, #f36b32);
    font-weight: 650;
  }

  .content-body {
    min-height: 0;
    overflow: auto;
    scrollbar-gutter: stable;
  }

  .tasks-heading {
    display: flex;
    justify-content: space-between;
  }

  .count {
    color: var(--accent, #f36b32);
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }

  ul {
    display: grid;
    gap: 4px;
    margin: 6px 0 0;
    padding: 0;
    list-style: none;
  }

  .task {
    display: grid;
    gap: 2px;
    width: 100%;
    min-width: 0;
    padding: 5px 8px;
    background: transparent;
    text-align: left;
  }

  .task-title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
  }

  .task-title.overdue {
    color: var(--danger, #ffaaa1);
  }

  .empty,
  .more {
    margin: 7px 0 0;
  }

  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    min-width: 0;
  }

  footer label {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    white-space: nowrap;
  }

  button:hover:not(:disabled) {
    border-color: var(--text-soft, #c5c9d0);
    color: var(--text, #f1f3f5);
    background: var(--surface-hover, #272b31);
  }

  .actions .primary:hover:not(:disabled) {
    border-color: var(--accent-hover, #ff7a42);
    color: var(--accent-ink, #251007);
    background: var(--accent-hover, #ff7a42);
  }

  button:focus-visible,
  input:focus-visible {
    outline: 2px solid var(--info, #5ba9ff);
    outline-offset: 1px;
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.56;
  }
</style>
