# Adversarial acceptance run - 2026-07-31

Phase 17.11 of [S-04 (safety closeout)](S-04_safety-closeout/spec.md): the run that flips
hygiene gate (g). Unlike the happy-path run of
[2026-07-30 (gate f)](acceptance-run-2026-07-30.md), this run's matrix is hostile by design:
every input is chosen to make the tool fail wrong if it is going to fail at all.

**Verdict: PASS on Run 2** (fixed binary, fresh scratch copy), after Run 1 found four
findings (AR-05 through AR-08, all fixed same-day, red-first). Findings first, evidence
below.

- Binary: `awt 1.0.0`, release build. Run 1 at `main` @ `69ace60`; Run 2 on the same tree
  plus the AR-05..AR-08 fixes (committed with this report).
- Scratch home: `E:\Projects\_temp\awt-acceptance-2026-07-31` via
  `scripts/new-scratch-home.ps1` (57,243 files, 3.41 GB). Fresh copy per run; fidelity
  verified each time (history.jsonl SHA-256 equal to live; `.claude.json` parses,
  byte-length equal to live).
- Adversarial mini-home: `E:\Projects\_temp\awt-adv-mini`, synthetic, rebuilt per run by a
  seeder script (transcript + history + plugin-state dir keyed by
  `sha256(recorded cwd)[:16]`).
- Nothing in either run touched the live `~/.claude`; every command ran with `--home`
  pointed at a scratch copy.

## Findings (Run 1)

### AR-05 (rollback report panic) - defect, fixed

`apply` prints `report: ...report.json` and rollback's only input flag is named
`--report`. Handing rollback that exact file panicked at `rollback.rs:88`
(`Option::unwrap()` on a missing `src_abs`), exit 101. The natural invocation, the one the
tool's own output suggests, was the one that crashed; nothing was restored.

Root cause: `rollback` assumed every input is a backup manifest; `report.json` is a
different shape (`applied`/`verify`/`backup_dir`) sitting next to `manifest.json` in the
same backup directory. Wrong-shape JSON must refuse with exit 4 per the exit-code
contract, and this particular wrong shape should not refuse at all, because the tool
itself pointed the user at it.

Fix (red-first, `crates/awt-cli/tests/acceptance_ar05_ar07.rs`):

- `resolve_manifest_path` in `awt-core/src/rollback.rs`: a manifest passes through; an
  apply report dereferences to its sibling `manifest.json`; anything else refuses with
  `UnrecognizedFormat` (exit 4).
- Every `unwrap()` on manifest fields in `rollback()` replaced with the same exit-4
  refusal, so a direct core caller cannot panic either.
- The `--report` help text now says "manifest.json or report.json".

### AR-06 (stale --force help) - help-text drift, fixed

`--force` help read "Allow overwriting a destination that already exists". AC-58
(2026-07-30) made destination collisions refuse unconditionally; force now lifts exactly
two guards (worktree source, live IDE lock). The help text advertised a bypass that does
not exist. Now reads: "Proceed despite a worktree source or a live IDE lock (collisions
still refuse)".

### AR-07 (archive ignores --json) - defect, fixed

`archive --json` advertised machine-readable output and emitted the text summary line.
This is the AR-03 defect class from the 2026-07-28 run (--json ignored), missed on the one
subcommand the 07-28 sweep did not cover. All three archive paths (`--hook-stdin`,
`--session`, full sweep) now emit `{"copied": n, "skipped": n}` under `--json`; test
locks the shape.

### AR-08 (no-state misclassified as exit 4) - contract fix

`associate --from <path with no recorded state>` exited 4 (`UnrecognizedFormat`). The
exit-code contract reserves 4 for store bytes the tool cannot parse; "nothing recorded for
this path" is a guard refusal (exit 2): the input was understood, there is nothing to act
on. Both sites (`associate.rs`, `archive.rs::archive_project`) now raise the exit-2 class
with the same actionable message. Two core tests pin the class.

Run 1's other 20 steps passed; every finding above was re-exercised in Run 2.

Suite grew 175 to 180 (three AR-05/AR-07 CLI tests, two AR-08 core tests); fmt, clippy
clean.

## Run 2 (fixed binary, fresh scratch copy): all steps

### Happy-path core

| # | Step | Result |
|---|------|--------|
| H1 | `doctor` cold / warm | exit 0, 79.8s / 3.1s (Run 1 on a cold OS cache: 266.9s / 153.9s) |
| H1 | `list` | exit 0, 2.5s, 60 projects, chain-smoke row healthy |
| H1 | `scan` on `E:\tmp\chain-smoke` | exit 0, finds the project dir |
| H2 | Injection | forward-slash `projects` key + two `githubRepoPaths` slugs sharing one backslash value, inserted as raw text at the anchors; file still parses |
| H3 | `plan` | exit 0; both injected shapes picked up; the shared duplicate literal appears as ONE coalesced `json array` splice (AR-04 behavior holding on real data) |
| H3 | `apply` | exit 0, 6 changes, folder moved |
| H3 | `verify` | exit 0, 7/7 checks including plan-derived splice checks |
| H2b | Post-move duplicate check | fwd-slash key moved, old key gone; BOTH `githubRepoPaths` slugs rewritten |
| H4 | `rollback --report <report.json>` (the AR-05 path, on purpose) | exit 0, **11/11 restored byte-identical**, including `subagents/` sidecar files the AC-54 recursive snapshot exists for; folder renamed back; `.claude.json` and `history.jsonl` hash-identical to pre-move |
| H5 | `verify` after rollback | exit 3 with 2 FAILs (moved state correctly reported gone) |
| H6 | `archive` full sweep | exit 0, 10,344 copied, 98.2s |
| H6 | `archive` incremental re-run | exit 0, 0 copied / 10,344 skipped, 3.4s |
| H7 | `associate` into existing key | exit 2 refusal (AC-58 contract: collisions refuse unconditionally) |
| H7 | `associate` gone folder -> fresh target | exit 0; 12 history lines re-homed; projects key and both `githubRepoPaths` values spliced |
| H8 | `repair --drive-letter` dry / apply / re-run | 34 values / 2,303 lines repaired; re-run finds nothing; leftovers exactly 12 distinct / 818 lines (the D7 known set; tripwire quiet, no new `::` values) |
| H9 | `--json` sweep, all seven emitting commands | list, scan, plan, repair, archive, verify (exit 3 with payload), doctor: every payload parses; archive now `{"copied","skipped"}` |

### Adversarial matrix (mini-home + real filesystem)

| # | Attack | Want | Got |
|---|--------|------|-----|
| A1 | `repair` on history.jsonl with invalid UTF-8 bytes | exit 4, file untouched | exit 4, hash unchanged |
| A2 | `plan` with lowercase/forward-slash src spelling vs plugin dir hashed from recorded spelling | plugin dir found | found (AC-60 recorded-spelling hashing) |
| A3 | `plan` with missing source folder | exit 2 | exit 2 |
| A4 | Removed flags `--recursive`, `--scope`, `--on-collision` | clap usage error, exit 2 | exit 2, all three |
| A5 | `archive --hook-stdin` with `..`-escape transcript_path | exit 4, nothing archived | exit 4, archive dir empty |
| A6 | Real junction (`New-Item -ItemType Junction`) inside project state dir: `plan` | exit 2 (refuse at plan time) | exit 2 |
| A6 | Same junction: `archive` sweep | exit 0, junction skipped, target not pulled in | exit 0, transcript archived, secret.txt NOT in archive |
| A7 | Nested project under src | exit 2, names the child | exit 2, child named |
| A8 | Malformed `settings.json` + `archive --install-hook` | exit 4, file byte-identical, no temp residue | exit 4, hash unchanged, no `.awt-tmp` |
| A9 | Destination exists | exit 2 | exit 2 |
| A10 | Cross-volume destination | exit 2 | exit 2 |
| A11 | `repair` with no selection flag | exit 2 | exit 2 |
| A12 | `associate` from a never-recorded path | exit 2 (AR-08 fix) | exit 2 |
| R1 | `rollback --report` on wrong-shape JSON | exit 4, message names both accepted shapes | exit 4 |
| R2 | `rollback --report` on the report.json apply prints | full restore | 11/11 byte-identical (H4 above) |
| R3 | `archive --json` | valid JSON | valid |

Note on Run 1's A12: it appeared to pass with exit 2, but that was a clap usage error from
wrong flag names in the harness, not the tool refusing. Re-run with correct flags exposed
the exit-4 misclassification that became AR-08 (no-state misclassified). Harness errors
can masquerade as passes; exit codes were re-verified against stderr text in Run 2.

## What this run proves that 07-30 did not

- The AC-54 (whole-tree rollback) rename-back restores sidecar files under real Windows
  rename semantics on real data, byte-identical, via the invocation a user would actually
  type.
- The AC-61 (path confinement + junctions) policy holds against a real `mklink`-class
  junction: mutation refuses at plan time (exit 2), archive skips the subtree and archives
  the rest.
- The AC-56 (settings fail-closed) refusal leaves malformed settings byte-identical with
  no temp-file residue.
- The AC-53a (strict UTF-8 repair) refusal holds on actual invalid bytes.
- The exit-code contract survives an adversarial sweep: every refusal lands on its
  documented code (after AR-08 (no-state misclassified)).

## Gate (g) consequence

With this run PASS, every item in gate (g) is closed: AC-53a and AC-54 through AC-62, plus
the adversarial run. The remaining tag blockers are the maintainer-owned ones: the D10
fixture read and rewrite decision, and the S-01 sign-off (gate a).

Cleanup: scratch copies and mini-homes under `E:\Projects\_temp\` and the disposable move
targets under `E:\tmp\` were deleted after the run.
