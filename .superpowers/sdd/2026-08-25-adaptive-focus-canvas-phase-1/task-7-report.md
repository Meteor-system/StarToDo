# Task 7 Implementation Report

Status: DONE

## Outcome

Implemented the Phase 1 task details drawer, focusable task-row Space completion, and non-blocking green star completion feedback. Task editing and normal deletion now live in one `TaskDetailsDrawer`; normal task rows expose one details action while trash rows retain restore and permanent-delete behavior.

## TDD Evidence

1. Created `src/lib/task-interaction.test.ts` first with the brief's exact four cases.
2. RED command: `npm run test:unit -- src/lib/task-interaction.test.ts`
   - Exit code: 1
   - Genuine failure: `Cannot find module './task-interaction'`
   - Test files: 1 failed; tests were not collected because the production module did not exist.
3. Created the minimal `src/lib/task-interaction.ts` predicate with the exact requested interface.
4. GREEN command: `npm run test:unit -- src/lib/task-interaction.test.ts`
   - Exit code: 0
   - Test files: 1 passed
   - Tests: 4 passed

## Implementation Notes

- `TaskDetailsDrawer.svelte` owns its draft, field validation, local datetime conversion, recurrence timezone defaulting, normal-delete confirmation, command-error alerts, Ctrl/Cmd+Enter save, title focus on open, and success-only close.
- False `onChanged` or `onRemoved` results retain the drawer and draft. Command errors also retain both.
- Every `TaskItem.svelte` article remains focusable through the brief-exact `tabindex="0"` row contract. Unmodified Space toggles only when the event target is the row itself, and `!trashMode` prevents trash-row Space from mutating the task. Nested restore and permanent-delete buttons remain semantic and independently operable.
- Normal inline editing and normal-delete confirmation were removed from `TaskItem`; its details button has `aria-label="查看详情…"`.
- Focus preselection, snooze, defer, Pomodoro count, complete/restore, trash restore, permanent-delete confirmation, alerts, transition behavior, and task action target ids remain.
- `TaskScene.svelte` owns one `selectedTaskId`, derives one selected task, clears missing selections, renders one details drawer, and passes real token-aware callbacks.
- Completion feedback is triggered only after the current operation token is accepted for `kind === 'complete'`. It replaces/restarts on newer feedback, lasts 900ms, does not block operations, uses a sequence key for repeat animation, ignores pointer interaction, and removes scale/translate under reduced motion.
- Completion focus adjacency is selected from active tasks, preserving focus on the next executable task; recurring successor fallback and existing operation-token checks remain intact.

## Review

Independent review found no Critical issues and three Important issues:

- Ctrl/Cmd+Enter could save behind the delete confirmation.
- Trash rows had misleading completion-button row semantics.
- Completion adjacency could cross into the completed group.

All three were corrected before final verification. The reviewer also noted dead `TaskItem` editor/delete props as Minor; these were removed from the row component while preserving `TaskCanvas`'s external Task 6 callback/operation surface for the details drawer and scene.

Controller pre-review then identified two accessibility/event-scope corrections: the focusable article must remain a non-button row because it contains interactive descendants, and Ctrl/Cmd+Enter must be scoped to the details form rather than the window. The interactive role and row-level completion label were removed, the brief-aligned `article tabindex="0"` contract was restored with narrow Svelte diagnostics ignores, and the save key handler was moved onto the form. The copied archived-project note was also corrected to require `taskProject !== null` before checking its archive timestamp.

### Independent review fix round 1/5

Two Important defects were addressed in a new follow-up commit:

- Nested confirmation Escape propagation: `ContextDrawer` now ignores events already handled with `preventDefault()`, including both panel and backdrop Escape handling. ConfirmDialog Escape therefore cancels only the confirmation and preserves the details drawer draft; unhandled Escape still closes the drawer once. The measured Chromium probe showed the nested fixed confirmation covers the full viewport and receives hit testing, so no z-index change was made and the pre-existing dirty `ConfirmDialog.svelte` was not staged.
- Details operation presentation ownership: save/delete capture task id, monotonically increasing draft generation, and a unique operation id. Token-aware `onChanged`/`onRemoved` still run so accepted data applies, while drawer close, command error, and busy clearing occur only when the originating operation still owns the current presentation. Switching tasks or reopening the same task creates a new draft generation whose state cannot be overwritten by the old operation. `TaskScene` also avoids moving focus behind a different selected task drawer after accepted changes/removals.

TDD evidence for the ownership helper:

- RED: `npm run test:unit -- src/lib/task-interaction.test.ts` — 3 ownership cases failed with `TypeError: ownsTaskDetailsOperation is not a function`; the original 4 keyboard cases passed.
- GREEN: the same focused command — 1 file passed, 7/7 tests passed after adding the minimal pure helper.
- Ownership wiring follow-up RED: the realistic pre-effect switch case now passes both current prop task B and still-current draft task A. The old six-argument helper misread the new signature, causing the fully matching seven-argument ownership case to fail (`expected false to be true`) while the switched-task race case correctly remained unowned.
- Ownership wiring follow-up GREEN: the helper now requires current prop task id === current draft task id === operation task id, plus matching generation and operation id; the drawer passes `task?.id ?? null` and `draftTaskId`. The focused suite returned 7/7 passed.

Deferred Minor: component-boundary E2E coverage for normal row Space, trash-row Space, and nested-button keyboard behavior is deferred to Task 12's keyboard matrix; no static substitute weakens that requirement.

Deferred Minors: the Task 12 keyboard-matrix coverage above.

## Verification

- Focused interaction GREEN: `npm run test:unit -- src/lib/task-interaction.test.ts` — 1 file passed, 7/7 tests passed after the ownership-wiring correction.
- Strict diagnostics: `npm run check` — 0 errors, 0 warnings in the post-ownership-wiring rerun.
- Full unit suite: `npm run test:unit` — 6 files passed, 58/58 tests passed after the ownership-wiring correction.
- Focused static/behavior audit: PASS for exactly one details drawer, no TaskItem inline editor or normal-delete confirmation, row/nested-control keyboard boundary, current-token feedback placement, local task scrolling, and success-only drawer close.
- `git diff --check` — exit 0; only Git line-ending conversion notices.
- UTF-8/non-empty validation — passed for every implementation output. The report was validated again before staging.
- Self-review covered every brief step and the operation-token, recurring-successor, adjacent-focus, local-scroll, Pomodoro-authority, and reduced-motion invariants.

## Rulings

Ruling: Keep nested ConfirmDialog z-index unchanged based on measured browser stacking evidence — the fixed confirmation covered the full Chromium viewport and received center hit testing inside the drawer stacking context — cost if wrong: Windows WebView could render it differently, so Task 12 Windows smoke must verify confirmation visibility/Escape.

Ruling: Gate presentation close/error/busy by current prop task id + current draft task id + draft generation — each operation also carries a unique operation id so a later operation in the same draft owns presentation state, without relying on effect timing after a prop switch — cost if wrong: a legitimate old operation may apply data without dismissing the current drawer, requiring explicit user close.

Ruling: Keep the required `TaskCanvas` callback/operation surface even when `TaskItem` no longer consumes update/delete callbacks — the details drawer and scene still require the Task 6 integration boundary — cost if wrong: a future cleanup may move more callbacks out of `TaskCanvas`.

Ruling: Keep every TaskItem article focusable while gating row-Space with `!trashMode`, and retain semantic nested restore and permanent-delete buttons — this follows the exact row-focus contract without allowing trash-row keyboard mutation — cost if wrong: one redundant trash-row tab stop versus deviation from the exact row-focus contract.

Ruling: Derive completion adjacency from active tasks, then use the existing recurring-successor/search fallback — completion should restore focus to the next executable task, not an already-completed row — cost if wrong: an unusual filtered ordering could prefer a different visible neighbor.

Ruling: Stage the final full `TaskItem.svelte` path despite inherited overlap — Task 7 owns and refactors the component interface, and omitting the inherited Pomodoro-count/focus-preselection/semantic styling work would make the Task 7 commit non-reproducible — cost if wrong: task attribution is less pure.

## Preserved Dispatch Overlap

`src/lib/components/TaskItem.svelte` was already dirty at dispatch with SHA256 `A8F5C665B8D94D00CDA5FFF6EB03677773EB6D7065ED5926B6F961B8A723512D` and a baseline diff of 39 insertions/7 deletions. The inherited Pomodoro-count display, focus-preselection action that does not start the timer, and semantic styling changes were preserved in the final component and are intentionally included in this Task 7 commit. No other pre-existing dirty path is staged.
