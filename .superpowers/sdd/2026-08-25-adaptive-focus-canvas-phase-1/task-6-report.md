# Task 6 report

Implemented the TaskScene adaptive-focus canvas conversion from the recovered ordinary-main baseline.

## Implementation

- Converted `TaskScene.svelte` from the legacy workspace/list-week/trash replacement model to a persistent task scene with a local `TaskCanvas` and contextual drawers.
- Removed `compact`, `WorkspaceView`, `workspaceView`, `activeWorkspaceView`, `WORKSPACE_VIEW_STORAGE_KEY`, `showTrash`, and all compact/workspace branching.
- Preserved operation tokens, stale-response discard, load generations, activation retry and acknowledgement sequencing, reminder warning reporting, Pomodoro callbacks, and existing mutation callbacks.
- Added `TaskDrawer`, `activeDrawer`, `visibleDeletedTasks`, `openTaskDrawer`, `closeTaskDrawer`, and planner return behavior.
- Added five `ContextDrawer` instances: detailed create, filters, projects, wide planner, and trash.
- Kept the normal task canvas visible independently of drawer state; trash renders `visibleDeletedTasks` in trash mode.
- Added the required TaskComposer height-density media queries without changing parsing, submit, partial retry, or Ctrl/Cmd+Enter behavior.
- Updated the keyboard E2E assertion from removed legacy list/week controls to the new drawer-tool controls.

## Recovery incident note

The controller restored `src/lib/components/TaskScene.svelte` exactly from untouched ordinary-main `TaskWorkspace.svelte` (UTF-8 SHA256 `5D505F98CA485D3D2AE317784531455232F208A33C698BDF23F3F13419724D06`) after prior non-convergent partial edits. This pass retained that recovered script's operation-token, load-generation, activation, reminder-warning, and callback behavior while replacing only the requested workspace presentation/state model.

## Required RED evidence

The brief's required RED test was already present in `e2e/preview.spec.ts` as `opens task tools as contextual drawers`. Before the implementation, it represented the expected failure mode where week/trash replaced the workspace rather than opening drawers. After the implementation it passes.

## Validation

- `npm run check`: passed, 0 errors and 0 warnings.
- `npm run test:unit`: passed, 51/51 tests.
- `$env:CI='1'; npm run test:e2e -- '--grep=task tools as contextual drawers|persistence|workspace'`: passed, 9/9 tests.
- `$env:CI='1'; npm run test:e2e`: passed, 9/9 tests.
- `git diff --check`: passed.
- UTF-8/non-empty validation for all Task 6 outputs: passed.
- Activation invariant audit: `focusActivation` uses local `[data-scroll-region="tasks"]` `scrollTo`; no `target.scrollIntoView` remains. TaskCanvas details callback is an intentional no-op placeholder until the details contract is introduced.
- Post-commit correction evidence: explicitly staged `D src/lib/components/TaskWorkspace.svelte` with the `TaskScene.svelte` replacement; cached diff was inspected to ensure the legacy component deletion is captured without restoring it.
- Final TaskScene legacy-symbol audit (`compact`, workspace-view aliases, `showTrash`, old `TaskItem` list): no matches.
- Fix round 1 rerun: `npm run check` passed with 0 errors/0 warnings; `npm run test:unit` passed 51/51; focused drawer/persistence/workspace E2E passed 9/9; `git diff --check` passed; TaskScene contains two local task-region references (planner return and activation) and no `scrollIntoView`.

## Staged-overlap paths/hunks

Only Task 6 scope is staged: `src/lib/components/TaskScene.svelte`, `src/lib/components/TaskCanvas.svelte`, `src/lib/components/TaskComposer.svelte`, `src/routes/+page.svelte`, `e2e/preview.spec.ts`, and this report. No unrelated dirty baseline files are staged. `+page.svelte` retains the unrelated `FocusMiniBar compact={false}` prop; TaskScene itself is rendered without a compact prop.

## Validation substrate and scope

The verification ran in the Phase 1 worktree, which intentionally preserves prerequisite baseline changes outside the Task 6 commit range. In particular, Task 6 consumes the Task 5 `ContextDrawer.svelte` contract and depends on preserved baseline versions of `ProjectSidebar.svelte`, `TaskItem.svelte`, and `WeekPlanner.svelte`. Those dependency paths remain dirty/uncommitted relative to ordinary main and were deliberately not staged in Task 6. The committed Task 6 files are `TaskScene.svelte`, `TaskCanvas.svelte`, `TaskComposer.svelte`, `+page.svelte`, `e2e/preview.spec.ts`, this report, and the explicit deletion of `TaskWorkspace.svelte`. A standalone cherry-pick onto a clean ordinary-main checkout requires the prerequisite Task 5/baseline patch containing those dependency paths.

## Deferred Minors

- `TaskCanvas.onOpenDetails` remains in the brief-required interface as an intentional no-op boundary. Task 7 owns task-details drawer wiring; cost if wrong: the callback remains dead until Task 7, without causing accidental planner navigation.
- Task 5's documented Minor remains deferred: actual immersive-display integration is owned by Task 9; `immersiveDisplay` remains intentionally initialized to `off`.
- No unrelated refactors or Phase 2/3 behavior were introduced.

## Self-review

The scene has the required `height:100%`, `min-height:0`, `grid-template-rows:auto auto minmax(0,1fr)`, and `container-type:inline-size`. It contains one search input and exact controls named `详细新建`, `筛选`, `项目`, `周计划`, and `回收站`. The normal `TaskCanvas` remains mounted outside drawers. Planner return closes the drawer, resets execution/status/search filters, awaits `tick`, scrolls the local `[data-scroll-region="tasks"]` region, and focuses `task-action-${id}`. No generated artifacts were added.
