# Acceptance run for the AR-10 fix - 2026-11-05

This record closes section 1.10 of [`maintainer-todo.md`](../../maintainer-todo.md). The maintainer chose to fix AR-10 (a project folder with no readable transcript could not be moved) before the tag. The fix changes how the mover decides which folder belongs to a project, so the move steps had to be repeated. The earlier records stay as written: [2026-11-04](acceptance-run-2026-11-04.md) and the [archive rerun](acceptance-run-2026-11-05.md).

**Verdict: every step passed on the final build.** The synthetic attack matrix and the move matrix both passed unchanged. On the real shapes in a copy of the maintainer's home, every folder that could move arrived byte-identical.

| Fact | Value |
|---|---|
| Binary | `awt 1.0.0`, release build of branch `fix/ar-10-folder-without-transcript` at `648af53` |
| Binary SHA-256 | `d02ae894c16d0b0598cad66099997671c087d4645ec6ee61ec319395be58285f` |
| Scratch home | Fresh copy from `scripts/new-scratch-home.ps1` for each run: 105,404 files, 11.61 GB |
| Results | 60 of 60 on synthetic homes. 52 of 52 on the move matrix. 29 of 29 on the real shapes |

**The live home was not written to.** The harness refused any `--home` outside its scratch root, and it passed `--home` on every command. Backups stayed inside the scratch root. The real-shapes run also hashed the live `settings.json` before and after, and the hashes match. `apply` did move one real folder, the disposable `E:\tmp\chain-smoke` used by earlier runs. That folder ended byte-identical at its original path. The scratch data was deleted afterward.

**The archive steps were left out of the move matrix.** `archive.rs` did not change in this fix, and those steps passed in full in the [archive rerun](acceptance-run-2026-11-05.md).

## The real shapes

The copy held 185 project folders, and 47 of them had no top-level transcript. That is the shape AR-10 was about.

| Shape | Folders |
|---|---|
| Holds only `memory/` | 25 |
| Holds session folders with no top-level transcript | 22 |

The run looked for a known path for each one, in `.claude.json`, in `awt list` and in `history.jsonl`.

| Outcome | Folders | What happened |
|---|---|---|
| One known path, and `associate` moved it | 7 | Exit 0. Every file arrived unchanged, and the old folder is gone |
| One known path, and `associate` refused | 1 | Exit 2, and nothing changed. Another folder's transcripts record this path among two live candidates, so the existing ambiguity guard (AC-7) is correct |
| No known path | 39 | Not attempted. See below |

**Rollback was byte-identical** for the first folder of each shape, one holding only `memory/` and one holding session folders.

**The 39 folders with no known path are a real limit.** Nothing records what path they belonged to, and the encoding cannot be reversed. If the project still exists on disk, `awt apply --src <path>` finds its folder, because the source folder proves the path is real. If the project is gone, `associate` refuses with exit 2. That refusal is deliberate: without it, a typo could take another project's folder. An earlier estimate that 46 folders "could not be moved" overstated what this fix can reach.

## The reviews

An independent reviewer ran scenario scripts and mutations against each version of the branch.

| Round | Result |
|---|---|
| First version | Would not merge. The fix turned a safe failure into a silent success, in which a typo could take another project's folder and evidence the tool could read was ignored |
| Second version | Would not merge. It fixed that but added two regressions. One left a state folder behind on the next move. The other let a typed case variant exit 0 with a stale transcript |
| Third version | This build. Would merge, with no blockers. All six findings from the second round are fixed. Its one new regression fails safely with a clean rollback |

The reviews also found older defects on the same path, and this branch fixes them. The most serious was that a trailing separator or forward slashes on `--src` could pass verification with state left behind. Section 5 of [`maintainer-todo.md`](../../maintainer-todo.md) lists them with what stays open.

The final round also ran 88 mutations and caught 71. Four of the 17 survivors marked behaviors no test pinned, and this branch adds tests for them after this build, without changing the engine: a nested rival must be a look-alike, an empty folder at the old name passes verification, a refused rollback says the move is still in place, and rollback stays off when asked. Each new test was run against its mutation and caught it. The other 13 are not closed, and the largest group is that only `apply` tests the path clean-up in the CLI.

## What this run does not prove

- **An owner that nothing records.** If `E:\foo_bar` exists and no file records it, moving `E:\foo-bar` can take a memory-only folder that was really `foo_bar`'s. awt cannot list every look-alike on disk. It warns whenever a folder name was the only link.
- **A real session running during a move.** FF-1 (the live-session guard misses terminal sessions) still applies, and the guard fix ships in v1.0.1.
- **A rename on a live home.** Every state write went to a copy.
