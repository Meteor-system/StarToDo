<script lang="ts">
  import {
    initialPlannerSelection,
    normalizePlannerSelection,
    type PlannerSelection
  } from '$lib/planner-selection';
  import {
    addLocalCalendarDays,
    epochMsToLocalDateTime,
    errorMessage,
    priorityLabel,
    weekStartLocalDate,
    type Project,
    type Task
  } from '$lib/tasks';

  interface Props {
    tasks: Task[];
    projects: Project[];
    weekStart: string;
    today: string;
    onWeekChange: (weekStart: string) => void;
    onReschedule: (id: number, plannedDate: string | null) => Promise<void>;
    onOpenTask: (id: number) => Promise<void>;
  }

  const WEEKDAY_LABELS = ['周一', '周二', '周三', '周四', '周五', '周六', '周日'] as const;

  let {
    tasks,
    projects,
    weekStart,
    today,
    onWeekChange,
    onReschedule,
    onOpenTask
  }: Props = $props();
  function initialSelection(): PlannerSelection {
    return initialPlannerSelection(weekStart, today);
  }

  let busyTaskIds = $state<Set<number>>(new Set());
  let taskErrors = $state<Record<number, string>>({});
  let selection = $state<PlannerSelection>(initialSelection());

  let days = $derived(WEEKDAY_LABELS.map((label, index) => ({
    date: addLocalCalendarDays(weekStart, index) ?? weekStart,
    label
  })));
  let weekEnd = $derived(days[6]?.date ?? weekStart);
  let unscheduledTasks = $derived(tasks.filter((task) => task.plannedDate === null));
  let scheduledDays = $derived(days.map((day) => ({
    ...day,
    tasks: tasks.filter((task) => task.plannedDate === day.date)
  })));
  let otherWeekCount = $derived(tasks.filter((task) =>
    task.plannedDate !== null && (task.plannedDate < weekStart || task.plannedDate > weekEnd)
  ).length);
  let narrowTasks = $derived(
    selection === 'unscheduled'
      ? unscheduledTasks
      : scheduledDays.find((day) => day.date === selection)?.tasks ?? []
  );
  let selectedDay = $derived(
    selection === 'unscheduled'
      ? null
      : scheduledDays.find((day) => day.date === selection) ?? null
  );

  $effect(() => {
    selection = normalizePlannerSelection(selection, weekStart);
  });

  function projectFor(task: Task): Project | null {
    if (task.projectId === null) return null;
    return projects.find((project) => project.id === task.projectId) ?? null;
  }

  function formatEpoch(value: number): string {
    return epochMsToLocalDateTime(value).replace('T', ' ');
  }

  function shortDate(value: string): string {
    return value.slice(5);
  }

  function setBusy(taskId: number, busy: boolean): void {
    const next = new Set(busyTaskIds);
    if (busy) next.add(taskId);
    else next.delete(taskId);
    busyTaskIds = next;
  }

  function clearTaskError(taskId: number): void {
    if (!taskErrors[taskId]) return;
    const next = { ...taskErrors };
    delete next[taskId];
    taskErrors = next;
  }

  async function reschedule(task: Task, event: Event): Promise<void> {
    const select = event.currentTarget as HTMLSelectElement;
    const plannedDate = select.value || null;
    if (plannedDate === task.plannedDate) return;

    clearTaskError(task.id);
    setBusy(task.id, true);
    try {
      await onReschedule(task.id, plannedDate);
    } catch (cause) {
      select.value = task.plannedDate ?? '';
      taskErrors = { ...taskErrors, [task.id]: errorMessage(cause) };
    } finally {
      setBusy(task.id, false);
    }
  }

  function moveWeek(days: number): void {
    const next = addLocalCalendarDays(weekStart, days);
    if (next) onWeekChange(next);
  }

  function returnToThisWeek(): void {
    onWeekChange(weekStartLocalDate(today));
  }

</script>

{#snippet plannerTask(task: Task, idSuffix = '')}
  {@const project = projectFor(task)}
  {@const dateId = `planner-date-${task.id}${idSuffix}`}
  {@const errorId = `planner-error-${task.id}${idSuffix}`}
  <article class="planner-task">
    <h5>{task.title}</h5>
    <div class="metadata">
      {#if project}<span>{project.name}{project.archivedAtUnixMs !== null ? '（已归档）' : ''}</span>{/if}
      {#if task.priority !== 'none'}<span>{priorityLabel(task.priority)}优先级</span>{/if}
      {#if task.dueAtUnixMs !== null}<span>截止 {formatEpoch(task.dueAtUnixMs)}</span>{/if}
      {#if task.reminderAtUnixMs !== null}<span>提醒 {formatEpoch(task.reminderAtUnixMs)}</span>{/if}
    </div>
    {#if task.recurrenceKind === 'none'}
      <label for={dateId}>安排到</label>
      <select
        id={dateId}
        value={task.plannedDate ?? ''}
        aria-label={`安排“${task.title}”到`}
        aria-describedby={taskErrors[task.id] ? errorId : undefined}
        disabled={busyTaskIds.has(task.id)}
        onchange={(event) => void reschedule(task, event)}
      >
        <option value="">未排期</option>
        {#each days as day}
          <option value={day.date}>{day.label} {shortDate(day.date)}</option>
        {/each}
      </select>
    {:else}
      <div class="recurring-actions">
        <span class="recurring-badge">重复任务</span>
        <button type="button" class="quiet" onclick={() => void onOpenTask(task.id)}>在列表中处理</button>
      </div>
    {/if}
    {#if busyTaskIds.has(task.id)}<p class="busy" role="status">正在更新计划日期…</p>{/if}
    {#if taskErrors[task.id]}<p class="error" id={errorId} role="alert">{taskErrors[task.id]}</p>{/if}
  </article>
{/snippet}

<section class="planner" aria-labelledby="week-planner-heading">
  <header class="planner-header">
    <div>
      <p class="eyebrow">周计划</p>
      <h3 id="week-planner-heading">
        <time datetime={weekStart}>{weekStart}</time>
        <span aria-hidden="true"> — </span>
        <time datetime={weekEnd}>{weekEnd}</time>
      </h3>
      <p class="scope-note">仅显示进行中的任务；卡片位置只由计划日期决定。</p>
    </div>
    <div class="week-actions" role="group" aria-label="周计划导航">
      <button type="button" onclick={() => moveWeek(-7)} aria-label="查看上一周">上一周</button>
      <button type="button" class="quiet" onclick={returnToThisWeek}>本周</button>
      <button type="button" onclick={() => moveWeek(7)} aria-label="查看下一周">下一周</button>
    </div>
  </header>

  <div class="planner-summary" aria-label="周计划任务摘要">
    <span>未排期 {unscheduledTasks.length}</span>
    <span>本周 {scheduledDays.reduce((count, day) => count + day.tasks.length, 0)}</span>
    <span>其他周 {otherWeekCount}</span>
  </div>

  <div class="planner-narrow">
    <div class="day-strip" role="tablist" aria-label="周计划日期">
      <button
        id="planner-tab-unscheduled"
        type="button"
        role="tab"
        aria-selected={selection === 'unscheduled'}
        aria-controls="planner-selected-panel"
        onclick={() => selection = 'unscheduled'}
      >
        <span>未排期</span>
        <strong>{unscheduledTasks.length}</strong>
      </button>
      {#each scheduledDays as day (day.date)}
        <button
          id={`planner-tab-${day.date}`}
          type="button"
          role="tab"
          aria-selected={selection === day.date}
          aria-controls="planner-selected-panel"
          onclick={() => selection = day.date}
        >
          <span>{day.label}</span>
          <time datetime={day.date}>{shortDate(day.date)}</time>
          <strong>{day.tasks.length}</strong>
        </button>
      {/each}
    </div>

    <div
      id="planner-selected-panel"
      class:today={selectedDay?.date === today}
      class="selected-day-body"
      role="tabpanel"
      aria-labelledby={selection === 'unscheduled' ? 'planner-tab-unscheduled' : `planner-tab-${selection}`}
      tabindex="0"
    >
      <header class="selected-day-heading">
        <div>
          <h4>
            {#if selection === 'unscheduled'}
              未排期
            {:else if selectedDay}
              <span>{selectedDay.label}</span>
              <time datetime={selectedDay.date}>{selectedDay.date}</time>
              {#if selectedDay.date === today}<strong>今天</strong>{/if}
            {/if}
          </h4>
          <p class="column-note">
            {selection === 'unscheduled'
              ? '尚未安排具体日期'
              : selectedDay?.date === today
                ? '当前本地日期'
                : '按计划日期显示'}
          </p>
        </div>
        <span>{narrowTasks.length}</span>
      </header>
      <div class="task-stack">
        {#each narrowTasks as task (task.id)}
          {@render plannerTask(task, '-narrow')}
        {:else}
          <p class="empty-column">
            {selection === 'unscheduled' ? '没有未排期任务。' : '这一天还没有任务。'}
          </p>
        {/each}
      </div>
    </div>
  </div>

  <div class="planner-scroll" role="region" aria-label="周计划看板，可横向滚动">
    <div class="planner-grid">
      <section class="planner-column" aria-labelledby="unscheduled-heading">
        <header class="column-heading">
          <h4 id="unscheduled-heading">未排期</h4>
          <span>{unscheduledTasks.length}</span>
        </header>
        <p class="column-note">尚未安排具体日期</p>
        <div class="task-stack">
          {#each unscheduledTasks as task (task.id)}
            {@render plannerTask(task)}
          {:else}
            <p class="empty-column">没有未排期任务。</p>
          {/each}
        </div>
      </section>

      {#each scheduledDays as day (day.date)}
        <section class:today={day.date === today} class="planner-column" aria-labelledby={`day-heading-${day.date}`}>
          <header class="column-heading">
            <h4 id={`day-heading-${day.date}`}>
              <span>{day.label}</span>
              <time datetime={day.date}>{day.date}</time>
              {#if day.date === today}<strong>今天</strong>{/if}
            </h4>
            <span>{day.tasks.length}</span>
          </header>
          <p class="column-note">{day.date === today ? '当前本地日期' : '按计划日期显示'}</p>
          <div class="task-stack">
            {#each day.tasks as task (task.id)}
              {@render plannerTask(task)}
            {:else}
              <p class="empty-column">这一天还没有任务。</p>
            {/each}
          </div>
        </section>
      {/each}
    </div>
  </div>
</section>

<style>
  .planner { height:100%; min-height:0; min-width:0; display:grid; grid-template-rows:auto auto minmax(0,1fr); container-type:inline-size; overflow:hidden; padding-top:20px; }
  .planner-header { display:flex; align-items:flex-start; justify-content:space-between; gap:16px; padding-bottom:13px; border-bottom:1px solid var(--line); }
  .eyebrow { margin:0; color:var(--muted); font-size:10px; letter-spacing:.14em; text-transform:uppercase; }
  .planner h3 { display:flex; flex-wrap:wrap; gap:4px; margin:4px 0 0; color:var(--text); font-size:15px; font-weight:650; }
  .scope-note,.column-note,.busy,.empty-column { margin:5px 0 0; color:var(--muted); font-size:11px; line-height:1.5; }
  .week-actions { display:flex; gap:6px; flex-wrap:wrap; justify-content:flex-end; }
  .week-actions button { border:1px solid var(--line-strong); border-radius:var(--radius-sm); padding:6px 8px; color:var(--text-soft); background:transparent; font-size:12px; }
  .week-actions button:hover:not(:disabled) { border-color:var(--text-soft); background:var(--surface-hover); color:var(--text); }
  .week-actions .quiet,.quiet { border-color:transparent; color:var(--muted); background:transparent; }
  .planner-summary { display:flex; flex-wrap:wrap; gap:8px; padding:11px 0; color:var(--muted); font-size:11px; font-variant-numeric:tabular-nums; }
  .planner-summary span { padding-right:8px; border-right:1px solid var(--line); }
  .planner-summary span:last-child { padding-right:0; border-right:0; }
  .planner-narrow { display:none; }
  .planner-scroll { min-width:0; min-height:0; overflow:auto; overscroll-behavior:contain; padding-bottom:9px; scrollbar-gutter:stable; }
  .planner-grid { display:grid; grid-template-columns:repeat(8,minmax(210px,1fr)); gap:8px; min-width:1728px; align-items:start; }
  .planner-column { min-height:240px; padding:11px; border:1px solid var(--line); border-radius:var(--radius-md); background:var(--surface); }
  .planner-column.today,.selected-day-body.today { border-color:rgba(91,169,255,.62); box-shadow:inset 0 2px 0 var(--info); }
  .column-heading,.selected-day-heading { display:flex; align-items:flex-start; justify-content:space-between; gap:8px; padding-bottom:8px; border-bottom:1px solid var(--line); }
  .column-heading h4,.selected-day-heading h4 { display:grid; gap:2px; margin:0; color:var(--text); font-size:12px; font-weight:650; }
  .column-heading h4 time,.selected-day-heading h4 time { color:var(--muted); font-size:10px; font-variant-numeric:tabular-nums; }
  .column-heading h4 strong,.selected-day-heading h4 strong { color:var(--info); font-size:10px; font-weight:650; }
  .column-heading > span,.selected-day-heading > span { color:var(--muted); font-size:11px; font-variant-numeric:tabular-nums; }
  .task-stack { display:grid; gap:8px; padding-top:10px; }
  .planner-task { padding:10px; border:1px solid var(--line); border-radius:var(--radius-sm); background:#16181c; }
  .planner-task h5 { margin:0; color:var(--text); overflow-wrap:anywhere; font-size:13px; font-weight:620; line-height:1.4; }
  .metadata { display:grid; gap:3px; margin-top:6px; color:var(--muted); font-size:10px; line-height:1.4; }
  .planner-task label { display:block; margin-top:9px; color:var(--muted); font-size:10px; }
  .planner-task select { width:100%; margin-top:4px; border:1px solid var(--line-strong); border-radius:var(--radius-sm); padding:6px 7px; color:var(--text); background:#111214; font-size:11px; }
  .recurring-actions { display:flex; align-items:center; justify-content:space-between; gap:6px; margin-top:9px; }
  .recurring-badge { color:var(--text-soft); font-size:10px; }
  .recurring-actions button { padding:4px 5px; font-size:10px; }
  .busy { color:var(--info); }
  .error { margin:6px 0 0; color:var(--danger); font-size:11px; line-height:1.4; }
  .empty-column { padding:8px 0; }

  @container (max-width:759px) {
    .planner-scroll { display:none; }
    .planner-narrow { min-width:0; min-height:0; display:grid; grid-template-rows:auto minmax(0,1fr); overflow:hidden; }
    .day-strip { display:flex; min-width:0; gap:6px; overflow-x:auto; overscroll-behavior-inline:contain; padding-bottom:8px; scrollbar-gutter:stable; }
    .day-strip button { flex:0 0 auto; min-width:72px; display:grid; grid-template-columns:1fr auto; gap:2px 6px; border:1px solid var(--line); border-radius:var(--radius-sm); padding:7px 8px; color:var(--muted); background:transparent; text-align:left; }
    .day-strip button[aria-selected='true'] { border-color:rgba(91,169,255,.65); color:var(--text); background:rgba(91,169,255,.18); }
    .day-strip time { grid-column:1; font-size:10px; font-variant-numeric:tabular-nums; }
    .day-strip strong { grid-column:2; grid-row:1 / span 2; align-self:center; color:var(--text-soft); font-size:11px; font-weight:650; }
    .selected-day-body { min-width:0; min-height:0; overflow:auto; overscroll-behavior:contain; border:1px solid var(--line); border-radius:var(--radius-md); padding:11px; background:var(--surface); }
  }

  @media (max-width:640px) {
    .planner-header { display:grid; }
    .week-actions { justify-content:flex-start; }
  }

  @media (max-height:559px) {
    .planner { padding-top:10px; }
    .scope-note { display:none; }
    .planner-header { gap:8px; padding-bottom:8px; }
    .planner-summary { padding:7px 0; }
  }
</style>
