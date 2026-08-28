# Task 11 report

## Outcome and review status

Task 11 implements the adaptive floating capsule and expanded companion on frozen base `295785b379614296dd9a26606ac2afba59cbf8e6`.

The controller review-visible base is `7ac46eab029e2d2a947ace255d6af3281fd11c2c`. Controller review result at that point: **FAIL**. The explicit root role was subsequently fixed in `56b4cc1e106fc8b9a47be196e60698ee24c0f188`. Fix round 1/5 addresses the remaining Important findings: explicit activation, first keyboard focus preservation, Pomodoro acceptance/warnings, generation-aware size reconciliation, and defined failed-intent retry.

Changed/authorized Task 11 paths in the final implementation:

- `src/lib/components/FloatingCapsule.svelte`
- `src/lib/components/FloatingExpandedPanel.svelte`
- `src/lib/components/FloatingWindow.svelte`
- `src/lib/floating-display.ts`
- `src/lib/floating-display.test.ts`
- `src/lib/floating-coordinator.ts`
- `src/lib/floating-coordinator.test.ts`
- `src/lib/floating-size-sync.ts`
- `src/lib/floating-size-sync.test.ts`
- `e2e/responsive.spec.ts`
- `.superpowers/sdd/2026-08-25-adaptive-focus-canvas-phase-1/task-11-report.md`

`src/lib/tasks.ts` and `src/routes/+page.svelte` were already correctly migrated and remain unchanged. Unrelated global dirt and generated artifacts were preserved.

## Frozen baseline

Before the first Task 11 edit, owned tracked files matched `task-11-base.txt` exactly:

- `FloatingWindow.svelte`: 15028 bytes, SHA-256 `6680C7AB98669F854E711EB6C063EEB3512F8E1C9F382312FD655C8A30F59F86`
- `tasks.ts`: 17366 bytes, SHA-256 `E6E54086EB3050759057B85EB7526033D85950E7170EA5B72918330EB8258FA1`
- `+page.svelte`: 37430 bytes, SHA-256 `FC7521E0EA32DD2757D87F0A70D34D5A159982B176A175A691D90A24B36F8957`
- `responsive.spec.ts`: 9328 bytes, SHA-256 `4B7C42980451DBAFA8B6802961329E43631D297003411DE0FC794F1948A60949`
- Both visual component create paths and this report were absent.

## Original TDD evidence

The prescribed npm E2E command did not forward grep:

- `npm run test:e2e -- --grep "expanded idle companion"`
- Actual invocation: `playwright test expanded idle companion`
- Result: exit 1, `No tests found`.

Direct genuine original RED:

- `npx playwright test --grep "expanded idle companion"`
- 1 test, 1 failed: expected `data-display-mode="expanded"`, received no attribute.

Direct original GREEN:

- Same direct Playwright grep: 1/1 passed.

## Fix round 1/5 RED and GREEN evidence

### Explicit click/activation

RED before reducer production edit:

- `npx vitest run src/lib/floating-display.test.ts`
- 12 tests: 2 failed, 10 passed.
- Failures: explicit `activate` left active state in `capsule`; activation following pointer expansion retained the old interaction deadline.

GREEN:

- Focused reducer file: 12/12 passed.
- `activate` enters `interaction-expanded`, preserves truthful pointer/focus flags, sets `collapseAt = at + 5000`, and deterministic timeout returns to capsule.

### Real active-capsule keyboard focus and activation

A minimal test-only Tauri IPC/event mock is installed with Playwright `addInitScript`; no production test hook exists.

RED progression:

- Initial active-capsule run: 2/2 failed because child initialization observed the parent `tauriAvailable=false` mount value and never retried when it became true. This exposed a real runtime initialization ordering defect.
- After runtime initialization became prop-reactive, 2/2 still failed at intended behavior: reverse/expand focus remained capsule and explicit click activation remained capsule.

GREEN:

- `npx playwright test --grep "active capsule"`: 2/2 passed.
- Forward Tab preserves open-focus across capsule subtree replacement.
- Direct focus of the reverse-tab destination preserves the logical expand target as the stable expanded hide control.
- Explicit expand-button activation enters interaction-expanded without synthetic pointer/focus ownership.
- A real Playwright mouse `.click()` cannot isolate this path: pointer movement fires the root `pointerenter`, expands the companion, and detaches the capsule button before click delivery. The final E2E therefore enables touch and taps the real capsule button coordinates, exercising the component callback without pre-hover expansion or synthetic DOM `dispatchEvent`.
- Transient destroyed-node focusout is ignored only while restoration is pending; restored focusin does not loop.

### Pomodoro ordering and warnings

RED:

- `npx vitest run src/lib/floating-coordinator.test.ts`
- Suite failed to import because `floating-coordinator` did not exist.

GREEN:

- Initial coordinator GREEN was 3/3.
- Follow-up RED: the stale-command test expected notification warning `notify` while preserving the newer read snapshot; 1/3 failed because the warning remained null.
- Follow-up GREEN: 3/3 passed. A stale command result cannot replace the newer authority snapshot, but its independently relevant notification warning is preserved.
- Final race RED: the stale-command test imported `pomodoroSnapshotAccepted`; 1/3 failed because the predicate did not exist. Warning preservation had made helper object identity unsuitable as proof of snapshot acceptance.
- Final race GREEN: 3/3 passed. The component captures token/epoch acceptance before applying the helper and calls `applyAcceptedSnapshot` only when that explicit predicate is true.
- A single monotonic epoch covers reads and mutations: the newer start wins for snapshots.
- Command snapshots are accepted only while their token is current, then a fresh authority read starts.
- Read/listener warnings, mutation failures, and notification warnings are separate; a successful read cannot erase a notification warning.
- `pomodoroBusy` still prevents local mutation concurrency; no browser-derived snapshot is accepted.

### Native-size failure, ABA, and retry

RED:

- `npx vitest run src/lib/floating-size-sync.test.ts`
- Suite failed to import because `floating-size-sync` did not exist.

GREEN:

- Initial size coordinator GREEN was 7/7.
- Follow-up RED: the ambiguous-failure test imported the required reconciliation-specific acceptance API; 1/7 failed because `acceptFloatingReconciliation` did not exist.
- Follow-up GREEN: 8/8 passed after adding reconciliation acceptance plus an obsolete-generation rejection test.
- Final race RED: 2/8 failed because retry remained generation 1 rather than allocating a newer generation in both the direct failure and reconciled-failure scenarios.
- Final race GREEN: 8/8 passed. A later explicit retry preserves the desired mode but allocates a new generation, so any still-pending reconciliation from the first attempt is rejected and cannot mark the retry presentation applied.
- Reconciliation updates ready/preferences only for its still-current request generation and never advances presentation `appliedGeneration`; after an expanded/userResized/different-width reconciliation there is no immediate spin, while a later explicit retry yields a newer-generation expanded request.
- `requestSizeSync` creates a new generation for semantic mode changes or a real failed-attempt retry. Ordinary explicit same-mode focus/click events create nothing when there is no matching failure.
- Covers initial preference-read gate, gate failure drain, failure with no spin, explicit later same-mode retry, superseded failure, ambiguous-failure reconciliation, ABA response rejection, obsolete reconciliation rejection, and latest-generation preference acceptance.

ABA timeline:

1. Generation 1 requests expanded.
2. While generation 1 is in flight, generation 2 requests capsule.
3. Generation 3 requests expanded again; backend/user-resize state may now have changed dimensions or `userResized`.
4. Generation-1 expanded response is obsolete even though its mode string equals generation 3. It must not update local preferences or satisfy generation 3.
5. Generation 3 remains pending and must receive its own response before preferences (including width/height/userResized) are accepted.

Ambiguous set/reset failures reconcile with `getFloatingWindowPreferences()`. Reconciliation is accepted only for the same current generation; it updates observed preferences but does not mark a failed command applied. The failed desired intent is retained without immediate retry. A later explicit pointer/focus/click/state event clears that one failure and retries once; a persistent failure does not spin.

## Final behavior audit

- Root is an explicit `<section role="region" aria-label="StarToDo 悬浮窗" data-display-mode={display.mode}>`.
- Capsule implements the exact Props interface and renders one decorative star, phase, tabular timer, ellipsized task, pause/resume primary, exact open-focus and expand labels, and no list/settings/diagnostics.
- Expanded implements the exact Props interface and renders current phase/status/timer/task, first five already-visible-filtered tasks, project and overdue/today/time labels, task/focus actions, always-expanded, conditional reset, hide, and local body scrolling.
- Browser/Tauri guards agree. Runtime initialization retries once when the parent prop becomes true; browser preview still invokes no Tauri command.
- Task and Pomodoro refresh acceptance is monotonic and disposed-safe. Pomodoro mutation snapshots cannot overwrite a newer-started read.
- Size sync is preference-read gated, single-flight, latest-generation aware, and reconciliation-safe. Manual resize preferences remain backend authority.
- One reducer collapse timer exists; timeout dispatches only timeout. Explicit activation uses reducer state, not synthetic ownership.
- Keyboard target restoration maps open-focus to open-focus, primary to primary, and capsule expand/reverse destination to the stable expanded hide control.
- Entry motion is 180ms, capsule/collapse motion 240ms, opacity/transform only, and reduced motion switches directly.
- Orange marks current/primary actions; danger is limited to overdue/warnings.
- File-wide responsive E2E hooks install exactly one pageerror and one console-error listener and assert the collected list after every test.

## Responsive evidence

At 360x260 browser preview:

- viewport 360x260
- document 360x260
- local content body clientHeight 136, scrollHeight 136, `overflow-y:auto`
- expanded mode and open-focus button visible
- document contained on both axes
- browser error collection empty

At active capsule 340x64 with mocked Tauri:

- capsule mode reached from a real running Pomodoro snapshot
- keyboard focus restoration and explicit activation pass
- required capsule resident controls remain present without document-level overflow in the focused responsive suite

## Rulings

- `Ruling: leave tasks.ts and +page.svelte unchanged — their binding import/API migrations already match authority and FloatingWindow can react to the prop transition itself — cost if wrong: unnecessary coordinator churn or runtime initialization remains one-shot.`
- `Ruling: require tauriAvailable and an actual Tauri runtime global — browser preview must never invoke native commands — cost if wrong: E2E console errors and misleading warnings.`
- `Ruling: retry FloatingWindow runtime initialization when tauriAvailable becomes true, guarded by runtimeInitialized — child onMount can precede the parent prop update — cost if wrong: desktop floating data and listeners never initialize or initialize twice.`
- `Ruling: model explicit click/touch activation as reducer activate — user activation must expand without inventing pointerInside or focusInside — cost if wrong: clicks are platform-dependent or expansion becomes stranded.`
- `Ruling: set activation collapseAt deterministically when no real pointer/focus owner exists — an ownership-free interaction still needs one bounded expanded interval — cost if wrong: immediate collapse or permanent expansion.`
- `Ruling: capture the initiating capsule control before reducer focus-in and restore its matching expanded target after tick — subtree replacement must preserve keyboard intent — cost if wrong: focus falls to body or jumps to an unrelated action.`
- `Ruling: ignore focusout only while one target-aware restoration token is pending — destroyed capsule nodes emit transient departure before expanded focus can land — cost if wrong: false collapse races restoration or genuine focus departure is swallowed.`
- `Ruling: use one Pomodoro acceptance epoch for reads and commands — whichever operation starts later owns acceptance — cost if wrong: an older command or poll overwrites newer backend authority.`
- `Ruling: keep notificationWarning separate from read/listener and mutation warnings — notification delivery failure remains relevant after a successful state refresh — cost if wrong: user loses the warning even though timer state is correct.`
- `Ruling: increment native-size semantic generation on every desired transition, including ABA — equal mode strings do not imply equal preference snapshots or userResized state — cost if wrong: first-expanded response incorrectly satisfies later-expanded intent.`
- `Ruling: reconcile preferences after ambiguous set/reset failure without marking the failed intent applied — backend may persist displayMode before set_size returns Err — cost if wrong: local dimensions/userResized stay stale or retry is suppressed.`
- `Ruling: preserve failed desired intent without immediate spin and clear it only on a later explicit interaction/state event — failures are retryable but persistent faults must remain bounded — cost if wrong: permanent suppression or command loop.`
- `Ruling: accept native preferences only for the latest semantic generation — obsolete success or reconciliation cannot overwrite newer desired state — cost if wrong: stale width, height, displayMode, or userResized wins.`
- `Ruling: use file-wide beforeEach/afterEach browser-error hooks — one listener set covers every responsive test consistently — cost if wrong: duplicate listeners add noise or failures escape attribution.`

## Fix round 2/5 evidence

### Reset/native-size serialization

RED before production coordination changes:

- `npx vitest run src/lib/floating-size-sync.test.ts`
- 11 tests: 3 failed, 8 passed.
- All three failures were `requestFloatingReset is not a function`, proving reset had no distinct operation model. The scenarios cover reset requested behind an in-flight set, reset authority versus an obsolete set response in either completion order, and a newer capsule transition requested during reset.

GREEN:

- Focused size coordinator: 11/11 passed.
- Reset and set operations now share one generation/token stream and one serialized drain. A reset receives a distinct intent generation, waits for an older set to settle, and only the latest intent may publish preferences.
- A display-mode change requested during reset supersedes reset acceptance and drains after reset settles. Obsolete set/reset results cannot overwrite newer `userResized`, dimensions, or display mode.

### Failed logical focus restoration

The first browser attempt had a test setup error because the observer was attached before `documentElement`; that run is excluded from behavior evidence. After correcting setup, genuine RED was:

- `npx playwright test --grep "failed logical focus restoration"`
- 1/1 failed: expected capsule after 5,000 ms, received interaction-expanded. No page/console errors remained.

GREEN:

- 1/1 passed with real DOM behavior. The test disables every mapped expanded target before post-tick restoration, leaving no focus owner.
- Production now tries the logical target, then a predictable first enabled marked fallback. After clearing transient focusout suppression it verifies `document.activeElement` is inside the region; if not, it dispatches real `focus-out`, arming bounded collapse.
- Test-quality follow-up replaces section-only `not.toBeFocused()` with the stronger executable assertion `!region.contains(document.activeElement)`, proving no descendant owns focus before time advances.

### Ownership-free activation deadline

Executable browser coverage retains the real touchscreen tap and adds a standards-based `HTMLElement.click()` branch with Playwright clock. The branch asserts focus remains outside the region.

- Initial fake-clock attempts exposed test-timing artifacts: retrying assertions and `setFixedTime` do not provide a reliable relative timer base. Those runs are not production REDs.
- With `clock.pauseAt` immediately before activation, the deterministic test is stable: immediate interaction-expanded, still interaction-expanded at 4,999 ms, capsule exactly at 5,000 ms.
- Stability check: 3/3 repeated runs passed.
- Test-quality follow-up moves the real touchscreen case into a touch-configured `test.describe` using the standard fixture page, so the file-wide pageerror/console-error beforeEach/afterEach hooks observe and assert the actual touch page. The timed ownership-free branch already uses the fixture page.

Controller reset acceptance follow-up RED:

- `npx vitest run src/lib/floating-size-sync.test.ts`
- 13 tests: 3 failed, 10 passed.
- Failures proved: current reset success left desired at the old set generation; an old set reconciliation still overwrote preferences after reset intent; and reset-token reconciliation did not exist.

Follow-up GREEN:

- Size coordinator 13/13 passed.
- Set reconciliation now requires both its desired generation and the global latest generation, so reset intent immediately invalidates any delayed old-set read.
- `nextFloatingResetRequest` returns reset only after size `inFlight` clears, while `nextFloatingSizeRequest` remains blocked by reset pending/in-flight; the production drain uses both selectors and never starts reset concurrently with an older set.
- Reset failure retains its reset token for one best-effort preferences read. Reset reconciliation accepts only that globally current reset token, observes authoritative `userResized`/dimensions without advancing presentation `appliedGeneration`, then clears reset intent. A failed reconciliation clears the intent without looping.
- Current reset success rebases desired to the returned `displayMode` at reset generation, keeping desired/applied state coherent.
- Later audit clarification: HEAD already gates set reconciliation on both global and desired generation and includes fail → reset intent → delayed old reconciliation coverage. Added characterization for two reset intents: an obsolete first-reset failure preserves the newer pending reset and clears only its matching in-flight token; size coordinator 14/14 passed without production changes.

Focused round-2 aggregate GREEN:

- Task 11 pure tests: 3 files, 26/26 passed.
- `npm run check`: 0 errors, 0 warnings.
- Active/failure/deadline/idle focused Playwright: 5/5 passed.

## Fix round 3/5 evidence

RED:

- `npx vitest run src/lib/floating-size-sync.test.ts`
- 17 tests: 3 failed, 14 passed.
- Exact failures: ambiguous reset reconciliation returned obsolete expanded generation 1 instead of reset generation 2; `abandonFloatingResetReconciliation` did not exist; and `nextFloatingSizeRequest` emitted obsolete desired generation 1 while global generation was 2.

GREEN:

- Focused size coordinator: 17/17 passed.
- `nextFloatingSizeRequest` rejects any desired request whose generation is not globally current.
- Current reset reconciliation rebases desired to `{ mode: preferences.displayMode, generation: resetRequest.generation }` without advancing `appliedGeneration`. The drain issues exactly one current presentation set; if that set fails, existing failed-state suppression prevents spin and a later explicit event can allocate a retry.
- If the preferences read after reset failure also fails, pure `abandonFloatingResetReconciliation` clears only the current reset token and rebases desired to the current UI display mode at reset generation, again without applied success. The component uses this helper instead of direct state mutation.
- Reset success and reset reconciliation generics require `{ displayMode: FloatingSizeMode }`; the intersection cast was removed.
- Existing newer mode/reset supersession tests remain green.

## Remaining Minors

- Browser preview still has no task fixture with six visible tasks, so actual expanded-list overflow and the remainder label are covered structurally (`slice(0, 5)`, conditional remainder, local `overflow:auto`) rather than by a populated E2E.
- `FloatingExpandedPanel.svelte` calls `Date.now()` independently from parent `nowUnixMs`, so relative labels and overdue color can disagree briefly around a time boundary.
- Task 12 owns Windows/Tauri smoke for native window resizing, manual user-resize preservation, notification-warning presentation, and real event delivery. Task 11 makes no native-smoke completion claim.

## Verification

Final post-code evidence:

- Direct focused reducer/coordinator files after model-gap fixes: 3 files, 23/23 tests passed.
- `npm run check`: 0 errors, 0 warnings.
- Direct focused Playwright (`active capsule|expanded idle companion`): 3/3 passed.
- Full `npm run test:unit` after round 3: 9 files, 85/85 passed.
- Full Playwright after round 2: 24/24 passed.
- `git diff --check`: exit 0; only Git line-ending notices.
- Changed authorized source/test/report files: strict UTF-8 and nonempty.
- Exact scoped status was reviewed before staging; unrelated dirt and generated artifacts were excluded.

Historical npm grep forwarding behavior remains documented above; all real focused E2E runs used direct Playwright.
