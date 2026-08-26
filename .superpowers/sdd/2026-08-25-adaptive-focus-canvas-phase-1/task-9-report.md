# Task 9 Report

Status: implemented, verified, independently reviewed, controller follow-up fixed, and committed in two scoped Task 9 commits.

## Scope and overlap disclosure

- Worktree: `D:\Code\Rust\StarToDo-adaptive-focus-canvas-phase1`
- Branch: `adaptive-focus-canvas-phase1`
- Verified dispatch HEAD before mutation: `214eea2324f6b82eec1e2444d9251f2a46b40053`.
- Adopted the authorized untracked `src/lib/components/FocusWorkspace.svelte` prerequisite at SHA256 `75F38E3F8478CD6427D0A814064AEB9DC35097446F1BF84F542F827344CA37EA`, 15041 bytes, into the new `FocusScene.svelte`. Because the source was untracked, there is intentionally no fabricated Git rename or staged deletion. The source path is absent at final.
- Finalized the authorized untracked `src/lib/components/FocusMiniBar.svelte` prerequisite from SHA256 `68F25FCA9E9A535818558B085D8F42F7132BB5F8B960C8669A06A0FA4DF2EDD7`, 3835 bytes.
- All other pre-existing dirty and untracked paths were preserved. Only the nine Task 9 output paths are eligible for staging.

## Implementation

- Added the exact binding E2E `shows a centered focus stage and exits visual immersive mode` before production changes.
- Replaced the workspace composition with one height-contained `FocusScene`, one exact-interface `FocusStage`, one focus-context `ContextDrawer`, and one `AutoImmersivePrompt`.
- Preserved backend snapshot authority, selected phase, selected task reconciliation/unavailability behavior, settings draft, timer helpers, status announcements, command callbacks, and browser-only backend disabling.
- Added `role="timer"` with the exact dynamic label, a semantic conic progress ring, 3.2-second breathing only while running, and an explicit reduced-motion animation shutdown. No Phase 2 constellation was added.
- Kept phase, timer, selected task context, primary control, return, context, and enter/exit immersive controls resident. Moved task selector, cycle/today metrics, next phase, and settings into the single local-scroll drawer.
- Implemented the exact prompt copy and controls, conservative initial focus, Tab/Shift+Tab containment, Escape cancellation with propagation stopped, and practical focus restoration.
- Changed `runPomodoroCommand` to `Promise<boolean>` with success only after command plus successful authoritative refresh; unavailable, busy, error, stale/failed refresh paths return false.
- Added pending-start generation and session identity checks to prevent stale prompt input from double-starting after a newer accepted Pomodoro view.
- Implemented browser visual fallback, Tauri system immersive, Tauri-entry fallback warning, serialized exit, system-exit failure retention, page/Escape exit, task-scene exit coordination, and no implicit immersive behavior for floating intents or pause/resume.
- Added the diagnostics auto-immersive selector using the existing typed preference authority and page-owned state/localStorage callback.
- Removed all FocusMiniBar compact props/branches/CSS. The task title is hidden only by CSS below 560px height.

## TDD RED and GREEN

Genuine RED command:

`$env:CI='1'; npx playwright test --grep 'centered focus stage'`

Result: 1 failed, retried once by CI. The failure was at `getByRole('timer')`: the current timer had no timer role, so the exact binding test could not reach the also-missing `进入沉浸` behavior. This is the expected pre-production semantic/immersive gap, not a typo or infrastructure error.

GREEN command:

`$env:CI='1'; npx playwright test --grep 'centered focus stage'`

Result: 1 passed.

## Verification evidence

- `npm run check`: PASS, 0 errors and 0 warnings.
- `npm run test:unit`: PASS, 7 test files and 62 tests.
- Required npm-forwarding E2E command: `$env:CI='1'; npm run test:e2e -- '--grep=centered focus stage|navigates between tasks and focus|focus layout'`.
  - The package invocation did not forward the filter: Playwright ran 18 tests rather than the intended subset.
  - Result: PASS, 18 tests.
- Direct exact GREEN filter: 1 passed.
- Direct focus regression after review fixes: 5 passed (`centered focus stage|focus layout`).
- `git diff --check`: PASS; only Git's informational LF-to-CRLF warning for `e2e/responsive.spec.ts` was emitted.
- UTF-8/nonempty audit: all eight source/test outputs were strict UTF-8 and nonempty; report is UTF-8 by construction.
- Static audit:
  - `FocusWorkspace` references/source: none; source absent.
  - compact prop/branches: none.
  - Phase 2 constellation terms in focus components: none.
  - FocusScene authority: one page import/render.
  - FocusStage authority: one FocusScene import/render.
  - AutoImmersivePrompt authority: one page import/render.
  - Focus context drawer authority: one ContextDrawer in FocusScene.

## Responsive measurements

All document dimensions equal the viewport in both axes.

- 320x720: document 320x720; canvas 292x333.21875; stage 292x254.96875; timer center aligned horizontally and within the centered stage tolerance.
- 520x420: document 520x420; canvas 478.40625x135.5; stage exactly 478.40625x135.5 after the short-height containment fix; timer 83.109375x33.59375.
- 1180x760: document 1180x760; canvas 1035x487.609375; stage 640x303.859375; timer 206.84375x83.59375.
- Reduced-motion 520x420: PASS; ring core computed `animation-name: none`, with both document axes within viewport.
- Existing Task 8 planner tests remained intact and passed at 520x420, 760x560, and 1180x760.

## Independent review and fixes

Independent review found no Critical issues. It found three actionable Important implementation issues and one Important coverage observation.

Fixes applied:

- Added pending-start generation plus current-session identity validation so a refresh or external session transition makes an old prompt input ineligible to start.
- Added `stopPropagation()` to prompt Escape so the child modal owns the event and does not also exit an underlying immersive display.
- Serialized immersive exit calls with one in-flight promise and made task-scene navigation await that flight; failed system exit keeps the focus scene and immersive state.
- Re-ran `npm run check` and the five focused immersive/layout E2Es after fixes; all passed.

The review's broad request for controllable Tauri command-response integration tests cannot be implemented honestly in browser preview without introducing a second backend authority or expanding the authorized file list. The browser tests cover manual fallback, Escape off, centering, document containment, resident controls, reduced motion, and console errors. The Tauri-only system entry/exit failure branches remain bounded verification Minors for desktop smoke/integration coverage.

## Controller review follow-up

The controller review found two valid Important defects and three bounded Minors after commit `4ef5783490550eca65ef002e257f6e347118c1ed`.

Fixes applied:

- Floating task intents now call and await `changeScene('tasks')`. They recheck disposal and that the task scene actually became active before revealing or activating the task. A failed system exit therefore leaves FocusScene/system authoritative. Floating focus intents remain a direct focus transition plus refresh and never auto-enter immersive.
- Added one `immersiveEnterFlight` and coordinated it bidirectionally with `immersiveExitFlight`. Duplicate enters reuse one promise. Enter waits for an existing exit and aborts when that exit fails. Exit waits for an existing enter, rechecks the exit single-flight slot, then performs exactly one system/fallback/off exit. Task navigation and Escape treat an enter flight as immersive work. Auto-start enters immersive only when its command/refresh succeeds and the active scene is still focus.
- Replaced accepted-view generation with a current-session transition epoch. `acceptPomodoroSnapshot()` is the sole snapshot acceptance path for command results and refreshed views, and increments the epoch only when `currentSession?.id` changes. A harmless same-session refresh preserves a prompt; null→id, id→another id, and id→null transitions invalidate it.
- Added `aria-expanded` to the focus-context trigger and replaced Task 9 hardcoded primary-button ink with `var(--accent-ink)`.
- Added browser visual-fallback → task transition coverage.
- Made reduced-motion evidence non-vacuous: the test explicitly emulates reduced motion, confirms the media query, forces the ring through a running selector, and then verifies the ring core computes `animation-name: none`. This uncovered and fixed a scoped CSS reduced-motion defect.

Fresh follow-up verification:

- `npm run check`: PASS, 0 errors and 0 warnings.
- `npm run test:unit`: PASS, 7 test files and 62 tests.
- Direct focused E2E `centered focus stage|focus layout|navigates between tasks and focus|returns from browser visual immersive mode`: PASS, 7 tests.
- New transition/motion RED: visual-fallback task transition already passed against the prior implementation; the non-vacuous reduced-motion test failed with computed `focus-breathe` until the CSS override and explicit emulation were corrected.
- Static race audit found both flights, bidirectional awaits, task transition routing, the accepted-owner busy guard before token allocation, the retained helper busy guard, active-scene/token auto-enter guards, centralized snapshot acceptance, and no obsolete generation names or hardcoded Task 9 primary ink.

Concurrency reasoning:

- Enter→Escape and enter→return tasks: the exit call sees `immersiveEnterFlight`, awaits it, then exits the final system or fallback state before returning. The task scene changes only after that successful exit.
- Duplicate enter: every caller receives the existing `immersiveEnterFlight`; only its owner calls the Tauri/system or fallback entry.
- Enter during exit: enter awaits the existing exit. A false exit result aborts entry and retains FocusScene/system. After a successful exit it rechecks the captured immersive-intent token, `activeScene === 'focus'`, and display off before one new entry begins, so any newer Escape/task intent wins.
- Duplicate exit: after any enter completes, callers recheck and reuse `immersiveExitFlight`; only one system restore call runs.
- Late auto-start: after the page confirms `pomodoroBusy` is false, the accepted command owner captures an enter-intent token before awaiting the command. Successful command plus refresh is insufficient unless that token is still latest and `activeScene === 'focus'`; Escape or task navigation advances the epoch immediately and invalidates the older auto-entry. A rapid rejected Start never receives a token and cannot cancel the accepted owner's eventual entry.
- Exit during an already-started Tauri enter: the entry flight still records the actual system/fallback result when its call resolves; the waiting exit then unwinds that real state and wins final `off`.
- Floating task intent: awaiting the same scene-change/exit path prevents task activation on exit failure or disposal.

## Deferred Minors

- Tauri-only prompt/start/refresh ordering and system enter/exit failure branches need desktop-capable integration or Windows smoke coverage; browser preview intentionally cannot start the backend-owned timer.
- `FocusScene` retains the baseline-compatible `tasks` plus optional `taskSummaries` alias. The current page uses one canonical `tasks` input, so there is no duplicated live authority; removing the compatibility alias can be a later cleanup when all consumers are tracked together.
- The prompt uses the established ARIA dialog pattern with a narrowly scoped Svelte a11y suppression rather than native `<dialog>`. Its required role, modal semantics, label/description, initial focus, containment, Escape, and restoration behavior are explicit.

## Rulings

- Ruling: adopt the untracked workspace source into a new tracked FocusScene without a Git rename — the source was never tracked, so a rename record would be false — cost if wrong: provenance could be misreported or an untracked duplicate could remain.
- Ruling: manual browser immersive uses `visual-fallback` even though Pomodoro controls are disabled — manual display mode is a UI function independent of backend timer authority — cost if wrong: browser preview could not verify accessible immersive layout and Escape behavior.
- Ruling: command success belongs to command result plus successful refresh — the refreshed backend view remains the final authority before auto-immersive — cost if wrong: immersive could open for a start that the UI failed to reconcile.
- Ruling: keep all secondary focus context in one ContextDrawer — it preserves one task/settings draft and command authority while keeping the stage centered — cost if wrong: duplicated draft state or unclipped document overflow could emerge.
- Ruling: guard pending starts with a current-session transition epoch and captured session identity — same-session refresh is harmless, while any accepted identity transition invalidates the old prompt — cost if wrong: routine polling could cancel valid intent or a stale choice could conflict with another client/session.
- Ruling: combine serialized transitions with a latest-intent epoch owned only by accepted operations — auto-entry checks page busy before capturing intent, the command helper retains its own busy defense, manual entry creates intent only for a new flight, and exit/task navigation invalidates older entry before waiting — cost if wrong: a rejected rapid Start could steal intent ownership and cancel the accepted start, or serialization could replay an older auto-enter after a newer Escape or Return intent.
- Ruling: let an already-started Tauri enter report actual state before exit unwinds it — invalidating intent cannot pretend an external fullscreen call never completed — cost if wrong: exit could observe `off`, skip restoration, and leave the OS window fullscreen.
- Ruling: route every task transition, including floating task intents, through `changeScene('tasks')` — system exit failure must keep FocusScene and prevent activation — cost if wrong: a background intent could bypass the fullscreen restore invariant.
- Ruling: auto-enter only while the focus scene remains active — command/refresh completion does not preserve the user's earlier navigation intent — cost if wrong: a late start response could force immersive mode over the task scene.
- Ruling: prompt Escape stops propagation — the topmost modal must cancel only its pending action while the page handler respects child default prevention — cost if wrong: one Escape would cancel the prompt and exit immersive simultaneously.
- Ruling: preserve the baseline-compatible FocusScene task prop alias for this bounded task — the page still supplies exactly one collection and removing compatibility is not required for behavior — cost if wrong: an unseen prerequisite consumer could fail at compile time, while retaining it modestly broadens the component interface.
- Ruling: verify Tauri-only branches statically and defer live system failure injection — browser preview cannot truthfully emulate Tauri without widening test infrastructure and authority — cost if wrong: a platform-only regression may remain until desktop smoke or dedicated integration coverage.
