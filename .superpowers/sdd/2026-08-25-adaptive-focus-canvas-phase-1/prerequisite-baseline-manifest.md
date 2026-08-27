# Adaptive Focus Canvas Phase 1 Protected Prerequisite Manifest

## Purpose

Phase 1 is an attributable feature delta over protected pre-existing workspace state. Direct human instruction prohibited changing, staging, or attributing the prerequisite files below to this feature branch. This manifest records the exact feature-worktree bytes that were load-bearing for final verification at production source head `1a7734369e6e3389957b223a102053db95ee8bee`.

A clean checkout of the Phase 1 Git range is not independently buildable: committed `src-tauri/src/lib.rs` declares `mod pomodoro`, while `src-tauri/src/pomodoro.rs` and other required source/configuration remain outside the committed range. The hashes below do not make those files part of Phase 1; they establish the byte identity required for a separately attributed prerequisite commit or integration base.

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

## Required integration order

1. Preserve the current workspace and retained stash until the prerequisite source is separately attributed.
2. Establish a prerequisite commit/tree containing the exact approved product/configuration bytes above; keep it distinct from Phase 1 authorship.
3. Confirm the prerequisite tree satisfies every hash, or document and review any intentional normalization before proceeding.
4. Apply the complete Phase 1 range rather than isolated task commits.
5. Verify the resulting combined tree is clean and self-contained.
6. Re-run `npm run verify` and `npm run tauri -- build -- --bundles nsis` from that clean tree.
7. Record the clean combined tree ID, notification-host SHA-256, installer size/SHA-256, and signing status.
8. Do not represent merge or release as complete until the 14 unchecked Windows rows are either executed in a disposable installed environment or remain explicitly deferred.
