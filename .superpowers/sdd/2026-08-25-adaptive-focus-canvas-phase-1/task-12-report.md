# Task 12 Implementation Report

## Scope and base

- Worktree: `D:\Code\Rust\StarToDo-adaptive-focus-canvas-phase1`.
- Branch: `adaptive-focus-canvas-phase1`.
- Frozen accepted base verified before edits: `6416e303ec5ce37d47e64ce6b9d241bb5932327d`.
- Owned implementation paths: `e2e/preview.spec.ts`, `docs/testing/adaptive-focus-canvas-windows-smoke.md`, this report, and the Task 12 progress-ledger append. No production source fix was required.
- Preserved unrelated dirt includes the user-named `src-tauri/src/pomodoro.rs`, `$null`, package/Rust/capability/component/notification-host/config files, and all other pre-existing modified/untracked paths.

## Implementation evidence

- Added the binding task/focus viewport matrix: 320 x 720, 520 x 420, 760 x 560, 1180 x 760, and 1440 x 900. Every case asserts both document width and document height are at most the viewport, switches to focus, asserts the timer, and repeats both containment assertions. Existing stronger focus-stage geometry tests in `e2e/responsive.spec.ts` were retained unchanged.
- Added 520 x 420 local-scroll evidence: opens the real planner drawer, requires its visible selected-bucket tabpanel, requires local `overflow-y: auto|scroll`, verifies document scroll top/left are exactly zero, audits every visible `[data-scroll-region]`, exercises the real local planner region, and confirms document scroll remains zero.
- Added reduced-motion + keyboard evidence: explicitly emulates reduced motion and proves the media query matches, focuses the settings trigger, opens the drawer, presses Escape, proves trigger focus restoration, switches to focus, and proves the timer is visible.
- Existing per-test browser-error hooks remain binding for all added cases. Browser preview/Tauri fallback behavior was preserved.

## RED/GREEN record

- Characterization evidence: the first run of all five viewport matrix tests passed immediately (5/5). This is existing-behavior evidence, not a RED and not represented as a product fix.
- Test-fixture RED 1: focused Playwright run exited 1 because `[data-scroll-region="tasks"]` was absent in browser preview. Root cause: `TaskScene.svelte` intentionally mounts `TaskCanvas` only when `tauriAvailable`; the test incorrectly required a Tauri-only task list in browser preview. Minimal test correction: exercise the visible planner tabpanel, a real local-scroll region available in browser preview. No production change.
- Test-fixture RED 2: the same run exited 1 because describe-level `test.use({ reducedMotion: 'reduce' })` left `matchMedia('(prefers-reduced-motion: reduce)').matches` false in this installed Playwright environment. Existing working coverage uses `page.emulateMedia`. Minimal test correction: call `page.emulateMedia({ reducedMotion: 'reduce' })` before navigation and retain the strict matchMedia assertion. No production change.
- GREEN: exact focused command `npx playwright test e2e/preview.spec.ts --grep "task and focus scenes fit|scrolling is confined|keeps drawer and scene operations keyboard usable"` exited 0 with 7/7 passed in 11.2 s.
- Full GREEN: `npm run test:e2e` exited 0 with 29/29 passed in 20.4 s; later `npm run verify` repeated 29/29 passed in 16.6 s.

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

## Windows smoke

- Environment observed: Windows 11 Pro 10.0.26200 build 26200, 64-bit; one active 2560 x 1440 AOC display at 180 Hz and 100%/96 DPI; NVIDIA RTX 3060.
- Read-only installed-app evidence: stopped `D:\Program Files\StarToDo\StarToDo.exe`, version 0.1.2, size 14,196,224 bytes; real roaming and local user-data directories exist.
- Read-only protocol evidence: `startodo:` points to `"D:\Code\Rust\StarToDo\src-tauri\target\debug\startodo.exe" "%1"`.
- Checked checklist items are limited to executable browser-observable UI/fallback/planner/floating behavior. Every checked item has concrete evidence in `docs/testing/adaptive-focus-canvas-windows-smoke.md`.
- Installed-app launch, protocol activation, notifications, native window/tray/fullscreen, installer execution, upgrade, and uninstall remain unchecked. No disposable Sandbox/VM exists; startup calls `register_all` and uses the live identifier/data authority. The installed app/user data/registration were not mutated.

## Rulings

- Ruling: Treat the five matrix cases as characterization because they passed on their first execution — honest evidence is stronger than inventing RED — cost if wrong: the report would overstate implementation work, but no product behavior changed.
- Ruling: Exercise the planner tabpanel for browser local-scroll evidence instead of fabricating Tauri task data — it is a real user-visible drawer region available at 520 x 420 and avoids production test hooks — cost if wrong: task-body overflow remains covered by Tauri-path/native work only, while the required real planner local region remains binding here.
- Ruling: Explicitly call `page.emulateMedia` because this installed Playwright did not activate reduced motion through describe-level `test.use` — the matchMedia assertion proves the actual runtime state — cost if wrong: a future Playwright version may make the explicit call redundant, but behavior remains correct.
- Ruling: Do not create or launch an alternate native config — exact isolation would need coordinated identifier, app-data, notification, single-instance, and `startodo:` authorities, while startup calls `register_all`; the safety boundary forbids risking live user state — cost if wrong: fewer native smoke items are checked, but no user installation or registration is mutated.
- Ruling: Browser E2E evidence may check UI usability, visual fallback, planner, and floating-state behavior, but may not be labeled Windows-native WebView2/installed-app evidence — this prevents overclaiming — cost if wrong: the checklist is conservative and leaves more items unchecked.
- Ruling: Preserve the failing exact NSIS command as required evidence and use the additional npm-forwarding form only to test the actual installer build — the first failure occurs before Cargo build due to CLI parsing — cost if wrong: downstream automation that uses the exact script spelling remains broken and must be corrected outside this task.

## Deferred Minors and blockers

- Deferred inherited Task 7 Minor: component-boundary coverage for normal row Space, trash row Space, and nested-button Space was not added because browser preview has no mutable task rows and Task 12 ownership is viewport/smoke verification.
- Deferred inherited Task 8 Minors: Arrow/Home/End tab navigation and literal sequential Tab/Space E2E depth remain unchanged; existing reachable-tab/Enter coverage passed.
- Deferred inherited Task 11 Minors: expanded task labels use an independent `Date.now()`; reset success generic cast was already corrected before the frozen base. No Task 12 source change was justified.
- Native smoke blocker: no disposable isolated Windows environment; installed app, real user data, protocol, and notification registration must remain untouched.

## Scoped status

- Task 12 source/test scope currently consists only of E2E verification and documentation/report/ledger evidence; no production defect was observed after correcting test assumptions.
- Final fresh `npm run verify` after the last test edit exited 0: diagnostics 0/0, units 85/85, Rust 84/84 + 0 + 0, Playwright 29/29, sidecar/build/fmt all passed.
- UTF-8/nonempty audit passed for all four Task 12 files. Smoke checklist totals: 10 checked with concrete evidence; 38 unchecked and all 38 include explicit blockers.
- Final diff/staging audit and commit hash are recorded by the closing commit/status evidence.
