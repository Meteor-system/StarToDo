<script lang="ts">
  import TaskItem from '$lib/components/TaskItem.svelte';
  import type {
    MutationWarningResult,
    Project,
    Task,
    TaskChangeKind,
    TaskCompletionMutationWithToken,
    TaskInput,
    TaskMutationWithToken
  } from '$lib/tasks';

  interface Props {
    activeTasks: Task[];
    completedTasks: Task[];
    trashTasks: Task[];
    projects: Project[];
    activationId: number | null;
    pomodoroCounts: ReadonlyMap<number, number>;
    trashMode?: boolean;
    onOpenDetails: (taskId: number) => void;
    onFocus?: (taskId: number) => void;
    onUpdate: (id: number, input: TaskInput) => Promise<TaskMutationWithToken>;
    onCompleted: (id: number, completed: boolean) => Promise<TaskCompletionMutationWithToken>;
    onSnooze: (id: number, untilUnixMs: number) => Promise<TaskMutationWithToken>;
    onDeferToTomorrow: (id: number) => Promise<TaskMutationWithToken>;
    onDelete: (id: number) => Promise<MutationWarningResult>;
    onChanged: (task: Task, kind: TaskChangeKind, result: TaskMutationWithToken) => Promise<boolean>;
    onRemoved: (id: number, result: MutationWarningResult) => Promise<boolean>;
    onRestore: (id: number) => Promise<TaskMutationWithToken>;
    onPermanentlyDelete: (id: number) => Promise<MutationWarningResult>;
    onRestored: (task: Task, result: TaskMutationWithToken) => Promise<boolean>;
    onPermanentlyRemoved: (id: number, result: MutationWarningResult) => Promise<boolean>;
  }

  let {
    activeTasks, completedTasks, trashTasks, projects, activationId, pomodoroCounts,
    trashMode = false, onOpenDetails, onFocus, onUpdate, onCompleted, onSnooze, onDeferToTomorrow,
    onDelete, onChanged, onRemoved, onRestore, onPermanentlyDelete, onRestored,
    onPermanentlyRemoved
  }: Props = $props();
</script>

<div class="task-scroll" data-scroll-region="tasks" aria-busy="false">
  {#if trashMode}
    <section class="task-group trash-group" aria-labelledby="trash-heading">
      <h3 id="trash-heading">已删除 <span>{trashTasks.length}</span></h3>
      {#if trashTasks.length === 0}<p class="empty">回收站是空的。</p>
      {:else}{#each trashTasks as task (task.id)}
        <TaskItem {task} {projects} highlighted={task.id === activationId} trashMode={true} {onOpenDetails} {onCompleted} {onChanged} {onRestore} {onPermanentlyDelete} {onRestored} {onPermanentlyRemoved} />
      {/each}{/if}
    </section>
  {:else}
    <section class="task-group" aria-labelledby="active-heading">
      <h3 id="active-heading">进行中 <span>{activeTasks.length}</span></h3>
      {#if activeTasks.length}{#each activeTasks as task (task.id)}
        <TaskItem {task} {projects} highlighted={task.id === activationId} pomodoroCount={pomodoroCounts.get(task.id) ?? 0} {onOpenDetails} {onFocus} {onCompleted} {onSnooze} {onDeferToTomorrow} {onChanged} />
      {/each}{:else}<p class="muted">目前没有匹配的进行中任务。</p>{/if}
    </section>
    <section class="task-group completed" aria-labelledby="completed-heading">
      <h3 id="completed-heading">已完成 <span>{completedTasks.length}</span></h3>
      {#if completedTasks.length}{#each completedTasks as task (task.id)}
        <TaskItem {task} {projects} highlighted={task.id === activationId} pomodoroCount={pomodoroCounts.get(task.id) ?? 0} {onOpenDetails} {onCompleted} {onSnooze} {onDeferToTomorrow} {onChanged} />
      {/each}{:else}<p class="muted">目前没有匹配的已完成任务。</p>{/if}
    </section>
  {/if}
</div>

<style>
  .task-scroll { min-height: 0; overflow: auto; overscroll-behavior: contain; scrollbar-gutter: stable; }
  .task-group { padding-top: 12px; }
  .task-group h3 { margin: 0 0 8px; font-size: 13px; }
  .task-group h3 span { color: var(--muted); font-weight: 400; }
</style>
