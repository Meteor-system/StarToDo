# Task 8 Report — Adaptive WeekPlanner

## Scope and result

Implemented Task 8 in the isolated worktree `D:\Code\Rust\StarToDo-adaptive-focus-canvas-phase1` on branch `adaptive-focus-canvas-phase1`, starting from `c0c125280c660d34e548a3bd0930cc12e59ef36e`.

The planner now uses one task-card snippet and one reschedule/busy/error authority across its mounted wide and narrow surfaces. Its container width selects either the full eight-column locally scrolling board or an eight-tab narrow day selector with one locally scrolling selected tabpanel.

## TDD evidence

### RED

Command:

```powershell
npm run test:unit -- src/lib/planner-selection.test.ts
```

Observed before `src/lib/planner-selection.ts` existed:

```text
FAIL  src/lib/planner-selection.test.ts
Error: Cannot find module './planner-selection'
Test Files  1 failed (1)
Tests       no tests
```

This was the required genuine missing-module RED.

### GREEN

The same focused command after implementing the helper:

```text
✓ src/lib/planner-selection.test.ts (4 tests)
Test Files  1 passed (1)
Tests       4 passed (4)
```

## Verification

- `npm run check`: PASS — 0 errors, 0 warnings.
- `npm run test:unit`: PASS — 7 test files, 62 tests.
- `$env:CI='1'; npm run test:e2e -- '--grep=planner stays inside|planner layout'`: PASS — 13 tests. The repository's Playwright CLI forwarding treated the supplied grep form as an unfiltered run, so all 13 current E2E tests ran; the three required responsive planner cases were included and passed.
- `git diff --check`: PASS. Git emitted only the repository line-ending advisory that LF will become CRLF when Git next touches `WeekPlanner.svelte`; no whitespace errors were reported.
- UTF-8/nonempty audit: PASS for all four implementation/test paths.
- Static planner audit: PASS — exactly one `plannerTask` snippet, exactly one `role="tablist"`, required inline-size container query present, no `min-width: 1568px`, and narrow card IDs parameterized with `-narrow`.
- Browser error capture: PASS — no page errors or console errors in the responsive suite.

## Responsive E2E measurements

| Viewport | Planner container | Document | Expected surface |
| --- | --- | --- | --- |
| 520 × 420 | 440 × 334 | 520 × 420 | Narrow tablist + selected tabpanel |
| 760 × 560 | 680 × 474 | 760 × 560 | Narrow because actual container is below 760px |
| 1180 × 760 | 948 × 674 | 1180 × 760 | Wide eight-column board |

At every size, document `scrollWidth <= innerWidth` and `scrollHeight <= innerHeight`. The narrow day strip and selected-day body own local scrolling; the wide board owns local two-axis overflow without expanding the document.

## Self-review

- Selection starts on today when today is in the displayed week and otherwise on Monday; stale dates normalize after week navigation while `unscheduled` persists.
- The narrow selector renders one tablist, eight native-button tabs, one selected tab, shared `aria-controls`, and one tabpanel labelled by the selected tab. No tab receives `tabindex="-1"`; E2E verifies all eight can receive focus and activate with Enter.
- Historic wide IDs remain `planner-date-${task.id}` and `planner-error-${task.id}`. Narrow cards render through the same snippet with `-narrow`, consistently applied to `label for`, `select id`, `aria-describedby`, and error `id`.
- Task title, project/archive indication, priority, due, reminder, recurrence handling, select behavior, busy status, error alert, and `onOpenTask` behavior remain in the single shared snippet.
- Week navigation and return-to-this-week behavior remain unchanged.
- No second reschedule function, busy set, or error record was introduced.
- Semantic tokens inherited from Task 5 remain intact. Height ownership, `min-height: 0`, container query switching, and local overscroll containment were added without document overflow.
- The legacy mobile 1568px rule was removed. The existing 320px compatibility test also passed in the focused command's full E2E execution.

## Preserved overlap disclosure

`src/lib/components/WeekPlanner.svelte` was already dirty at dispatch with SHA256 `5E7E4FE90625E8BA3EA091DC94EDC0CEC653EBD7D9325DF98F9E2858628FBACF` and a baseline unstaged diff of exactly 35 additions / 4 deletions. That overlap consists of inherited Task 5 semantic-token/responsive styling. It was preserved. The explicit Task 8 ruling authorizes staging and committing the full final `WeekPlanner.svelte` path because this task owns the complete refactor. No other pre-existing dirty or untracked path is authorized for staging.

## Deferred Minors

- Full ArrowLeft/ArrowRight/Home/End tab movement remains deferred. All eight native tab buttons stay in sequential keyboard order and support Enter/Space activation, so no bucket is keyboard-inaccessible.
- Browser-preview E2E cannot seed or persist task mutations because this repository deliberately disables task persistence in preview. Task-level reschedule/error/recurrence interaction remains covered by preserved component code and static review; adding a Tauri-backed E2E fixture is deferred beyond Task 8.
- The focused Playwright grep argument was accepted by the requested npm command but did not filter the run in this environment. This costs execution time only; all 13 tests passed and the three planner cases were explicitly observed.
- Existing compact single-line component/CSS formatting elsewhere is not reformatted as part of Task 8.

## Rulings

- Ruling: Task 8 owns the full final `WeekPlanner.svelte` path — the authorized inherited styling overlaps the component-wide adaptive refactor — cost if wrong: inherited Task 5 semantic styling could be omitted from the commit or split into an unusable partial component state.
- Ruling: keep historic unsuffixed IDs on wide cards and append `-narrow` only through the shared snippet parameter — both surfaces remain mounted for CSS container switching — cost if wrong: duplicate DOM IDs would break label/error relationships and accessibility queries.
- Ruling: retain one shared task snippet with one reschedule/busy/error state authority — narrow and wide are presentations of the same planner behavior — cost if wrong: actions and errors could diverge or issue duplicate mutations.
- Ruling: switch layout from the planner's measured inline-size at 759px — drawer width, not viewport width, determines available planner space — cost if wrong: the 760px viewport with a 680px drawer would render an unusable wide board.
- Ruling: make the planner, wide board, day strip, and selected body explicit local scroll owners — the authority spec forbids document-level scrolling — cost if wrong: 520×420 and 320px preview layouts could overflow the page or hide planner controls.
- Ruling: preserve unscheduled selection across week changes but normalize stale date selections to the new week start — this is the exact helper contract and expected navigation behavior — cost if wrong: users could land on an empty/nonexistent day or lose deliberate unscheduled context.
- Ruling: keep both CSS-switched surfaces mounted — the brief explicitly states wide+narrow remain mounted and requires duplicate-ID mitigation for that architecture; reviewer advice to conditionally mount one tree was rejected as conflicting with binding authority — cost if wrong: changing to conditional mounting would depart from the specified implementation and alter component state/focus behavior.
- Ruling: keep all eight tab buttons in native sequential focus order rather than using an incomplete roving-tab model — Task 8 does not require arrow-key navigation, while `tabindex="-1"` without it makes seven buckets inaccessible — cost if wrong: keyboard users would need extra Tab presses through eight buckets, but every bucket remains reachable and activatable.
- Ruling: use measured container and document dimensions in E2E assertions — responsive behavior depends on drawer geometry and zero document overflow — cost if wrong: viewport-only assertions could pass while the actual planner container chooses the wrong layout.
- Ruling: record the unfiltered 13-test E2E result rather than weakening or renaming assertions — the supplied argument forwarding is environment-specific, but every required planner case ran and passed — cost if wrong: evidence could overstate grep selectivity, though not planner correctness.
