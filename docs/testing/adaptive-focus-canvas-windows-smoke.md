# Adaptive Focus Canvas Windows Smoke Test

## Environment

- Tested Task 12 source/evidence range: `6416e303ec5ce37d47e64ce6b9d241bb5932327d..911393832edb3dfd4b6940f4d0e587da3649dbad` (initial evidence `439473866cf4bfa11bcef5c677ff5f61a99ec5b0`, non-vacuous scroll follow-up `911393832edb3dfd4b6940f4d0e587da3649dbad`); fix-round-1 test/doc changes are recorded by the later commit containing this document.
- Windows version: Windows 11 Pro 10.0.26200 build 26200, 64-bit.
- Display scale: 100% (96 DPI), supplied runtime fact; registry `Win8DpiScaling=0` was observed, while `LogPixels` was unset.
- Monitor layout: one active AOC2702 display, 2560 x 1440 at 180 Hz on NVIDIA GeForce RTX 3060; supplied runtime fact says one active monitor.
- Build command: exact requested `npm run tauri -- build --bundles nsis` exposed npm forwarding failure (`tauri build nsis`, exit 1); correctly forwarded `npm run tauri -- build -- --bundles nsis` exited 0 and produced the NSIS bundle.
- Test date: 2026-08-27 (local UTC+08:00); earlier browser verification began before the local date rollover.
- Safety boundary: isolated native runs used alternate identifiers ending `smoke20260827`, `green20260827`, and `final20260827`; the real StarToDo 0.1.2 installation and real roaming/local app data stayed untouched. Isolated startup did temporarily replace live `startodo:` registration after an exact `.reg` backup; final byte-equivalent restoration remains pending controller evidence. Verification accidentally recreated the disposable worktree Toolkit AUMID/CLSID/icon once; exact-path Toolkit Uninstall removed them and registry/icon absence was verified. Do not treat cleanup as final until protocol restoration evidence arrives.

## Main window

- [x] First launch opens maximized inside the Windows work area and keeps the taskbar visible. Observed isolated native evidence: first disposable launch was maximized, non-fullscreen, with inner 2560 x 1369 inside the 2560 x 1392 Windows work area; the taskbar remained visible.
- [x] Restored normal window cannot resize below 520 x 420. Observed isolated native evidence: unmaximize restored exact inner 960 x 680 at x=260,y=260, and a real user bottom-right drag below the minimum clamped to exact inner 520 x 420.
- [x] The UI remains usable at 520 x 420. Observed evidence: Chromium E2E at 520 x 420 kept task and focus document width/height at 520 x 420, opened the planner drawer, exercised its visible local tabpanel, restored settings-trigger focus after Escape, and displayed the focus timer with no browser errors.
- [x] Normal size and position survive hide-to-tray, show, UI release/rebuild, and process restart. Observed isolated native evidence: exact inner 960 x 680 at x=260,y=260 survived hide/show, Release UI/WebView rebuild, and a full process restart; Release retained original PID 139084, second PID 137180 exited 0, and rebuilt bounds remained exact.
- [ ] Legacy compact preferences migrate without recreating compact mode. Blocker: Rust tests cover migration, but no isolated native legacy preference directory was launched.
- [ ] Always-on-top still persists and applies. Blocker: requires native window and persisted preference mutation.

## Focus and immersive mode

- [x] Starting a focus phase with preference unset asks once. Observed isolated native evidence: an unset auto-immersive preference produced the choice prompt before starting focus.
- [x] “进入并记住” starts focus and enters true fullscreen. Observed isolated native evidence: the action persisted enabled, started a running Pomodoro, and entered Windows true fullscreen.
- [x] “保持窗口模式” starts focus without fullscreen. Observed isolated native evidence: the action persisted disabled and started a running Pomodoro with fullscreen=false.
- [x] The diagnostics preference can restore “下次询问”. Observed isolated native evidence: diagnostics changed the persisted preference from disabled back to unset.
- [x] Escape exits fullscreen. Observed isolated native evidence: Escape exited Windows true fullscreen and restored the exact prior normal bounds.
- [ ] The visible exit action exits fullscreen. Blocker: browser E2E proves visual fallback behavior only.
- [ ] Exiting restores the exact prior maximized or normal state. Partial native PASS: Escape restored exact prior normal bounds. Blocker: restoration of a prior maximized state was not supplied, so every clause is not proved.
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
- [x] Expansion collapses after the five-second interaction window and leave buffer. Observed evidence: a clock-controlled Chromium test kept `interaction-expanded` at 4,999 ms and observed `capsule` exactly at 5,000 ms without focus ownership.
- [x] Keyboard focus inside prevents collapse. Observed browser evidence: deterministic Chromium clock coverage expands by keyboard and remains `interaction-expanded` with the restored control focused after 5,700 ms. A separate targeted Tauri-event regression retains the DOM active element, delivers native floating focus loss, and observes `capsule` after the reducer’s 700 ms leave buffer; failed logical-focus restoration remains GREEN. Actual two-WebView controller retest of the new native event path is pending.
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

- Controller safety evidence: isolated runs used disposable identifiers ending `smoke20260827`, `green20260827`, and `final20260827`; runtime database paths were verified under those identities and installed app/real data stayed untouched. Live `startodo:` registration was backed up exactly before isolated startup temporarily changed it. Verification accidentally recreated the disposable worktree Toolkit AUMID/CLSID/icon once; exact-path Toolkit Uninstall removed them and registry/icon absence was verified. Final protocol byte-equivalent restoration evidence is still pending, so cleanup is not declared complete.
- Native RED: first launch reported preferences `normalBounds=960x680`, `maximized=true`; actual maximized inner size was 2560 x 1369, `fullscreen=false`, with 2560 x 1392 work area/taskbar visible. Real `set_main_window_maximized(false)` plus 500 ms returned `maximized=false`, but actual inner remained 2560 x 1369 and outer became 2576 x 1408 at x=286,y=286; tracking then persisted this monitor-sized off-screen normal bound.
- Normal-placement native GREEN: latest isolated build started with preferences maximized=true/normal 960 x 680; real unmaximize restored exact inner 960 x 680 at x=260,y=260, not the previous 2560 x 1369 placement.
- UI Release native GREEN: Release closed CDP but original PID 139084 remained alive; second instance PID 137180 exited 0; WebView rebuilt in PID 139084 with exact 960 x 680 bounds.
- Third native RED/GREEN — Pomodoro warning dismissal: initial host-absent native run left `关闭通知` inert for 120 seconds. Controller retest at source head `426d35b`, binary SHA-256 `A88473D649D8B16DC32ADF37FE3099474EE650625A638D2E8C1623BA12FBC283`, again with sidecar absent, confirmed the Pomodoro toast detached after close; only the independent reminder-reconciliation toast remained. Distinct warning B reappearance is browser-E2E evidence only: host-absent native mutations all early-return the same inventory warning, so no native distinct-B sequence is claimed.
- Fourth native RED and failed first fix — floating WebView deactivation: initial native sequence left the active 360 x 260 companion expanded after main-WebView focus transfer. Commit `e94fc8c` listened for browser `window.blur`, but controller retest on binary head `426d35b`, disposable identity `final20260827`, PID 140372 proved WebView2 did not deliver it: real `打开专注工作区` successfully focused the main Focus scene, floating pointer was false, retained `隐藏悬浮窗`, and remained expanded after 5,800 ms. The replacement source path emits a targeted Tauri event from native `WindowEvent::Focused(false)`; the mounted floating UI invalidates restoration and dispatches the same focus-out transition. Backend/frontend RED-GREEN is recorded; controller retest remains pending and no native GREEN is claimed.
- Fifth native RED/GREEN — live manual-resize ownership: initial native run persisted 480 x 350/userResized=true but required WebView reload to surface reset. Controller retest at source head `426d35b` confirmed the same mounted WebView learned real drag ownership: running-focus collapse preserved 480 x 350, focusing the capsule showed reset without reload, and keyboard reset returned exact 360 x 260 with userResized=false/displayMode=expanded and no reset button. This is native GREEN for manual preservation, live affordance, and reset.

## Read-only native and installer observations

- The stopped installed binary `D:\Program Files\StarToDo\StarToDo.exe` exists, reports file version 0.1.2, and is 14,196,224 bytes. This does not count as launch/upgrade smoke.
- `%APPDATA%\com.aidotnet.startodo` and `%LOCALAPPDATA%\com.aidotnet.startodo` both exist; they were not opened or mutated.
- Before isolated startup, `HKCU\Software\Classes\startodo\shell\open\command` was read as `"D:\Code\Rust\StarToDo\src-tauri\target\debug\startodo.exe" "%1"` and the full registration was backed up to an exact `.reg` file. Disposable startup later changed the live scheme registration; controller byte-equivalent restoration evidence is pending.
- Alternate application identifiers isolated database/preferences/process state, but `startodo:` remained a shared OS authority because startup calls `register_all`. The disposable notification Toolkit registration accidentally recreated once during verification was removed with its exact-path uninstaller, and its AUMID/CLSID/icon absence was verified. Do not claim final safety cleanup until the protocol backup is restored and verified.
