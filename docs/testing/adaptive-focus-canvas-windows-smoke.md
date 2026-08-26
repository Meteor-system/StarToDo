# Adaptive Focus Canvas Windows Smoke Test

## Environment

- Tested Task 12 source/evidence range: `6416e303ec5ce37d47e64ce6b9d241bb5932327d..911393832edb3dfd4b6940f4d0e587da3649dbad` (initial evidence `439473866cf4bfa11bcef5c677ff5f61a99ec5b0`, non-vacuous scroll follow-up `911393832edb3dfd4b6940f4d0e587da3649dbad`); fix-round-1 test/doc changes are recorded by the later commit containing this document.
- Windows version: Windows 11 Pro 10.0.26200 build 26200, 64-bit.
- Display scale: 100% (96 DPI), supplied runtime fact; registry `Win8DpiScaling=0` was observed, while `LogPixels` was unset.
- Monitor layout: one active AOC2702 display, 2560 x 1440 at 180 Hz on NVIDIA GeForce RTX 3060; supplied runtime fact says one active monitor.
- Build command: exact requested `npm run tauri -- build --bundles nsis` exposed npm forwarding failure (`tauri build nsis`, exit 1); correctly forwarded `npm run tauri -- build -- --bundles nsis` exited 0 and produced the NSIS bundle.
- Test date: 2026-08-27 (local UTC+08:00); earlier browser verification began before the local date rollover.
- Safety boundary: the real StarToDo 0.1.2 installation at `D:\Program Files\StarToDo`, real roaming/local app data, and the existing `startodo:` registration were inspected read-only only. The installed app was not launched, replaced, upgraded, or uninstalled. No protocol or notification registration was altered.

## Main window

- [ ] First launch opens maximized inside the Windows work area and keeps the taskbar visible. Blocker: native launch was unsafe because startup calls `register_all` for the live `startodo:` scheme and the exact installed/user-data isolation boundary was not guaranteed; static config only shows `maximized: true`.
- [ ] Restored normal window cannot resize below 520 x 420. Partial PASS evidence: controller’s latest isolated build started with preferences maximized=true/normal 960 x 680, and real unmaximize restored exact inner 960 x 680 at x=260,y=260 instead of the prior 2560 x 1369 corruption. Blocker: the controller has not yet supplied an actual resize attempt below 520 x 420, so the minimum-size clause remains unchecked.
- [x] The UI remains usable at 520 x 420. Observed evidence: Chromium E2E at 520 x 420 kept task and focus document width/height at 520 x 420, opened the planner drawer, exercised its visible local tabpanel, restored settings-trigger focus after Escape, and displayed the focus timer with no browser errors.
- [ ] Normal size and position survive hide-to-tray, show, UI release/rebuild, and process restart. Partial PASS evidence: controller’s latest isolated build restored exact 960 x 680 at x=260,y=260, Release UI closed CDP while original PID 139084 remained alive, second PID 137180 exited 0, and the WebView rebuilt in PID 139084 with exact 960 x 680 bounds. Blocker: hide/show and a full process-restart persistence cycle have not yet been supplied, so this combined row remains unchecked.
- [ ] Legacy compact preferences migrate without recreating compact mode. Blocker: Rust tests cover migration, but no isolated native legacy preference directory was launched.
- [ ] Always-on-top still persists and applies. Blocker: requires native window and persisted preference mutation.

## Focus and immersive mode

- [ ] Starting a focus phase with preference unset asks once. Blocker: browser preview disables persistence/timer commands; native launch was unsafe.
- [ ] “进入并记住” starts focus and enters true fullscreen. Blocker: requires native fullscreen and preference mutation.
- [ ] “保持窗口模式” starts focus without fullscreen. Blocker: requires native Pomodoro persistence.
- [ ] The diagnostics preference can restore “下次询问”. Blocker: requires persistent native preference mutation.
- [ ] Escape exits fullscreen. Blocker: browser E2E proves Escape exits visual fallback only, not Windows true fullscreen.
- [ ] The visible exit action exits fullscreen. Blocker: browser E2E proves visual fallback behavior only.
- [ ] Exiting restores the exact prior maximized or normal state. Blocker: requires native window state transitions.
- [ ] A fullscreen command failure uses in-window immersive fallback without changing Pomodoro state. Blocker: browser runtime absence selects visual fallback before `enterImmersiveMode` is invoked, so current E2E does not exercise a rejected `enter_immersive_mode` command or compare a live Pomodoro snapshot before/after; no isolated native failure injection was available.
- [ ] Pause, resume, skip, reset, notification warning, and SQLite restoration still work. Blocker: requires isolated native database and notification-host interaction; Rust tests passed but are not native smoke.

## Task scene

- [ ] Search, execution filter, status filter, and project selection survive scene changes. Blocker: browser preview has no persisted task/project dataset and no isolated native dataset was launched.
- [ ] Quick capture and partial batch retry still work. Blocker: browser preview intentionally disables task persistence.
- [ ] Detailed task creation preserves project, reminder, priority, and recurrence fields. Blocker: requires isolated native database mutation.
- [ ] Update, complete, restore, snooze, defer, delete, trash restore, and permanent delete work. Blocker: requires isolated native database mutation.
- [ ] Stale task command results do not overwrite newer UI state. Blocker: unit coverage passed, but no native race was safely exercised.
- [ ] Completing a task restores focus to an adjacent task or search. Blocker: browser preview has no mutable task rows.
- [ ] Completion shows the non-blocking star feedback. Blocker: browser preview has no mutable task rows.
- [ ] Notification activation reveals and focuses the intended task. Blocker: requires notification/protocol registration and native database state.

## Planner

- [x] Wide planner shows unscheduled plus seven day columns. Observed evidence: Chromium at 1180 x 760 showed the wide board region, hid the narrow tablist/tabpanel, measured the planner at 948 x 674, and kept the document at 1180 x 760.
- [x] Narrow planner shows the date strip and one selected bucket. Observed evidence: Chromium at 520 x 420 showed eight reachable date tabs, exactly one selected tab, and one visible selected-bucket tabpanel while the wide board was hidden.
- [ ] Reschedule failures restore the previous select value and show the existing error. Blocker: browser preview has no mutable task data/Tauri command failure path.
- [ ] Planner-to-list navigation closes the drawer and focuses the task. Blocker: browser preview has no task rows.
- [x] Planner scrolling never moves the document. Observed evidence: at 520 x 420 the real visible planner tabpanel had local `overflow-y` auto/scroll; temporary inert overflow content made `scrollHeight > clientHeight`, setting local `scrollTop` produced a value greater than 0, the content was removed, and `document.scrollingElement.scrollTop` remained exactly 0; browser-error hooks stayed empty.

## Floating window

- [x] Idle state opens expanded. Observed evidence: Chromium floating-route E2E at 360 x 260 rendered region `StarToDo 悬浮窗` with `data-display-mode="expanded"`, an accessible “打开专注工作区” action, document 360 x 260, and local body overflow `auto`.
- [x] Running or paused focus defaults to capsule. Observed evidence: the active Tauri browser fixture returned a running focus snapshot and Chromium asserted `data-display-mode="capsule"` at 340 x 64.
- [x] Pointer or keyboard interaction expands the capsule. Observed evidence: keyboard Tab/focus and explicit touch activation each changed the active fixture from `capsule` to `interaction-expanded`.
- [x] Expansion collapses after the five-second interaction window and leave buffer. Observed evidence: a clock-controlled Chromium test kept `interaction-expanded` at 4,999 ms and observed `capsule` exactly at 5,000 ms without focus ownership.
- [x] Keyboard focus inside prevents collapse. Observed browser evidence: deterministic Chromium clock coverage expands by keyboard and remains `interaction-expanded` with the restored control focused after 5,700 ms. A separate targeted Tauri-event regression retains the DOM active element, delivers native floating focus loss, and observes `capsule` after the reducer’s 700 ms leave buffer; failed logical-focus restoration remains GREEN. Actual two-WebView controller retest of the new native event path is pending.
- [ ] “始终展开” persists across floating-window recreation and process restart. Blocker: requires native preference persistence and process recreation.
- [x] Manual resize is not overwritten by later display-state changes. Observed native evidence: controller dragged the active expanded floating window from 360 x 260 to inner 480 x 350; backend preferences became width=480,height=350,userResized=true, and a subsequent display-mode command preserved 480 x 350.
- [ ] “恢复自动尺寸” restores the recommended size. Native RED: backend recorded the 480 x 350 manual ownership, but the mounted frontend retained initialization `userResized=false`; the reset button remained absent for a 30-second wait and appeared only after WebView reload. Source/browser RED-GREEN now adds an authoritative manual-resize event and same-page reset coverage, but controller native click/reset to expected 360 x 260 is pending.
- [ ] Floating task and focus intents open the correct main-window scene. Blocker: requires two native windows and backend intent routing.
- [ ] Closing the floating window hides it instead of exiting the app. Blocker: requires native window close behavior.

## Tray, notification, and installer

- [ ] Tray Show works. Blocker: requires native tray interaction.
- [ ] Tray Focus queues the focus scene intent. Blocker: requires native tray/backend interaction.
- [ ] Tray 悬浮窗 toggles the floating window. Blocker: requires native tray/window interaction.
- [ ] Tray Hide works. Blocker: requires native tray/window interaction.
- [x] Tray Release UI destroys and safely recreates the main UI. Observed evidence: latest isolated native rebuild closed CDP on Release UI while original PID 139084 stayed alive; launching the same binary produced second PID 137180 which exited 0, and the WebView rebuilt inside original PID 139084 with exact inner 960 x 680 bounds. Installed app/data remained untouched.
- [ ] Tray Quit exits the process. Blocker: requires native tray/process interaction.
- [ ] Task notification activation works. Blocker: notification registration and real user data could not be mutated safely.
- [ ] Pomodoro notification activation works. Blocker: notification registration and native SQLite state could not be mutated safely.
- [ ] Installed-app protocol activation works. Blocker: the real `startodo:` command points to `D:\Code\Rust\StarToDo\src-tauri\target\debug\startodo.exe`; launching it would be outside this worktree and could mutate real user data/registration.
- [ ] Uninstall removes the app and protocol registration cleanly. Blocker: no disposable Windows Sandbox/VM exists, and uninstalling the real installation is explicitly prohibited.

## Isolated native fix validation and current warning finding

- Controller safety evidence: debug build used disposable identifier `com.aidotnet.startodo.adaptivefocuscanvas.smoke20260827`, both generated notification hosts were disabled, runtime database path was verified under the disposable identifier before interaction, the installed app/real data were untouched, and the live protocol registration was backed up for exact restoration.
- Native RED: first launch reported preferences `normalBounds=960x680`, `maximized=true`; actual maximized inner size was 2560 x 1369, `fullscreen=false`, with 2560 x 1392 work area/taskbar visible. Real `set_main_window_maximized(false)` plus 500 ms returned `maximized=false`, but actual inner remained 2560 x 1369 and outer became 2576 x 1408 at x=286,y=286; tracking then persisted this monitor-sized off-screen normal bound.
- Normal-placement native GREEN: latest isolated build started with preferences maximized=true/normal 960 x 680; real unmaximize restored exact inner 960 x 680 at x=260,y=260, not the previous 2560 x 1369 placement.
- UI Release native GREEN: Release closed CDP but original PID 139084 remained alive; second instance PID 137180 exited 0; WebView rebuilt in PID 139084 with exact 960 x 680 bounds.
- Third native RED — Pomodoro warning dismissal: with notification host absent, real Pomodoro start succeeded in isolated SQLite and produced `pomodoroWarning`. The visible `关闭通知` control remained inert through repeated activation for 120 seconds, stayed focused, kept the toast overlay present, and prevented diagnostics interaction. The source fix gives each distinct Pomodoro warning its own dismissible toast identity; browser RED/GREEN is recorded, while controller native retest is pending.
- Fourth native RED and failed first fix — floating WebView deactivation: initial native sequence left the active 360 x 260 companion expanded after main-WebView focus transfer. Commit `e94fc8c` listened for browser `window.blur`, but controller retest on binary head `426d35b`, disposable identity `final20260827`, PID 140372 proved WebView2 did not deliver it: real `打开专注工作区` successfully focused the main Focus scene, floating pointer was false, retained `隐藏悬浮窗`, and remained expanded after 5,800 ms. The replacement source path emits a targeted Tauri event from native `WindowEvent::Focused(false)`; the mounted floating UI invalidates restoration and dispatches the same focus-out transition. Backend/frontend RED-GREEN is recorded; controller retest remains pending and no native GREEN is claimed.
- Fifth native RED — live manual-resize ownership: Win32 user drag changed real expanded inner size from 360 x 260 to 480 x 350. Backend preferences correctly became userResized=true and later display-mode synchronization preserved 480 x 350, but the live frontend retained its initial false value because tracking emitted no observation; `恢复自动尺寸` timed out after 30 seconds and appeared only after WebView reload. The source fix emits preferences only for resize events classified as genuinely manual, and the frontend accepts observations only with no size/reset generation pending. Browser proves same-mounted-page appearance, reset to expanded 360 x 260/userResized=false, and disappearance; controller native reset retest is pending.

## Read-only native and installer observations

- The stopped installed binary `D:\Program Files\StarToDo\StarToDo.exe` exists, reports file version 0.1.2, and is 14,196,224 bytes. This does not count as launch/upgrade smoke.
- `%APPDATA%\com.aidotnet.startodo` and `%LOCALAPPDATA%\com.aidotnet.startodo` both exist; they were not opened or mutated.
- `HKCU\Software\Classes\startodo\shell\open\command` was read as `"D:\Code\Rust\StarToDo\src-tauri\target\debug\startodo.exe" "%1"`; the key was not changed.
- The isolated worktree config uses the same live identifier `com.aidotnet.startodo` and scheme `startodo`, and startup calls `app.deep_link().register_all()`. No temporary alternate config was created because safely proving every identifier/data/notification/protocol boundary would require changing multiple runtime authorities outside Task 12.
