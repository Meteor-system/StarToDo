# Task 11 report

## Outcome

Implemented the adaptive floating focus companion on frozen base `295785b379614296dd9a26606ac2afba59cbf8e6` in the prescribed isolated worktree. Added separate capsule and expanded visual components, connected the existing reducer, preserved backend authority, strengthened stale-response protection, and added the prescribed responsive E2E.

Changed paths:

- `src/lib/components/FloatingCapsule.svelte`
- `src/lib/components/FloatingExpandedPanel.svelte`
- `src/lib/components/FloatingWindow.svelte`
- `e2e/responsive.spec.ts`
- `.superpowers/sdd/2026-08-25-adaptive-focus-canvas-phase-1/task-11-report.md`

`src/lib/tasks.ts` and `src/routes/+page.svelte` were proven already migrated and were deliberately left unchanged. Preference/show/hide/size imports remain owned by `windowing.ts`; floating task/focus intents remain owned by `tasks.ts`.

## Baseline evidence

Owned-path-only status was clean before the first edit. Fingerprints matched `task-11-base.txt` exactly:

- `FloatingWindow.svelte`: 15028 bytes, SHA-256 `6680C7AB98669F854E711EB6C063EEB3512F8E1C9F382312FD655C8A30F59F86`
- `tasks.ts`: 17366 bytes, SHA-256 `E6E54086EB3050759057B85EB7526033D85950E7170EA5B72918330EB8258FA1`
- `+page.svelte`: 37430 bytes, SHA-256 `FC7521E0EA32DD2757D87F0A70D34D5A159982B176A175A691D90A24B36F8957`
- `responsive.spec.ts`: 9328 bytes, SHA-256 `4B7C42980451DBAFA8B6802961329E43631D297003411DE0FC794F1948A60949`
- Both component create paths and this report path were absent.

Unrelated global dirt was preserved and never staged or modified.

## TDD evidence

The prescribed command did not forward grep:

- `npm run test:e2e -- --grep "expanded idle companion"`
- Actual script invocation: `playwright test expanded idle companion`
- Result: exit 1, `Error: No tests found.`

Direct genuine RED before production code:

- `npx playwright test --grep "expanded idle companion"`
- 1 test run, 1 failed.
- Expected `data-display-mode="expanded"`; received no attribute (`null`) on the existing region.

Direct GREEN after production code:

- `npx playwright test --grep "expanded idle companion"`
- 1 passed.
- Final fresh focused run: 1 passed in 7.7s.

## Verification

- `npm run test:unit -- src/lib/floating-display.test.ts`: 1 file, 9/9 tests passed. This npm script forwarded the file argument correctly.
- Direct `npx vitest run src/lib/floating-display.test.ts`: 1 file, 9/9 passed.
- `npm run check`: 0 errors, 0 warnings.
- Final `npm run test:unit`: 7 files, 62/62 passed.
- Focused `npx playwright test --grep "expanded idle companion"`: 1/1 passed.
- `npx playwright test e2e/responsive.spec.ts`: 8/8 passed.
- Final full `npx playwright test`: 20/20 passed.
- `git diff --check` on owned outputs: exit 0; only Git line-ending notices, no whitespace errors.
- All changed source/E2E files were validated as nonempty strict UTF-8.
- Playwright browser-error hooks remained empty for the responsive case and full suite.

## Responsive measurements

At 360x260 browser preview:

- viewport: 360x260
- document: 360x260
- local content body: clientHeight 136, scrollHeight 136, computed overflow-y `auto`
- `data-display-mode`: `expanded`
- `打开专注工作区` visible
- document horizontal and vertical containment passed
- no Tauri invoke warning or browser error

Capsule static fit at 340x64:

- Exactly four resident groups: one decorative star marker, one combined phase/timer/ellipsized-task focus control, one pause/resume primary control, and one expand control.
- Grid is `auto minmax(0, 1fr) auto auto`; all children use `min-width: 0`; task text ellipsizes; total vertical sizing is bounded by the 64px host.
- No task list, settings, warnings, or diagnostics are rendered by the capsule component.

## Static audit

- Exact Props interfaces implemented for both new components.
- Expanded panel receives already-visible-filtered tasks and renders `slice(0, 5)` with project and overdue/today/time labels.
- Only `.content-body` owns local scrolling; the root/document is locked and every changed html/body inline style is restored exactly on destroy.
- Reset auto size is rendered only for backend `userResized`; always-expanded is persisted only through `writeFloatingExpansionPreference` and the reducer event.
- Root has exact section/region semantics through the named `<section>`, `data-display-mode`, pointer events, focus events, and related-target containment.
- One collapse timer exists; it is cleared before replacement and on destroy; timeout dispatches only `{ type: 'timeout', at: Date.now() }`.
- Keyboard focus does not manufacture pointer-inside state. Mode replacement restores the logical primary focus target after `tick()` when focus was inside.
- Tasks and Pomodoro refreshes use independent monotonic sequences plus disposed checks. A Pomodoro mutation increments the refresh sequence before awaiting the command, accepts only the mutation snapshot, then refreshes backend authority.
- Browser preview invokes no Tauri commands because the runtime guard requires both `tauriAvailable` and an actual Tauri global.
- Desktop size requests are microtask-coalesced before launch and then single-flight with one latest pending target. Successful current responses update preferences; superseded responses cannot overwrite them. Failures warn, do not mutate Pomodoro state, do not spin retry, and later explicit transitions can retry.
- Manual `userResized` remains backend authority; reset uses only the backend command and accepted response.
- Entry motion is 180ms, capsule/collapse motion 240ms, opacity/transform only, with direct reduced-motion switching. Outer dimensions are never animated in CSS.
- Orange is restricted to current/primary emphasis; danger is used only for overdue and warning content.

## Independent review

Independent review found no Critical issues. Three Important issues were fixed before final verification:

1. Permanent suppression of a transiently failed size mode was removed; later explicit requests may retry without spin retry.
2. Focused capsule DOM replacement now restores the corresponding logical primary focus target after rendering.
3. Size requests now coalesce in a microtask before the serialized latest-pending drain starts.

## Rulings

- `Ruling: leave tasks.ts and +page.svelte unchanged — their binding import/API migrations already match authority — cost if wrong: unnecessary churn or ownership regression.`
- `Ruling: require tauriAvailable and an actual Tauri runtime global — browser preview must never invoke native commands — cost if wrong: E2E console errors and misleading warnings.`
- `Ruling: accept running and paused sessions as focus-active — both are live sessions for display breathing — cost if wrong: paused sessions expand as idle and violate reducer behavior.`
- `Ruling: sequence refresh starts, not completions — only the newest task/Pomodoro request may update state — cost if wrong: older polls overwrite listener or mutation results.`
- `Ruling: invalidate Pomodoro refreshes before mutation and refresh again after accepting mutation — Rust/SQLite stays authoritative — cost if wrong: stale poll rollback or browser-invented state.`
- `Ruling: microtask-coalesce then serialize native size requests — rapid same-turn transitions collapse to the latest target while in-flight commands remain ordered — cost if wrong: obsolete resize flicker or stale response wins.`
- `Ruling: do not permanently blacklist failed size modes — persistent failures must not spin, but later user/state transitions may retry — cost if wrong: transient failure disables sizing for the rest of the mount.`
- `Ruling: restore the logical primary focus target after a focused mode swap — reducer-driven DOM replacement must not strand keyboard users — cost if wrong: focus loss and premature collapse.`
- `Ruling: let root pointer/focus events drive capsule expansion and only reaffirm focus-in when the root actually matches :focus-within — reducer state must follow DOM truth without synthetic pointer state — cost if wrong: expansion can become stranded or collapse while keyboard focus is active.`
- `Ruling: keep warning overlay outside both pure visual component interfaces — exact Props remain binding and warnings stay coordinator-owned — cost if wrong: component API drift or business concerns leak into visuals.`

## Minors

- The browser preview has no task-fixture injection, so the E2E proves the local body scroll contract (`overflow-y:auto`) but cannot create six visible tasks to prove actual `scrollHeight > clientHeight`, five residents, and the remainder label. Those behaviors are statically explicit in `slice(0, 5)` and the conditional remainder.
- The active 340x64 capsule cannot be reached in browser-only E2E because browser preview has no backend Pomodoro snapshot injection. Its exact resident set and fit constraints were audited statically; a Windows/Tauri smoke test should exercise the live active-session capsule.

## Follow-up static notes

A controller follow-up was incorporated before final handoff:

- Capsule primary `busy` now includes `!tauriAvailable`, matching the expanded panel.
- Capsule expand no longer synthesizes pointer state. It only reaffirms `focus-in` when the root actually matches `:focus-within`; real pointer/focus root events remain authoritative.
- Desktop size requests queue while `getFloatingWindowPreferences` is unresolved. The current preference response establishes `floatingPreferences` and `lastSentSizeMode`, then marks sizing ready and drains the latest requested display mode. A failed preference read warns, marks ready, and drains without a browser invoke.
- The exact floating E2E preserves its prescribed assertions and is covered by the file-wide beforeEach pageerror/console collection plus afterEach empty-array assertion; no duplicate listeners or in-test assertion are needed.

Additional rulings:

- `Ruling: disable both capsule and expanded primary controls whenever tauriAvailable is false — unavailable native mutations must not appear actionable — cost if wrong: misleading browser/unavailable UI.`
- `Ruling: let real root pointer/focus events drive expansion and make onExpand conditional on actual focus-within — reducer input must reflect DOM truth — cost if wrong: stranded focusInside or synthetic interaction state.`
- `Ruling: make the initial preference read a barrier before native size drain — the read establishes the authoritative current mode before queued requests execute — cost if wrong: an older read response can overwrite newer command preferences or lastSent state.`
- `Ruling: use the file-wide beforeEach/afterEach browser-error hook for the exact floating case — one listener set covers every responsive test and attaches failures consistently — cost if wrong: duplicate listeners or assertions add noise without improving coverage.`
- `Ruling: capture the capsule control's logical focus target before reducer expansion and restore its expanded counterpart after tick — keyboard expansion must preserve user intent across DOM replacement — cost if wrong: focus falls to the document and collapse may arm while the user is interacting.`
- `Ruling: ignore focusout only while a target-aware restoration token is pending — the destroyed capsule emits a transient departure before the expanded target can receive focus — cost if wrong: a false focus-out deadline races the restored focus or genuine departures are suppressed too broadly.`

## Focus-preservation follow-up

A direct active-capsule E2E is not feasible in browser preview because there is no Tauri/Pomodoro snapshot fixture. TDD therefore covers the pure logical mapping: direct Vitest first failed 1/10 because `floatingFocusTargetSelector` did not exist, then passed 10/10 after implementation. Capsule and expanded controls now carry matching `data-floating-focus-target` markers for `open-focus`, `primary`, and `expand`; the expanded stable target for capsule expand is the hide control. Root `focusin` captures the target synchronously, reducer expansion replaces the panel, `tick` restores that corresponding target, and transient destruction focusout is ignored only until restoration completes.
