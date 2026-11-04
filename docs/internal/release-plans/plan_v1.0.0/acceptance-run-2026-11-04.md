# Adversarial acceptance re-run - 2026-11-04

This is section 1.6 of [`maintainer-todo.md`](../../maintainer-todo.md). The [2026-07-31 run](acceptance-run-2026-07-31.md) stopped describing the tag binary for two reasons. PR #26 moved clap, which parses the command line. PR #32 changed `rollback`, `plan` and `list` to fix F-1 (rollback leaves a plugin folder behind) and F-2 (`list --html` replaces an existing file).

**Verdict: every planned step passed, and the run found one new issue that needs a maintainer decision.** AR-09 (`archive` copies less than it promises) loses no data during a move, but `archive` is the tool's protection against auto-delete. See [AR-09](#ar-09-archive-copies-less-than-it-promises).

| Fact | Value |
|---|---|
| Binary | `awt 1.0.0`, release build of `main` at `df6985f` |
| Binary SHA-256 | `bf734980e67579c1c0954e12cbad954477caa6a3e54268af38c8e556099fbb59` |
| Scratch home | Fresh copy from `scripts/new-scratch-home.ps1`: 101,617 files, 11.37 GB |
| Synthetic homes | One fresh home per attack, built on the real filesystem |
| Results | 60 of 60 on the synthetic homes (34 engine steps, 26 checks). 56 of 56 on the scratch copy (29 engine steps, 27 checks) |
| Other gates | `cargo audit` exit 0. `awt-core` has no `tauri`, `clap` or network crate. CI passed on `df6985f` |

**The live home was not written to.** The harness refused any `--home` outside its scratch root, and it passed `--home` on every command. `apply` does move a real folder, so the run used `E:\tmp\chain-smoke`, the disposable folder the 2026-07-31 run also used. That folder ended byte-identical at its original path. The scratch copy, backups and archive were deleted afterward.

**Harness errors, none of them the tool's.** The first synthetic run failed A8, because the harness left out `--archive-dir` and the tool refused for that reason instead. The first scratch run crashed reading a `list` row whose `cwd` is null, before it had written anything. Both were corrected, and the results above come from complete runs. Exit codes were checked against stderr, because a clap usage error also exits 2. That trap produced a false pass on 2026-07-31.

## New in this run

| # | Attack | Want | Got |
|---|---|---|---|
| P1 | F-1 on a synthetic home: `apply`, `rollback`, same `apply`, `verify`, `rollback` | Exit 0 each time, no leftover folder, whole tree identical | As wanted. The whole tree was compared after both rollbacks |
| P1 | F-1 on the scratch copy, with a plugin folder seeded | The same, twice | As wanted. Each rollback reported 15 of 15 checks |
| P2 | F-2: `list --html` on an existing file, on both homes | Exit 2, file unchanged | As wanted. A new path still writes |
| P3 | A folder already occupies the rename destination | Exit 2 at `plan` and at `apply`, nothing changed | As wanted, and the refusal names the folder |
| P4 | A manifest with its recorded destinations stripped, as an older build wrote it | Rollback restores, no leftover folder | As wanted |
| L1 | An IDE lock file, with and without `--force` | Exit 2, then exit 0 with a warning | As wanted |
| L2 | No lock file | Exit 0 | As wanted. This records the limit the docs now state for FF-1 (the live-session guard misses terminal sessions). It proves nothing about a running session |
| W1 | A worktree source, with and without `--force` | Exit 2, then exit 0 with a warning | As wanted |

## Repeated from 2026-07-31

### On the scratch copy

| # | Step | Result |
|---|---|---|
| H1 | `doctor`, `list`, `scan` | Exit 0. `list` showed 135 projects with a recorded path. `doctor` took 108 s on a cold cache and about 8 s after that |
| H2 | Inject a forward-slash `projects` key, two `githubRepoPaths` slugs sharing one value, a history line and a plugin folder | The file still parses |
| H3 | `plan`, `apply`, `verify`, run twice | Exit 0. Each `apply` made 8 changes |
| H2b | After each move | The old key is gone, and both slugs were rewritten |
| H4 | `rollback --report <report.json>`, run twice | Exit 0. `.claude.json`, `history.jsonl`, the transcripts and the project folder are all identical to before |
| H5 | `verify` after each rollback | Exit 3 with FAIL lines, as expected |
| H6 | `archive`, then again | 28,641 copied and 1,140 skipped, then 0 copied and 29,781 skipped. See AR-09 for what the count leaves out |
| H7 | `associate` into an existing key | Exit 2 |
| H7 | `associate` from a gone folder to a fresh target | Exit 0, 5 changes |
| H8 | `repair --drive-letter`: dry run, apply, re-run | 34 values and 2,303 lines repaired. The re-run finds nothing repairable |
| H9 | `--json` on `list`, `scan`, `plan`, `repair`, `archive`, `verify` and `doctor` | Every payload parses, including `verify` at exit 3 |

**The D7 tripwire is quiet.** The leftovers after `repair` are 12 distinct values and 818 lines, exactly the known set. D7 (the corruption cause) therefore stays a tripwire and not a task.

### On the synthetic homes

| # | Attack | Want | Got |
|---|---|---|---|
| A1 | `repair` on a history file with invalid UTF-8 | Exit 4, file untouched | As wanted |
| A2 | `plan` with a lowercase, forward-slash spelling of the source | Plugin folder found | As wanted |
| A3 | Missing source | Exit 2 | As wanted |
| A4 | Removed flags `--recursive`, `--scope`, `--on-collision` | Exit 2 as unexpected arguments | As wanted, so clap 4.6.7 still rejects them |
| A5 | Hook payload with a `..` escape | Exit 4, nothing archived | As wanted |
| A6 | A real junction in project state | `plan` exits 2. `archive` exits 0, skips it and copies the transcript | As wanted |
| A7 | Nested project under the source | Exit 2, naming the child | As wanted |
| A8 | Malformed `settings.json` with `--install-hook` | Exit 4, file byte-identical, no temp file | As wanted |
| A9 | Destination exists | Exit 2 | As wanted |
| A10 | Cross-volume destination | Exit 2 | As wanted |
| A11 | `repair` with no selection flag | Exit 2 | As wanted |
| A12 | `associate` from a never-recorded path | Exit 2 from the tool, not from clap | As wanted |
| R1 | `rollback` on wrong-shape JSON | Exit 4, message names both shapes | As wanted |
| R2 | `rollback` on the `report.json` that `apply` prints | Full restore | As wanted |
| R3 | `archive --json` | `copied` and `skipped` present | As wanted |

## AR-09 (`archive` copies less than it promises)

**What happened.** The first sweep into an empty folder reported 1,140 skipped files, so the run checked what the sweep had copied. The skips are harmless, but the check found a larger gap.

**The skips lose nothing.** `copy_if_changed` skips only when the destination already holds identical bytes. The copied count also adds up exactly: 3,957 top-level transcripts, 24,683 file-history files, and one transcript that the later `associate` step moved. The sweep visits some file-history folders twice, and the likely cause is a session shared by two project folders (inferred).

**What the sweep does not copy**, measured on the scratch copy:

| Not copied | Files | Size |
|---|---|---|
| Subagent transcripts (`subagents/*.jsonl`) | 10,405 | 5,785 MB |
| Other subagent files | 9,941 | 2 MB |
| `tool-results/` | 3,666 | 837 MB |
| Other nested `*.jsonl` | 42 | 162 MB |
| `workflows/` | 977 | 51 MB |
| `memory/` | 702 | 3 MB |
| `session-env/` | 1,684 | not measured |
| `tasks/` | 363 | not measured |
| `todos/` | 19 | not measured |

**Why it matters.** AC-35 in [S-02 (inventory, retention, associate)](S-02_inventory-retention-associate/spec.md) says the archive includes file-history, session-env, tasks and todos. Only file-history is implemented, and only it has a test. The design's archive layout also shows `tool-results` and `subagents`. All 3,957 top-level transcripts were copied, so AC-34 holds. A user relying on `archive` to survive auto-delete would still lose subagent transcripts, and nothing tells them. This does not affect a move or a rollback.

**What this run did.** It corrected the pages that overstated the scope, and it did not change the code or the S-02 spec. Whether this blocks the tag is the maintainer's decision, recorded in `maintainer-todo.md` section 1.9.

## What this run does not prove

- **A real session running during a move.** FF-1 still applies, and the guard fix ships in v1.0.1.
- **The compiled desktop `.exe` as a separate file.** Only documentation changed after its build, and the engine inputs are unchanged since `0ef19c1`, so its own acceptance results still apply (inferred).
- **A rename on a live home.** Every state write went to a copy.
