# Task 9 Report

Status: implemented, verified, independently reviewed, and ready for the scoped Task 9 commit.

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

## Deferred Minors

- Tauri-only prompt/start/refresh ordering and system enter/exit failure branches need desktop-capable integration or Windows smoke coverage; browser preview intentionally cannot start the backend-owned timer.
- `FocusScene` retains the baseline-compatible `tasks` plus optional `taskSummaries` alias. The current page uses one canonical `tasks` input, so there is no duplicated live authority; removing the compatibility alias can be a later cleanup when all consumers are tracked together.
- The prompt uses the established ARIA dialog pattern with a narrowly scoped Svelte a11y suppression rather than native `<dialog>`. Its required role, modal semantics, label/description, initial focus, containment, Escape, and restoration behavior are explicit.

## Rulings

- Ruling: adopt the untracked workspace source into a new tracked FocusScene without a Git rename — the source was never tracked, so a rename record would be false — cost if wrong: provenance could be misreported or an untracked duplicate could remain.
- Ruling: manual browser immersive uses `visual-fallback` even though Pomodoro controls are disabled — manual display mode is a UI function independent of backend timer authority — cost if wrong: browser preview could not verify accessible immersive layout and Escape behavior.
- Ruling: command success belongs to command result plus successful refresh — the refreshed backend view remains the final authority before auto-immersive — cost if wrong: immersive could open for a start that the UI failed to reconcile.
- Ruling: keep all secondary focus context in one ContextDrawer — it preserves one task/settings draft and command authority while keeping the stage centered — cost if wrong: duplicated draft state or unclipped document overflow could emerge.
- Ruling: guard pending starts with accepted-view generation and current-session identity — an old prompt must not start after newer backend state is accepted — cost if wrong: a stale choice could double-start or conflict with another client/session.
- Ruling: serialize immersive exit and reuse the same result for scene navigation — concurrent exit requests must not race scene mutation — cost if wrong: tasks could appear even when system fullscreen restoration failed.
- Ruling: prompt Escape stops propagation — the topmost modal must cancel only its pending action while the page handler respects child default prevention — cost if wrong: one Escape would cancel the prompt and exit immersive simultaneously.
- Ruling: preserve the baseline-compatible FocusScene task prop alias for this bounded task — the page still supplies exactly one collection and removing compatibility is not required for behavior — cost if wrong: an unseen prerequisite consumer could fail at compile time, while retaining it modestly broadens the component interface.
- Ruling: verify Tauri-only branches statically and defer live system failure injection — browser preview cannot truthfully emulate Tauri without widening test infrastructure and authority — cost if wrong: a platform-only regression may remain until desktop smoke or dedicated integration coverage.
