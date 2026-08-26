# Task 10 Report — Floating Window Auto-Size and Manual-Resize Protection

## Scope and base proof

- Worktree: `D:\Code\Rust\StarToDo-adaptive-focus-canvas-phase1`
- Branch: `adaptive-focus-canvas-phase1`
- Frozen HEAD before first edit: `b2c4f50f8aee3618009edec053a039febd470090`
- `src-tauri/src/lib.rs`: 167061 bytes, SHA256 `A47838984A2F64FF6000BE62B828D88422D777F40B62C7DA5A3112C0909294A3`
- `src/lib/windowing.ts`: 1669 bytes, SHA256 `EC38C409A06DFFB7F5C04BAE506DED8E5A56431D12B29C2BD9B2D6DD1A9A9385`
- Both owned production files were clean and matched the manifest before editing. Existing unrelated dirty/untracked files were preserved.

## TDD evidence

### RED, before production changes

1. `cargo test --manifest-path src-tauri/Cargo.toml floating_display -- --nocapture`
   - Exit 1.
   - Genuine expected compile RED: missing `FloatingDisplayMode`, missing `floating_display_size`, missing `FloatingWindowPreferences.user_resized`, and missing `FloatingWindowPreferences.display_mode`.
   - Rust emitted 7 errors: E0433, E0425, and E0609.
2. `cargo test --manifest-path src-tauri/Cargo.toml legacy_floating -- --nocapture`
   - Exit 1.
   - Genuine expected compile RED with the same 7 missing enum/helper/field errors.
3. `cargo test --manifest-path src-tauri/Cargo.toml matching_programmatic_resize_before_deadline_clears_target_without_marking_user -- --nocapture`
   - Exit 1.
   - Genuine expected compile RED: missing `FloatingWindowRuntime` and `classify_floating_resize`, alongside the prescribed missing schema symbols.
4. Reviewer-driven generation regression RED: `cargo test --manifest-path src-tauri/Cargo.toml repeated_programmatic_resize_requests_receive_distinct_generations -- --nocapture`
   - Exit 1.
   - Genuine expected compile RED: missing generation helper/cleanup behavior and the new generation field was not yet wired into constructors.

The two prescribed RED commands were run concurrently and briefly reported Cargo package/build lock contention; each completed with its own genuine compiler output.

### Focused GREEN

1. `cargo test --manifest-path src-tauri/Cargo.toml floating_display -- --nocapture`
   - 1 passed, 0 failed, 80 filtered out; main binary 0 tests.
2. `cargo test --manifest-path src-tauri/Cargo.toml legacy_floating -- --nocapture`
   - 1 passed, 0 failed, 80 filtered out; main binary 0 tests.
3. `cargo test --manifest-path src-tauri/Cargo.toml programmatic_resize -- --nocapture`
   - Initial behavior run: 3 passed, 0 failed, 78 filtered out.
   - After reviewer-driven generation hardening: 4 passed, 0 failed, 79 filtered out.
4. `cargo test --manifest-path src-tauri/Cargo.toml resize_without_programmatic_target -- --nocapture`
   - 1 passed, 0 failed, 80 filtered out.
5. `cargo test --manifest-path src-tauri/Cargo.toml failed_older_resize -- --nocapture`
   - 1 passed, 0 failed, 82 filtered out.

One attempted cargo invocation supplied two positional test filters and failed at argument parsing; it did not run tests or indicate a code failure. The valid single-filter commands above were then used.

## Implementation summary

- Added camelCase `FloatingDisplayMode::{Capsule, Expanded}` with exact recommended sizes `(340.0, 64.0)` and `(360.0, 260.0)` and default `Expanded`.
- Extended persisted floating preferences with serde-default `user_resized=false` and `display_mode=Expanded`; defaults are now 360 x 260.
- Updated validation to width 260..=800 and height 56..=800 while preserving coordinate bounds.
- Added `FloatingWindowRuntime` to `AppState`, with the exact required target/deadline fields plus an internal generation token used only to distinguish repeated/concurrent resize requests during error cleanup.
- Programmatic target state is installed before `set_size`; the runtime mutex is released before calling Tauri. A failed older request can clear only its own generation and cannot erase a newer identical target.
- Split actual window events: Moved persists only logical coordinates; Resized converts the event physical size to a rounded logical tuple, classifies it, and persists the actual logical width/height.
- Serialized floating preference read-modify-write transactions through a dedicated mutex so callbacks cannot revert newer command state. The preference mutex is not held across `set_size` or any visibility/focus API.
- Added and registered `set_floating_display_mode` and `reset_floating_auto_size`. Neither command calls show, hide, or focus. Existing hidden windows can resize while remaining hidden.
- Extended `src/lib/windowing.ts` with the exact union, preference fields, and typed invoke wrappers.

## Quality and static audit

- Lock lifetime: runtime lock is dropped before `set_size`; preference transaction lock is separate and is released before window API calls. Callback reentrancy therefore does not reacquire a held runtime or preference lock.
- Physical/logical conversion: Resized uses the event's physical `size`, converts with the current scale factor, rounds once to `(u32, u32)`, then persists those logical values. Moved converts only event position and never reads or classifies size.
- Clock boundary: `now <= programmatic_until_unix_ms` is protected; `now > until` is expired.
- Target clearing: matching protected target clears target/deadline; intermediate protected mismatch retains them; expired target clears and marks user; no target marks user.
- Error cleanup: target, deadline, and generation must all match the failed request. A newer repeated identical request is preserved.
- Event persistence: a dedicated preference mutation lock prevents lost updates between move/resize callbacks and mode/reset commands.
- Legacy serde: the exact legacy JSON preserves visible/x/y/width/height/alwaysOnTop and defaults the two new fields.
- Validation: range containment rejects NaN and positive/negative infinity implicitly as well as out-of-range finite values; coordinate validation is unchanged.
- Visibility/focus: the new commands contain no show/hide/focus calls. Only the pre-existing explicit show/create flows retain such calls.
- Formatting scope: first `cargo fmt ... -- --check` found one owned-file line wrap. It was fixed with one targeted edit; no broad formatting write was run and no unowned Rust source was modified.

## Rulings

- Ruling: treat the deadline instant as still protected (`now <= until`) — the brief says the deadline is unexpired through its boundary — cost if wrong: an event arriving in the exact deadline millisecond could be classified opposite to controller intent.
- Ruling: retain intermediate mismatches until the target arrives or the deadline expires — native resize callbacks can emit transient sizes — cost if wrong: a user's real resize within the 1.5-second protection window could be ignored until a later event.
- Ruling: add an internal generation token in addition to the exact required runtime fields — target/deadline equality alone cannot distinguish identical requests issued in one millisecond — cost if wrong: negligible private runtime state; omitting it risks an older failure erasing a newer request.
- Ruling: serialize floating preference mutations with a dedicated mutex — file-backed read-modify-write otherwise loses concurrent command/callback fields — cost if wrong: very short preference I/O serialization; omitting it risks reverting display mode or manual-resize state.
- Ruling: persist preferences before applying programmatic resize in the two new commands — mode/reset must always persist, while the resize is a fallible presentation side effect — cost if wrong: on a resize failure the command returns an error although the requested preference is already durable.
- Ruling: rely on Rust range containment for finite validation — NaN and infinities fail inclusive range containment already — cost if wrong: error text remains range-oriented rather than explicitly saying “finite.”

## Verification ordering and evidence

1. Prescribed focused RED commands: both exit 1 with 7 expected missing-symbol/field compile errors.
2. Focused GREEN: prescribed tests and pure behavior filters pass as detailed above.
3. `npm run check`: 0 errors, 0 warnings.
4. First full `cargo test --manifest-path src-tauri/Cargo.toml`: 81 passed, 0 failed; main 0; doc 0.
5. First `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`: exit 1 for one owned-file line wrap; targeted edit applied.
6. Post-edit `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`: exit 0.
7. Post-edit full Rust: 81 passed, 0 failed; main 0; doc 0.
8. Post-edit `npm run check`: 0 errors, 0 warnings.
9. Reviewer identified two Important concurrency defects; tests were added first, both defects were fixed, and focused generation tests passed.
10. Final pre-stage `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`: exit 0.
11. Final prescribed focused commands: each 1 passed, 0 failed, 82 filtered out; main binary 0 tests.
12. Final full `cargo test --manifest-path src-tauri/Cargo.toml`: 83 passed, 0 failed; main 0; doc 0.
13. Final `npm run check`: 0 errors, 0 warnings.
14. Final `git diff --check`, UTF-8/nonempty audit, and exact staged-scope inspection are performed immediately before commit.

Rust test runs emit one existing Windows/MSVC linker informational warning (`linker stdout` reporting creation of `.dll.lib` and `.dll.exp`); there are no compiler warnings attributable to Task 10. Cargo lock-wait messages occurred only while intentionally running filtered commands concurrently.

## Deferred Minors

- Explicit `is_finite()` validation and focused NaN/infinity/boundary tests are not required for correctness because the existing inclusive range predicates reject every non-finite value. This remains a documentation/error-message clarity improvement, not a functional gap.
- No live Windows GUI smoke test was performed in this bounded backend/typed-wrapper task. Static inspection proves the new commands do not show, hide, or focus; controller-level Windows smoke coverage remains appropriate for the phase integration pass.
