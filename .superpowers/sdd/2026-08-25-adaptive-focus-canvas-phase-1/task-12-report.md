# Task 12 Implementation Report

## Scope and base

- Worktree: `D:\Code\Rust\StarToDo-adaptive-focus-canvas-phase1`.
- Branch: `adaptive-focus-canvas-phase1`.
- Frozen accepted base verified before edits: `6416e303ec5ce37d47e64ce6b9d241bb5932327d`.
- Owned implementation paths: `e2e/preview.spec.ts`, `docs/testing/adaptive-focus-canvas-windows-smoke.md`, this report, and the Task 12 progress-ledger append. No production source fix was required.
- Preserved unrelated dirt includes the user-named `src-tauri/src/pomodoro.rs`, `$null`, package/Rust/capability/component/notification-host/config files, and all other pre-existing modified/untracked paths.

## Implementation evidence

- Added the binding task/focus viewport matrix: 320 x 720, 520 x 420, 760 x 560, 1180 x 760, and 1440 x 900. Every case asserts both document width and document height are at most the viewport, switches to focus, asserts the timer, and repeats both containment assertions. Existing stronger focus-stage geometry tests in `e2e/responsive.spec.ts` were retained unchanged.
- Added 520 x 420 local-scroll evidence: opens the real planner drawer, requires its visible selected-bucket tabpanel, requires local `overflow-y: auto|scroll`, verifies document scroll top/left are exactly zero, audits every visible `[data-scroll-region]`, temporarily appends inert overflow content inside that real production tabpanel, proves `scrollHeight > clientHeight` and `scrollTop > 0`, removes the content, and confirms document scroll remains zero.
- Added reduced-motion + keyboard evidence: explicitly emulates reduced motion and proves the media query matches, focuses the settings trigger, opens the drawer, presses Escape, proves trigger focus restoration, switches to focus, and proves the timer is visible.
- Existing per-test browser-error hooks remain binding for all added cases. Browser preview/Tauri fallback behavior was preserved.

## RED/GREEN record

- Characterization evidence: the first run of all five viewport matrix tests passed immediately (5/5). This is existing-behavior evidence, not a RED and not represented as a product fix.
- Test-fixture RED 1: focused Playwright run exited 1 because `[data-scroll-region="tasks"]` was absent in browser preview. Root cause: `TaskScene.svelte` intentionally mounts `TaskCanvas` only when `tauriAvailable`; the test incorrectly required a Tauri-only task list in browser preview. Minimal test correction: exercise the visible planner tabpanel, a real local-scroll region available in browser preview. No production change.
- Test-fixture RED 2: the same run exited 1 because describe-level `test.use({ reducedMotion: 'reduce' })` left `matchMedia('(prefers-reduced-motion: reduce)').matches` false in this installed Playwright environment. Existing working coverage uses `page.emulateMedia`. Minimal test correction: call `page.emulateMedia({ reducedMotion: 'reduce' })` before navigation and retain the strict matchMedia assertion. No production change.
- GREEN: exact focused command `npx playwright test e2e/preview.spec.ts --grep "task and focus scenes fit|scrolling is confined|keeps drawer and scene operations keyboard usable"` exited 0 with 7/7 passed in 11.2 s.
- Full GREEN: `npm run test:e2e` exited 0 with 29/29 passed in 20.4 s; later `npm run verify` repeated 29/29 passed in 16.6 s.
- Controller-review RED: after strengthening the local-scroll test, the unchanged planner fixture produced `scrollHeight = clientHeight = 153` and exit 1 at `expect(scrollHeight).toBeGreaterThan(clientHeight)`, proving the prior `scrollTop` assignment could remain zero. An initial attempt was invalid environmental evidence because stale Vite optimized dependencies returned HTTP 504; the unchanged rerun reached the expected behavioral assertion.
- Important controller finding — local-scroll evidence: the original claim was false because diagnostics measured `scrollHeight = 153`, `clientHeight = 153`, local `scrollTop = 0`, and document top `0`. Strict RED exited 1 at `expect(scrollHeight).toBeGreaterThan(clientHeight)`; an earlier stale-Vite HTTP 504 run was discarded as invalid RED evidence. GREEN appends temporary `aria-hidden` overflow content inside the real planner tabpanel, sets `scrollTop = 1`, captures strict overflow/scroll measurements, removes the content, and asserts document scroll remains zero; exact focused command passed 1/1 in 2.4 s. No production hook/source change was added, and existing matrix/geometry coverage was retained.
- Important review finding — focus-owned expansion: existing coverage proved logical focus restoration and a separate ownership-free deadline, not that focused content prevents collapse. Initial focused RED exited 1 after `body.focus()` failed to move focus genuinely outside and the companion correctly remained `interaction-expanded`; this identified the test’s false focus-out action rather than a production defect. GREEN creates/focuses a genuine outside button: keyboard expansion keeps the matching expanded target focused and the companion expanded through 5,700 ms, then real focus-out plus 700 ms yields capsule; focused command passed 1/1 in 8.1 s with browser-error hooks active.
- Important review finding — fullscreen failure smoke: browser runtime absence branches to visual fallback before invoking `enterImmersiveMode`, so it cannot prove rejection handling or Pomodoro immutability. The smoke row is now unchecked with that precise blocker; no failure-path success is claimed.

## Exact gates

- `npm run check`: exit 0; `svelte-check found 0 errors and 0 warnings`.
- `npm run test:unit`: exit 0; 9 files, 85/85 tests passed.
- `npm run build`: exit 0; Vite transformed 196 SSR and 209 client modules; static adapter wrote `build`.
- `npm run test:e2e`: exit 0; 29/29 Chromium tests passed.
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`: exit 0; no output.
- `cargo test --manifest-path src-tauri/Cargo.toml`: exit 0; 84/84 library tests passed, 0 main tests, 0 doc tests. Informational MSVC linker import-library warning only.
- `npm run build:notification-host`: exit 0; self-contained win-x64 publish completed to the ignored publish directory.
- `npm run verify`: exit 0; check 0/0 diagnostics, unit 85/85, sidecar publish, frontend build, format check, Rust 84/84 + 0 + 0, Playwright 29/29. Informational warnings: MSVC linker stdout and `NO_COLOR` ignored because `FORCE_COLOR` is set.

## Installer

- Exact required command `npm run tauri -- build --bundles nsis`: exit 1 after successful sidecar/frontend pre-build because npm forwarded it as `tauri build nsis`; Tauri/Cargo reported `unexpected argument 'nsis' found`. This is argument forwarding in the existing npm/Tauri command surface, not a Task 12 product/layout defect.
- Equivalent correctly forwarded command `npm run tauri -- build -- --bundles nsis`: exit 0; release profile completed in 3m 27s and NSIS produced one bundle.
- Installer: `D:\Code\Rust\StarToDo-adaptive-focus-canvas-phase1\src-tauri\target\release\bundle\nsis\StarToDo_0.1.2_x64-setup.exe`; 29,096,631 bytes; SHA-256 `20C49C298CFD826A640985EEBEFC01C1E44874E15E27A89DB0DCD7F3034FCD77`; Authenticode status `NotSigned`.
- The installer was inspected read-only and was not launched. No installer or generated artifact is staged or intended for commit.

## Native smoke Important — pending controller GREEN

- Controller isolation/safety: debug build used disposable identifier `com.aidotnet.startodo.adaptivefocuscanvas.smoke20260827`; both generated notification hosts were disabled; runtime DB path was verified below that identifier before interaction; installed app and real data stayed untouched; live protocol registration was backed up for exact restoration.
- Exact native RED: preferences reported `normalBounds=960x680`, `maximized=true`; actual maximized inner was 2560 x 1369, `fullscreen=false`, in a 2560 x 1392 work area with taskbar visible. After real `set_main_window_maximized(false)` and 500 ms, command state was `maximized=false` but actual inner remained 2560 x 1369, outer was 2576 x 1408 at x=286,y=286, and tracking persisted that corrupt monitor-sized normal bound.
- Root cause: `apply_window_preferences` seeded normal size/position only when `preferences.maximized` was false. A config-created maximized window therefore never received stored/default restore placement; Windows unmaximize reused maximized-sized placement, and geometry tracking treated it as normal.
- TDD RED substrate: first focused Rust attempt exited 1 before compile because ignored notification-host publish input was absent; after rebuilding that required substrate, valid RED exited 1 with E0425/E0433 for missing `main_window_placement_plan` and `MainWindowPlacementStep` (10 errors).
- Implementation/pure GREEN: startup placement is now planned as `Unmaximize -> SetNormalSize -> SetNormalPosition -> Maximize` for maximized preference and the same first three steps followed by `LeaveNormal` otherwise. Geometry tracking is installed only after preference application in both initial setup and UI rebuild, so intermediate placement events are not persisted. Focused Rust passed 1/1; full Rust passed 85/85 plus 0 main/0 docs; fmt-check passed; frontend check reported 0 errors/0 warnings.
- Native status: not GREEN. The source commit requires controller rebuild and rerun of the real isolated binary. No restored-normal, restart, or geometry smoke row is checked from pure/unit evidence.

## Windows smoke

- Environment observed: Windows 11 Pro 10.0.26200 build 26200, 64-bit; one active 2560 x 1440 AOC display at 180 Hz and 100%/96 DPI; NVIDIA RTX 3060.
- Read-only installed-app evidence: stopped `D:\Program Files\StarToDo\StarToDo.exe`, version 0.1.2, size 14,196,224 bytes; real roaming and local user-data directories exist.
- Read-only protocol evidence: `startodo:` points to `"D:\Code\Rust\StarToDo\src-tauri\target\debug\startodo.exe" "%1"`.
- Checked checklist items are limited to executable browser-observable UI/fallback/planner/floating behavior. Every checked item has concrete evidence in `docs/testing/adaptive-focus-canvas-windows-smoke.md`.
- Installed-app launch, protocol activation, notifications, native window/tray/fullscreen, installer execution, upgrade, and uninstall remain unchecked. No disposable Sandbox/VM exists; startup calls `register_all` and uses the live identifier/data authority. The installed app/user data/registration were not mutated.

## Rulings

- Ruling: Treat the five matrix cases as characterization because they passed on their first execution — honest evidence is stronger than inventing RED — cost if wrong: the report would overstate implementation work, but no product behavior changed.
- Ruling: Exercise the planner tabpanel for browser local-scroll evidence instead of fabricating Tauri task data — it is a real user-visible drawer region available at 520 x 420 and avoids production test hooks — cost if wrong: task-body overflow remains covered by Tauri-path/native work only, while the required real planner local region remains binding here.
- Ruling: Add temporary inert overflow content through Playwright to the mounted production planner tabpanel rather than add fixture-only production data/hooks — the assertion tests the real element's overflow ownership and requires actual local displacement while leaving application authorities untouched — cost if wrong: CSS depending specifically on organic planner-child structure might not be covered, but the local scrolling contract itself is directly executable and non-vacuous.
- Ruling: Explicitly call `page.emulateMedia` because this installed Playwright did not activate reduced motion through describe-level `test.use` — the matchMedia assertion proves the actual runtime state — cost if wrong: a future Playwright version may make the explicit call redundant, but behavior remains correct.
- Ruling: Do not create or launch an alternate native config — exact isolation would need coordinated identifier, app-data, notification, single-instance, and `startodo:` authorities, while startup calls `register_all`; the safety boundary forbids risking live user state — cost if wrong: fewer native smoke items are checked, but no user installation or registration is mutated.
- Ruling: Browser E2E evidence may check UI usability, visual fallback, planner, and floating-state behavior, but may not be labeled Windows-native WebView2/installed-app evidence — this prevents overclaiming — cost if wrong: the checklist is conservative and leaves more items unchecked.
- Ruling: Preserve the failing exact NSIS command as required evidence and use the additional npm-forwarding form only to test the actual installer build — the first failure occurs before Cargo build due to CLI parsing — cost if wrong: downstream automation that uses the exact script spelling remains broken and must be corrected outside this task.
- Ruling: Mark fullscreen-command failure unchecked rather than build a broad main-window Tauri mock during this fix round — runtime absence bypasses the rejection path, and a faithful Pomodoro/Tauri integration would require substantially more command authority than the focused floating fixture — cost if wrong: rejection fallback remains without executable browser integration and must be verified in later safe native or dedicated integration work.
- Ruling: A genuine outside focus target is required to prove focus-out; `body.focus()` is not sufficient when body is not focusable — the browser’s active element is asserted outside the floating region before advancing the leave buffer — cost if wrong: unusual browser focus behavior could still require a dedicated focusable page fixture, but Chromium evidence is deterministic here.
- Ruling: Seed normal placement while the configured window is still hidden, then apply final maximize before installing geometry tracking — this gives Windows a restore placement without allowing temporary unmaximize/resize/move events to overwrite preferences — cost if wrong: platform event delivery delayed beyond listener installation could still expose an intermediate event, which is why controller native rerun remains mandatory.
- Ruling: Use an explicit pure placement plan as the smallest deterministic Rust regression — Tauri `WebviewWindow` is not cheaply mockable, while the ordered OS calls are the defect boundary — cost if wrong: the pure test cannot prove Windows honors the calls, so native GREEN is intentionally withheld.

## Deferred Minors and blockers

- Deferred inherited Task 7 Minor: component-boundary coverage for normal row Space, trash row Space, and nested-button Space was not added because browser preview has no mutable task rows and Task 12 ownership is viewport/smoke verification.
- Deferred inherited Task 8 Minors: Arrow/Home/End tab navigation and literal sequential Tab/Space E2E depth remain unchanged; existing reachable-tab/Enter coverage passed.
- Deferred inherited Task 11 Minors: expanded task labels use an independent `Date.now()`; reset success generic cast was already corrected before the frozen base. No Task 12 source change was justified.
- Native smoke blocker: no disposable isolated Windows environment; installed app, real user data, protocol, and notification registration must remain untouched.

## Scoped status

- Tested source/evidence range before fix-round-1 documentation: `6416e303ec5ce37d47e64ce6b9d241bb5932327d..911393832edb3dfd4b6940f4d0e587da3649dbad`, containing initial Task 12 evidence commit `439473866cf4bfa11bcef5c677ff5f61a99ec5b0` and non-vacuous scroll follow-up `911393832edb3dfd4b6940f4d0e587da3649dbad`. The later fix-round-1 commit contains the new focus-ownership test and these evidence corrections; no self-referential hash is claimed here.
- Task 12 changes through fix round 1 are E2E verification and documentation/report/ledger evidence only; no production source defect was observed.
- Fix-round-1 verification before commit: focused floating E2E 3/3 in 9.0 s; full Playwright 30/30 in 17.8 s; `npm run check` 0 errors/0 warnings. A parallel focused invocation exited 1 only because the simultaneous full suite owned port 1420; the isolated rerun provides the valid behavioral evidence.
- UTF-8/nonempty audit passed for preview, responsive, smoke, report, and ledger files. Smoke checklist totals after removing the fullscreen overclaim: 9 checked, 39 unchecked, and all 39 unchecked rows contain explicit blockers. Native evidence remains unchanged pending separate controller-provided native smoke; no browser result is promoted to Windows-native evidence.
