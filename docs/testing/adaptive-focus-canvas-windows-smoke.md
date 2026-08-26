# Adaptive Focus Canvas Windows Smoke Test

## Environment

- Evidence revision map: browser baseline and non-vacuous-scroll evidence spans frozen base `6416e303ec5ce37d47e64ce6b9d241bb5932327d` through `911393832edb3dfd4b6940f4d0e587da3649dbad`; normal-placement source is `7ed7c31`, implicit-exit source is `845ae4c`, Pomodoro-warning source is `0219278`, and final warning/manual-resize native evidence used source `426d35b`; corrected native focus-loss plus final immersive/always-on-top binary source is `74386aa`; final Task 12 documentation-only reconciliation head before this correction is `919ef9f`. No production source changed after `74386aa` before this documentation fix.
- Windows version: Windows 11 Pro 10.0.26200 build 26200, 64-bit.
- Display scale: 100% (96 DPI), supplied runtime fact; registry `Win8DpiScaling=0` was observed, while `LogPixels` was unset.
- Monitor layout: one active AOC2702 display, 2560 x 1440 at 180 Hz on NVIDIA GeForce RTX 3060; supplied runtime fact says one active monitor.
- Build command: exact requested `npm run tauri -- build --bundles nsis` exposed npm forwarding failure (`tauri build nsis`, exit 1); correctly forwarded `npm run tauri -- build -- --bundles nsis` exited 0 and produced the NSIS bundle.
- Test date: 2026-08-27 (local UTC+08:00); earlier browser verification began before the local date rollover.
- Safety boundary and final restoration: isolated native runs used alternate identifiers ending `smoke20260827`, `green20260827`, and `final20260827`; the real StarToDo installation and real roaming/local data stayed untouched. Isolated startup temporarily replaced live `startodo:` after exact backup SHA-256 `813FCC1060251105B4852B8C34B0C09100EA42BE440DDEF80CB1693761AEDE03`; final cleanup deleted the isolated key, imported the original, and verified its command and DefaultIcon. Verification accidentally recreated disposable Toolkit registration once; exact-path uninstall removed it. Final registry/profile/temp/sidecar restoration checks are recorded below.

## Main window

- [x] First launch opens maximized inside the Windows work area and keeps the taskbar visible. Observed isolated native evidence: first disposable launch was maximized, non-fullscreen, with inner 2560 x 1369 inside the 2560 x 1392 Windows work area; the taskbar remained visible.
- [x] Restored normal window cannot resize below 520 x 420. Observed isolated native evidence: unmaximize restored exact inner 960 x 680 at x=260,y=260, and a real user bottom-right drag below the minimum clamped to exact inner 520 x 420.
- [x] The UI remains usable at 520 x 420. Observed evidence: Chromium E2E at 520 x 420 kept task and focus document width/height at 520 x 420, opened the planner drawer, exercised its visible local tabpanel, restored settings-trigger focus after Escape, and displayed the focus timer with no browser errors.
- [x] Normal size and position survive hide-to-tray, show, UI release/rebuild, and process restart. Observed isolated native evidence: exact inner 960 x 680 at x=260,y=260 survived hide/show, Release UI/WebView rebuild, and a full process restart; Release retained original PID 139084, second PID 137180 exited 0, and rebuilt bounds remained exact.
- [ ] Legacy compact preferences migrate without recreating compact mode. Blocker: Rust tests cover migration, but no isolated native legacy preference directory was launched.
- [x] Always-on-top still persists and applies. Observed native GREEN on binary `C2FB676…`: setting true produced persisted alwaysOnTop=true and actual native is_always_on_top=true in PID 135584; forced process restart to PID 128980 preserved both true. Controller then set false and verified actual state and preferences both false for cleanup.

## Focus and immersive mode

- [x] Starting a focus phase with preference unset asks once. Observed isolated native evidence: an unset auto-immersive preference produced the choice prompt before starting focus.
- [x] “进入并记住” starts focus and enters true fullscreen. Observed isolated native evidence: the action persisted enabled, started a running Pomodoro, and entered Windows true fullscreen.
- [x] “保持窗口模式” starts focus without fullscreen. Observed isolated native evidence: the action persisted disabled and started a running Pomodoro with fullscreen=false.
- [x] The diagnostics preference can restore “下次询问”. Observed isolated native evidence: diagnostics changed the persisted preference from disabled back to unset.
- [x] Escape exits fullscreen. Observed isolated native evidence: Escape exited Windows true fullscreen and restored the exact prior normal bounds.
- [x] The visible exit action exits fullscreen. Observed native GREEN on binary `C2FB676…`: entered state was maximized=true/fullscreen=true/normal 960 x 680; activating visible `退出沉浸` returned UI display state off and native state maximized=true/fullscreen=false/normal 960 x 680.
- [x] Exiting restores the exact prior maximized or normal state. Observed native evidence covers both branches: earlier Escape restored exact prior normal bounds, while visible `退出沉浸` restored maximized=true from true fullscreen and retained normal 960 x 680.
- [ ] A fullscreen command failure uses in-window immersive fallback without changing Pomodoro state. Blocker: browser runtime absence selects visual fallback before `enterImmersiveMode` is invoked, so current E2E does not exercise a rejected `enter_immersive_mode` command or compare a live Pomodoro snapshot before/after; no isolated native failure injection was available.
- [x] Pause, resume, skip, reset, notification warning, and SQLite restoration still work. Observed isolated native evidence: start, pause, resume, skip, restart, and reset all mutated isolated SQLite and returned explicit absent-host warnings; running session id 5 survived a forced full process restart and was then reset.

## Task scene

- [ ] Search, execution filter, status filter, and project selection survive scene changes. Blocker: browser preview has no persisted task/project dataset and no isolated native dataset was launched.
- [ ] Quick capture and partial batch retry still work. Partial native PASS: quick capture created `Task 12 native smoke`. Blocker: partial batch retry was not exercised, so the combined row remains unchecked.
- [ ] Detailed task creation preserves project, reminder, priority, and recurrence fields. Blocker: requires isolated native database mutation.
- [ ] Update, complete, restore, snooze, defer, delete, trash restore, and permanent delete work. Blocker: requires isolated native database mutation.
- [ ] Stale task command results do not overwrite newer UI state. Blocker: unit coverage passed, but no native race was safely exercised.
- [x] Completing a task restores focus to an adjacent task or search. Observed isolated native evidence: completing `Task 12 native smoke` moved focus to `#task-search`; restoring the task then succeeded.
- [x] Completion shows the non-blocking star feedback. Observed isolated native evidence: completing `Task 12 native smoke` displayed the star feedback without blocking the task scene.
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
- [x] Expansion collapses after the five-second interaction window and leave buffer. Observed native GREEN at source `74386aa`: running session id 5 started exact capsule 340 x 64/userResized=false; keyboard focus expanded to interaction-expanded inner 360 x 260/prefs expanded, real `打开专注工作区` routed backend intent and focused the main Focus scene, and after 5,800 ms floating returned to exact capsule inner 340 x 64/prefs capsule,userResized=false. Clock-controlled Chromium coverage separately proves the boundary timing.
- [x] Keyboard focus inside prevents collapse. Observed browser evidence: keyboard focus remains interaction-expanded past 5,700 ms while ownership remains inside; targeted native-event coverage retains DOM activeElement and releases ownership on OS focus loss. Controller native GREEN at source `74386aa` proves real two-WebView focus transfer now collapses after the bounded interval. Failed logical-focus restoration remains GREEN.
- [x] “始终展开” persists across floating-window recreation and process restart. Observed isolated native evidence: always-expanded remained enabled through floating hide/show and a full process restart.
- [x] Manual resize is not overwritten by later display-state changes. Observed native evidence: controller dragged the active expanded floating window from 360 x 260 to inner 480 x 350; backend preferences became width=480,height=350,userResized=true, and a subsequent display-mode command preserved 480 x 350.
- [x] “恢复自动尺寸” restores the recommended size. Observed native GREEN at source head `426d35b`: in the same mounted floating WebView, real drag changed inner 360 x 260 to 480 x 350 and backend preferences became width=480,height=350,userResized=true; running-focus collapse preserved 480 x 350, capsule focus expanded with reset visible without reload, keyboard activation restored exact inner 360 x 260 with userResized=false/displayMode=expanded, and reset button count became 0.
- [x] Floating task and focus intents open the correct main-window scene. Observed isolated native evidence: task intent opened the Task scene and focused the intended task action; focus intent opened the Focus scene.
- [x] Closing the floating window hides it instead of exiting the app. Observed isolated native evidence: real WM_CLOSE hid the floating window, persisted visible=false, and left the process alive.

## Tray, notification, and installer

- [x] Tray Show works. Observed isolated native UI Automation evidence: left-invoking the unnamed notification-area icon showed the main window; the native `#32768` menu exposed labels exactly `Show`, `Focus`, `悬浮窗`, `Hide`, `Release UI`, and `Quit`.
- [x] Tray Focus queues the focus scene intent. Observed isolated native evidence: invoking `Focus` showed the Focus scene.
- [x] Tray 悬浮窗 toggles the floating window. Observed isolated native evidence: invoking `悬浮窗` hid and then showed the floating window.
- [x] Tray Hide works. Observed isolated native evidence: invoking `Hide` hid the main window.
- [x] Tray Release UI destroys and safely recreates the main UI. Observed evidence: latest isolated native rebuild closed CDP on Release UI while original PID 139084 stayed alive; launching the same binary produced second PID 137180 which exited 0, and the WebView rebuilt inside original PID 139084 with exact inner 960 x 680 bounds. Installed app/data remained untouched.
- [x] Tray Quit exits the process. Observed isolated native evidence: invoking `Quit` terminated the process and its CDP endpoint.
- [ ] Task notification activation works. Blocker: notification registration and real user data could not be mutated safely.
- [ ] Pomodoro notification activation works. Blocker: notification registration and native SQLite state could not be mutated safely.
- [ ] Installed-app protocol activation works. Blocker: the real `startodo:` command points to `D:\Code\Rust\StarToDo\src-tauri\target\debug\startodo.exe`; launching it would be outside this worktree and could mutate real user data/registration.
- [ ] Uninstall removes the app and protocol registration cleanly. Blocker: no disposable Windows Sandbox/VM exists, and uninstalling the real installation is explicitly prohibited.

## Isolated native fix validation and current warning finding

- Controller safety incident and final GREEN: isolated runs used disposable identifiers ending `smoke20260827`, `green20260827`, and `final20260827`; installed app/real data stayed untouched. Startup temporarily changed shared `startodo:` after exact backup, and verification accidentally recreated the disposable Toolkit AUMID/CLSID/icon once. Final cleanup used backup SHA-256 `813FCC1060251105B4852B8C34B0C09100EA42BE440DDEF80CB1693761AEDE03` to delete the isolated protocol key and import the original; command and DefaultIcon matched. Exact-path Toolkit cleanup, profile removal, and artifact absence checks below completed machine restoration.
- Native RED: first launch reported preferences `normalBounds=960x680`, `maximized=true`; actual maximized inner size was 2560 x 1369, `fullscreen=false`, with 2560 x 1392 work area/taskbar visible. Real `set_main_window_maximized(false)` plus 500 ms returned `maximized=false`, but actual inner remained 2560 x 1369 and outer became 2576 x 1408 at x=286,y=286; tracking then persisted this monitor-sized off-screen normal bound.
- Normal-placement native GREEN: latest isolated build started with preferences maximized=true/normal 960 x 680; real unmaximize restored exact inner 960 x 680 at x=260,y=260, not the previous 2560 x 1369 placement.
- UI Release native GREEN: Release closed CDP but original PID 139084 remained alive; second instance PID 137180 exited 0; WebView rebuilt in PID 139084 with exact 960 x 680 bounds.
- Third native RED/GREEN — Pomodoro warning dismissal: initial host-absent native run left `关闭通知` inert for 120 seconds. Controller retest at source head `426d35b`, binary SHA-256 `A88473D649D8B16DC32ADF37FE3099474EE650625A638D2E8C1623BA12FBC283`, again with sidecar absent, confirmed the Pomodoro toast detached after close; only the independent reminder-reconciliation toast remained. Distinct warning B reappearance is browser-E2E evidence only: host-absent native mutations all early-return the same inventory warning, so no native distinct-B sequence is claimed.
- Fourth native RED/failed-first-fix/GREEN — floating WebView deactivation: commit `e94fc8c` failed because WebView2 delivered no browser blur and remained expanded after 5,800 ms. Controller rebuilt production source `74386aa` with disposable config `final20260827`; binary SHA-256 `C2FB676B042AB750132517DFF22DE4EFEA298C523D89BC55E7F3D805A6D03CF9`, PID 135584, isolated DB verified, both host paths absent. Running session id 5 produced capsule 340 x 64/userResized=false; keyboard expansion produced inner 360 x 260/prefs expanded; real expanded `打开专注工作区` routed backend intent/native main focus and showed the main Focus scene; after 5,800 ms floating was exact capsule inner 340 x 64/prefs capsule,userResized=false. This is native GREEN for the `WindowEvent::Focused(false)` replacement.
- Fifth native RED/GREEN — live manual-resize ownership: initial native run persisted 480 x 350/userResized=true but required WebView reload to surface reset. Controller retest at source head `426d35b` confirmed the same mounted WebView learned real drag ownership: running-focus collapse preserved 480 x 350, focusing the capsule showed reset without reload, and keyboard reset returned exact 360 x 260 with userResized=false/displayMode=expanded and no reset button. This is native GREEN for manual preservation, live affordance, and reset.

## Read-only native and installer observations

- The stopped installed binary `D:\Program Files\StarToDo\StarToDo.exe` exists, reports file version 0.1.2, and is 14,196,224 bytes. This does not count as launch/upgrade smoke.
- `%APPDATA%\com.aidotnet.startodo` and `%LOCALAPPDATA%\com.aidotnet.startodo` both exist; they were not opened or mutated.
- Final protocol GREEN: with no StarToDo process or CDP endpoint, controller used exact backup SHA-256 `813FCC1060251105B4852B8C34B0C09100EA42BE440DDEF80CB1693761AEDE03`, deleted isolated `HKCU\Software\Classes\startodo`, imported the original, and verified command `"D:\Code\Rust\StarToDo\src-tauri\target\debug\startodo.exe" "%1"` plus matching DefaultIcon. Only two exact worktree MuiCache values were deleted; recursive registry search for `StarToDo-adaptive-focus-canvas-phase1` returned 0.
- Final disposable-state GREEN: smoke CLSID `{eb4967e6-7bd1-152c-afb3-1fad44bf1d25}` and worktree AUMID were absent. Exact roaming and local profiles for smoke/green/final identities—six directories—were deleted and verified absent.
- Final sidecar/artifact GREEN: one canonical generated notification host was restored at `tools\notification-host\publish` and `src-tauri\target\debug`, both SHA-256 `443F2C9867D955EC8ACE6F3B787741B346AA66449C915EF9500498949468FAFB`; all `.native-smoke-*` duplicates were removed. Exact temporary config, driver, tray, resize, screenshot, `.reg`, and cleanup-project assets were deleted; glob search found no native-smoke files or backups.
- Real `%APPDATA%\com.aidotnet.startodo`, `%LOCALAPPDATA%\com.aidotnet.startodo`, and installed `D:\Program Files\StarToDo\StarToDo.exe` still exist and remained untouched throughout.
