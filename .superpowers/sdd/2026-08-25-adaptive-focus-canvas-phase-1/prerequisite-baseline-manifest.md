# Adaptive Focus Canvas Phase 1 Protected Prerequisite Manifest

## Purpose

Phase 1 is an attributable feature delta over protected pre-existing workspace state. Direct human instruction initially prohibited changing, staging, or attributing the prerequisite files below to this feature branch. This manifest records the exact feature-worktree bytes that were load-bearing for final verification at production source head `1a7734369e6e3389957b223a102053db95ee8bee`.

A clean checkout of the Phase 1-only Git range remains intentionally non-self-contained: committed `src-tauri/src/lib.rs` declares `mod pomodoro`, while `src-tauri/src/pomodoro.rs` and other required source/configuration were protected prerequisite files outside that feature range. The prerequisite bytes were subsequently separately attributed in main commit `d86a795367730eb8c9586f9b2ab211fd16be2fc4` and combined with the complete Phase 1 range in merge commit `31619ddad29bc7bb81e2eafaca22c0eb2fa5327f`. The merged `main` tree is self-contained and was independently verified after integration; the hashes below preserve the pre-integration byte identity and provenance of the protected inputs.

## Exact protected bytes

| Working-tree status | Path | Bytes | SHA-256 |
|---|---|---:|---|
| modified | `.gitignore` | 287 | `8B2658D6D8239D26BDC9150406F115E2ADEB1F0C78889805E08AB8EB91A0CCE0` |
| modified | `package-lock.json` | 80,378 | `3E44C9176C5158B63AD53D99FBAF22AFD902B116C4305F37D98C21760750D357` |
| modified | `package.json` | 1,452 | `1481C72A7FE9C8F8BFE14E6D5ADA88EECFA839AB4487143E2EB0F89A9870A646` |
| modified | `src-tauri/Cargo.lock` | 136,948 | `F9B65225003735E7F97921B37EFB1BF698CF75A974A3B653E4B35119539317C5` |
| modified | `src-tauri/Cargo.toml` | 1,020 | `3529BB5FDFC2D7A80F521AB7BDF666A9022ED0CDB3464BA89B9FB9E1938F27E1` |
| modified | `src-tauri/capabilities/default.json` | 327 | `5EB3EEC54CA59231EB1FED5905866E73B7DC09EEBEBC24312C201F006A7EEAB1` |
| modified | `src/lib/components/ConfirmDialog.svelte` | 3,897 | `018EDE94D65424DC63710D988E46BC1F7370AA3F77FD5194C146254142DF2CA4` |
| modified | `src/lib/components/ProjectSidebar.svelte` | 9,558 | `D325BEB8C2126E413D15D2348CC15F2C182DFB9AFD0E22CB0AA442873F5EBE51` |
| modified | `tools/notification-host/Program.cs` | 19,206 | `0C81EB760A4F9F499B6DA5E141C463FF0E3F502564BB20FE36DABDE50A4E009F` |
| untracked | `playwright.config.ts` | 726 | `59F90C5BC510A6CC011650C15AC6BAC8ECC98A0DC89211CC8CB402C554E72EE6` |
| untracked | `rust-toolchain.toml` | 88 | `8C2DC5F2CF91553372842D942A9570C0C49EB13B8D290A6994FCEFA99534FE53` |
| untracked | `src-tauri/src/pomodoro.rs` | 60,887 | `1D5B2427FAE550063385DBB960F4DB731CD3CD08340BD192C4DF6708694646CF` |
| untracked | `src/lib/pomodoro.test.ts` | 4,949 | `7254F207C3EE449DAD94CBF6F92EECFFD0656C7BB10E978E523D68F279614AE8` |
| untracked | `src/lib/pomodoro.ts` | 7,309 | `60FCC3FA80D0AB00321C1F10721677618EC37BC40F80819328D1984899AAA145` |
| untracked | `src/lib/preferences.test.ts` | 1,847 | `33360154D6695634AF92368E98F284962CFA1D89C3F4DCC64CC45EFE44BFC13E` |
| untracked | `src/lib/tasks.test.ts` | 11,953 | `D9BA29B7A5B3F6E860854F2318AC94D6D07E84ED83108A5C843A38E268C211B5` |
| untracked | `src/routes/+layout.svelte` | 116 | `73D5A71C23A75742A95D1C53909A5D42043BA2D4C2A01B66923B2992D66BDC1B` |
| untracked | `vitest.config.ts` | 539 | `0F5D6D79083D8504E3F50339A5E804152795139C419B88AC56DFD91455BB92EE` |
| untracked | `docs/superpowers/specs/2026-08-25-adaptive-focus-canvas-design.md` | 11,170 | `3A951FAA2FAA3A120E885916994727DBE4CF9D29A74AEC418D9DEC85F407EB88` |

## Cross-worktree comparison

Seventeen of the nineteen listed files were byte-identical between the isolated feature worktree and ordinary `D:\Code\Rust\StarToDo` at handoff time. Two files were semantically identical but not byte-identical:

- `package-lock.json`: ordinary main retained a UTF-8 BOM; the feature-worktree copy did not. `git diff --no-index` showed only the first-character BOM difference.
- `src-tauri/Cargo.toml`: `git diff --no-index --ignore-space-at-eol` exited 0; the hashes differ only because of line-ending representation.

For an exact reproduction of the verified final build, use the feature-worktree hashes in this manifest. Do not normalize or rewrite either worktree as part of Phase 1 cleanup. Unrelated workspace metadata (`$null`, `.agents/`, `.github/`, `.plan/`, `.superpowers/brainstorm/`, and `AGENTS.md`) is intentionally excluded because it is not a product build prerequisite.

## Integration result

1. The workspace and retained `stash@{0}` were preserved while the prerequisite source was separately attributed.
2. Main commit `d86a795367730eb8c9586f9b2ab211fd16be2fc4` establishes the approved prerequisite baseline as a distinct commit, separate from Phase 1 task authorship.
3. The complete Phase 1 range was merged locally with commit `31619ddad29bc7bb81e2eafaca22c0eb2fa5327f`; its merged tree is `aae1c5da63d9f052e9fde2dc33ae48a797311fff` before this documentation follow-up.
4. Post-merge `npm run verify` exited 0: Svelte/TypeScript 0 errors/0 warnings, Vitest 9 files/86 tests, notification-host publish, static build, Rust format check, Rust 87 tests plus two zero-test targets, and Playwright 35/35.
5. Post-merge `npm run tauri -- build -- --bundles nsis` exited 0. The generated installer was 29,099,220 bytes, SHA-256 `E73C9C001CEC245641407DDD19016C945986DE341E9D5B279F4D3C51AA1B0AB7`, Authenticode `NotSigned`; merged notification-host copies were 93,107,335 bytes with SHA-256 `52AC4808B6E8DBB5DD5E6B7DB940040FB0F9A2523873CB98A05FAECBD064B1A8`.
6. The exact protected-input hashes above remain the provenance record for the pre-integration bytes. `package-lock.json` is 80,381 bytes in the merged working tree because Git's configured checkout conversion differs from the 80,378-byte feature-worktree snapshot; no semantic dependency change was made during integration.
7. The 14 unchecked Windows rows remain explicitly deferred; the unsigned installer was built but not installed or launched, and native observations remain tied to their listed native source/binary revisions.
