# Archive acceptance rerun - 2026-11-05

This record closes the rerun that section 1.9 of [`maintainer-todo.md`](../../maintainer-todo.md) asked for. It repeats the archive steps of the [2026-11-04 run](acceptance-run-2026-11-04.md) on the build that holds the fix for AR-09 (`archive` copies less than it promises). It adds `--install-hook` and `associate --export`, because the review fixes in PR #36 touched both.

**Verdict: every step passed, and the archive now holds every file it promises.** All 56,570 expected files are in the archive, and each one matches its source by SHA-256. This run found no new defect.

| Fact | Value |
|---|---|
| Binary | `awt 1.0.0`, release build of `main` at `dd257b0` |
| Binary SHA-256 | `068971f321d631444ada7a92c579f996bcbb4d3d6864fe0730a34153b4efcad7` |
| Scratch home | Fresh copy from `scripts/new-scratch-home.ps1`: 103,298 files, 11.57 GB |
| Results | 4 of 4 before the stop, 32 of 32 after the resume, and 2 of 2 in the timing addition |

**The live home was not written to.** The harness refused any `--home` outside its scratch root, and it passed `--home` on every command. The run also hashed the live `settings.json` before and after, and the two hashes match. The scratch copy, the archives and the backup from `associate` were deleted afterward.

**The run happened in two parts.** Claude Code stopped the first attempt because the machine ran low on memory, during the incremental step. That stop was not a failure of the tool. The second part reused the same scratch copy and the same binary, and it re-checked the first sweep's output from disk.

## Archive coverage

The expected list came from walking the scratch copy independently of the tool. The script compared each expected file with its archive copy by hash, and it also checked every manifest hash.

| Category | Files expected | Missing | Different |
|---|---:|---:|---:|
| Top-level transcripts | 3,966 | 0 | 0 |
| Subagent transcripts | 10,562 | 0 | 0 |
| Other subagent files | 10,104 | 0 | 0 |
| `workflows/` | 983 | 0 | 0 |
| `tool-results/` | 3,684 | 0 | 0 |
| Other files in session folders | 77 | 0 | 0 |
| Other nested `.jsonl` files | 42 | 0 | 0 |
| `file-history/` | 25,079 | 0 | 0 |
| `session-env/` | 1,691 | 0 | 0 |
| `tasks/` | 363 | 0 | 0 |
| `todos/` | 19 | 0 | 0 |
| **Total** | **56,570** | **0** | **0** |

The 650 files under `memory/` were excluded, as designed. No `memory/` folder reached the archive.

## Steps

| # | Step | Want | Got |
|---|---|---|---|
| R-1 | `archive` with the archive folder equal to the home | Exit 2 | As wanted |
| R-2 | `archive` with the archive folder equal to `projects/` | Exit 2 | As wanted |
| H6 | `archive`, full sweep into an empty folder | Exit 0, nothing failed | `56570 copied, 0 skipped, 0 failed`, in 903 s |
| H6 | `archive`, again | Exit 0, nothing copied | `0 copied, 56570 skipped, 0 failed`, in 1,215 s |
| H9 | `archive --json` | The payload has `copied`, `skipped` and `failed` | As wanted, in 1,183 s |
| AR-09 | The manifest | It lists every expected file, and its hashes match | 56,570 entries. None missing, none with a bad hash |
| AR-09 | Temp files | None left in the archive | None |
| HOOK | `--install-hook` on the scratch home | One entry, with a 60-second timeout and an absolute path | As wanted |
| HOOK | Install again | Still one entry | As wanted |
| HOOK | `--hook-stdin` on the session with the most files, 1,939 of them | Under 60 s, and its session folder is archived | 17.1 s, `2140 copied, 0 skipped, 0 failed` |
| HOOK-B | `--hook-stdin` on the session with the most bytes, 0.2 GB in 552 files | Under 60 s | 7.5 s for the first run, and 0.6 s for a repeat |
| HOOK | `--uninstall-hook` | The entry is gone | As wanted |
| H7 | `associate` into an existing key | Exit 2, and no export written into the target | As wanted |
| H7 | `associate` from a gone folder to a fresh target | Exit 0, an export with a manifest, and no `memory/` | Exit 0, 5 changes, 14 exported files |
| H7 | `verify` after that `associate` | Exit 0 | As wanted |

HOOK-B was added after the main script finished. The script chose its largest session by file count, and copy time also depends on bytes, so that choice alone did not prove the 60-second claim.

## What the timings show

**A repeat of a full sweep costs more than the first one on this data.** It took 1,215 s against 903 s for the copy. The likely reason is that a repeat reads and hashes both the source and the archive for every file (inferred from the code, not profiled). `docs/reference/commands.md` already says that a rerun is not free. This is a cost to know about, not a defect.

**A hook run does not have that cost.** It handles only its own session, which is why the repeat above took 0.6 s.

## What this run does not prove

- **A slower disk.** Both timings under the 60-second limit come from one internal drive on one machine. A session on a slower disk or a network folder could take longer. A hook run cut off at the limit should leave no partial file under its final name, because writes go through a temp file and a rename (read in the code, not forced in this run). It could leave an `.awt-tmp` file behind, which section 1.9 of [`maintainer-todo.md`](../../maintainer-todo.md) lists as a follow-up.
- **A file held open by a running session.** No session was running against the scratch copy, so `0 failed` says nothing about locks. `crates/awt-cli/tests/archive_locked_file.rs` covers that case.
- **The steps that move a project.** This run did not repeat them. PR #36 changed `archive.rs`, `associate.rs`, `settings.rs`, a test double in `fs.rs`, and the archive paths of the CLI, so the 2026-11-04 results for `apply` and `rollback` still apply (inferred from that file list).
- **AR-10.** A project folder with no readable transcript still makes `apply` and `associate` exit 3 and roll back. 46 folders in this scratch copy have that shape. The `associate` source in this run did not have that shape, so its exit 0 says nothing about AR-10. This was true of this run's build. [A later record](acceptance-run-2026-11-05-ar10.md) covers the fix.
