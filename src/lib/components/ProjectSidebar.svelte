<script lang="ts">
  import type { Project, ProjectSelection, Task } from '$lib/tasks';

  interface Props {
    projects: Project[];
    tasks: Task[];
    selectedProject: ProjectSelection;
    disabled?: boolean;
    onSelect: (selection: ProjectSelection) => void;
    onCreate: (name: string) => Promise<void>;
    onRename: (id: number, name: string) => Promise<void>;
    onArchive: (id: number) => Promise<void>;
    onRestore: (id: number) => Promise<void>;
  }

  let {
    projects,
    tasks,
    selectedProject,
    disabled = false,
    onSelect,
    onCreate,
    onRename,
    onArchive,
    onRestore
  }: Props = $props();
  let newProjectName = $state('');
  let editingProjectId = $state<number | null>(null);
  let editingName = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);

  let activeProjects = $derived(projects.filter((project) => project.archivedAtUnixMs === null));
  let archivedProjects = $derived(projects.filter((project) => project.archivedAtUnixMs !== null));

  function taskCount(projectId: number | null): number {
    return tasks.filter((task) => task.deletedAtUnixMs === null && task.projectId === projectId).length;
  }

  function selectProject(selection: ProjectSelection): void {
    error = null;
    onSelect(selection);
  }

  async function createProject(): Promise<void> {
    const name = newProjectName.trim();
    if (!name) {
      error = '请输入项目名称。';
      return;
    }

    busy = true;
    error = null;
    try {
      await onCreate(name);
      newProjectName = '';
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  function startRename(project: Project): void {
    editingProjectId = project.id;
    editingName = project.name;
    error = null;
  }

  function cancelRename(): void {
    editingProjectId = null;
    editingName = '';
  }

  async function saveRename(projectId: number): Promise<void> {
    const name = editingName.trim();
    if (!name) {
      error = '请输入项目名称。';
      return;
    }

    busy = true;
    error = null;
    try {
      await onRename(projectId, name);
      cancelRename();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function archiveProject(projectId: number): Promise<void> {
    busy = true;
    error = null;
    try {
      await onArchive(projectId);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function restoreProject(projectId: number): Promise<void> {
    busy = true;
    error = null;
    try {
      await onRestore(projectId);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  function handleNameKeydown(event: KeyboardEvent, submit: () => Promise<void>): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      cancelRename();
    } else if (event.key === 'Enter') {
      event.preventDefault();
      void submit();
    }
  }
</script>

<aside class="projects" aria-labelledby="projects-heading">
  <div class="heading"><div><p class="eyebrow">组织</p><h3 id="projects-heading">项目</h3></div><span>{activeProjects.length}</span></div>
  <div class="navigation" aria-label="项目筛选">
    <button type="button" class:active={selectedProject === 'all'} aria-pressed={selectedProject === 'all'} onclick={() => selectProject('all')} disabled={disabled || busy}>全部 <span>{tasks.filter((task) => task.deletedAtUnixMs === null).length}</span></button>
    <button type="button" class:active={selectedProject === 'inbox'} aria-pressed={selectedProject === 'inbox'} onclick={() => selectProject('inbox')} disabled={disabled || busy}>收件箱 <span>{taskCount(null)}</span></button>
    {#each activeProjects as project (project.id)}
      <div class="project-row">
        {#if editingProjectId === project.id}
          <input
            aria-label={`重命名项目：${project.name}`}
            bind:value={editingName}
            onkeydown={(event) => handleNameKeydown(event, () => saveRename(project.id))}
            disabled={busy}
          />
          <button type="button" onclick={() => void saveRename(project.id)} disabled={busy}>保存</button>
          <button type="button" class="quiet" onclick={cancelRename} disabled={busy}>取消</button>
        {:else}
          <button type="button" class:active={selectedProject === project.id} aria-pressed={selectedProject === project.id} onclick={() => selectProject(project.id)} disabled={disabled || busy}>{project.name} <span>{taskCount(project.id)}</span></button>
          <div class="row-actions">
            <button type="button" class="quiet" aria-label={`重命名项目：${project.name}`} onclick={() => startRename(project)} disabled={disabled || busy}>重命名</button>
            <button type="button" class="quiet" aria-label={`归档项目：${project.name}`} onclick={() => void archiveProject(project.id)} disabled={disabled || busy}>归档</button>
          </div>
        {/if}
      </div>
    {/each}
  </div>

  <form class="new-project" onsubmit={(event) => { event.preventDefault(); void createProject(); }}>
    <label for="new-project-name">新项目</label>
    <div><input id="new-project-name" bind:value={newProjectName} placeholder="例如：发布准备" disabled={disabled || busy} /><button type="submit" disabled={disabled || busy}>{busy ? '处理中…' : '添加'}</button></div>
  </form>

  {#if archivedProjects.length}
    <details>
      <summary>已归档项目（{archivedProjects.length}）</summary>
      <div class="archived">
        {#each archivedProjects as project (project.id)}
          <div class="project-row">
            <button type="button" class:active={selectedProject === project.id} aria-pressed={selectedProject === project.id} onclick={() => selectProject(project.id)} disabled={disabled || busy}>{project.name} <span>{taskCount(project.id)}</span></button>
            <button type="button" class="quiet" onclick={() => void restoreProject(project.id)} disabled={disabled || busy}>恢复</button>
          </div>
        {/each}
      </div>
    </details>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</aside>

<style>
  .projects { display:grid; gap:10px; margin-top:14px; padding:13px; border:1px solid var(--line); border-radius:6px; background:rgba(0,0,0,.1); }.heading { display:flex; justify-content:space-between; align-items:start; }.eyebrow { margin:0; color:var(--muted); font-size:10px; letter-spacing:.14em; text-transform:uppercase; } h3 { margin:4px 0 0; font-size:13px; font-weight:600; }.heading > span { color:var(--muted); font-size:12px; }.navigation,.archived { display:grid; gap:4px; }.project-row { display:grid; grid-template-columns:minmax(0,1fr) auto; align-items:center; gap:6px; }.navigation > button,.project-row > button { display:flex; justify-content:space-between; gap:10px; width:100%; border:1px solid transparent; border-radius:4px; padding:6px 7px; color:#d8e1eb; background:transparent; text-align:left; font-size:12px; }.navigation button.active,.project-row > button.active { border-color:rgba(91,169,255,.65); background:rgba(91,169,255,.18); }.navigation button span,.project-row > button span { color:var(--muted); font-variant-numeric:tabular-nums; }.row-actions { display:flex; gap:2px; }.quiet { border:0; padding:4px 6px; color:var(--muted); background:transparent; font-size:11px; }.new-project { display:grid; gap:5px; padding-top:4px; border-top:1px solid rgba(255,255,255,.08); }.new-project label { color:var(--muted); font-size:11px; }.new-project div { display:flex; gap:6px; }.new-project input,.project-row input { min-width:0; width:100%; border:1px solid var(--line); border-radius:4px; padding:6px 7px; color:inherit; background:rgba(0,0,0,.16); font-size:12px; }.new-project button,.project-row > button:not(.quiet) { border:1px solid var(--line); border-radius:4px; padding:6px 8px; color:#e5edf5; background:rgba(255,255,255,.05); font-size:12px; }.new-project button { flex:none; }.projects summary { color:var(--muted); cursor:pointer; font-size:11px; }.error { margin:0; color:#ffaeae; font-size:12px; line-height:1.5; } button:focus-visible,input:focus-visible { outline:2px solid var(--blue); outline-offset:2px; }
</style>
