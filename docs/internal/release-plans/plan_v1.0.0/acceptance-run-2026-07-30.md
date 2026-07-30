---
title: "Manual acceptance run - 2026-07-30"
type: acceptance-run
release: v1.0.0
commit: e50eba2 (main)
result: PASS
findings: []
supersedes: acceptance-run-2026-07-28.md
---

# Manual acceptance run - 2026-07-30

**Result: PASS.** Every step of [`docs/acceptance-run.md`](../../../acceptance-run.md) completed
against a fresh scratch copy of the maintainer's live Claude home, on `main` at `e50eba2`. No new
defects. This is the clean end-to-end run that the [2026-07-28
run](acceptance-run-2026-07-28.md) and its four fixes were working toward, and it satisfies
hygiene gate (f).

Re-run from scratch rather than continuing the previous scratch home: that one had been mutated by
the fix-verification work (an `associate` run, a `repair --apply`, and injected test entries), so
it could no longer answer "does this work on untouched real data".

## Environment

| Item | Value |
|---|---|
| Commit | `e50eba2`, `main`, CI green |
| Binary | `target/release/awt.exe`, `awt 1.0.0` |
| Scratch home | `E:\Projects\_temp\awt-acceptance-2026-07-30` (56,761 files, 3.39 GB) |
| Created by | `scripts/new-scratch-home.ps1` |
| Copy fidelity | `history.jsonl` SHA-256 identical to live |
| Live home | Never written. Verified after the run: `.claude.json` still parses, and all 3,121 damaged `history.jsonl` lines are still present, so `repair` demonstrably ran only against the copy. |

**A note on `.claude.json` fidelity.** The copy's `.claude.json` is *not* byte-identical to live,
and that is expected rather than a copy fault: Claude Code rewrites that file continuously
(`lastCost`, `lastSessionId`, `lastDuration`, and so on), and the live file's mtime was four
minutes after the copy was taken. The copy parses cleanly with 79 `projects` keys and 39
`githubRepoPaths` entries. A file under active write cannot be snapshotted byte-identically, which
is one of the reasons the acceptance run works on a copy in the first place.

## Results

| # | Step | Result | Detail |
|---|---|---|---|
| 1 | `doctor` (cold) | PASS | exit 0, 134.7s. 75 stale, 1 report-only, 15 unresolved, 0 warnings |
| 2 | `doctor` (warm) | PASS | 14.1s |
| 3 | `list` | PASS | exit 0, 3.1s, 58 rows: 38 ok, 5 stale, 15 unresolved |
| 4 | `scan` | PASS | exit 0, correct hits |
| 5 | `plan` | PASS | exit 0, 6 changes. **One** coalesced `json array` entry for two slugs sharing a value (AR-04), rendered with the escaped anchor (AR-01) |
| 6 | `apply` | PASS | exit 0, 6 changes, folder moved, `report.json` written |
| 7 | `verify` | PASS | exit 0, all 6 checks `[ok]`. Both `githubRepoPaths` slugs rewritten; `scan` on the old path returns 0 |
| 8 | `rollback` | PASS | exit 0, **5/5 byte-identical**, folder restored, all 4 references back |
| 9 | `verify` after rollback | PASS | exit 3 with FAIL lines - the expected confirmation |
| 10 | `archive` | PASS | exit 0, 9,974 files in 55.8s |
| 11 | `archive` re-run | PASS | exit 0, 0 copied / 9,974 skipped in 15.2s - content-hash dedup |
| 12 | `associate` (gone folder) | PASS | exit 0, 3 changes; all 4 references relocated (AR-02) |
| 13 | `repair --drive-letter` | PASS | dry run wrote nothing; `--apply` repaired 2,303 lines across 34 values; line count unchanged; 818 declined lines remain; second `--apply` a no-op |
| 14 | `--json` | PASS | valid JSON from all six subcommands (AR-03) |
| 15 | Guards | PASS | every exit code matches the contract |

### Exit-code contract, verified

| Condition | Expected | Actual |
|---|---|---|
| destination exists | 2 | 2 |
| cross-volume move | 2 | 2 |
| `repair` with no repair selected | 2 | 2 |
| `associate` on a path with no state | 4 | 4 |
| `verify` on a move that never happened | 3 | 3 |
| `doctor` on a healthy path | 0 | 0 |

### The four earlier findings, re-confirmed fixed on real data

| ID | Confirmation in this run |
|---|---|
| AR-01 (JSON escaping) | Step 6 applied a `githubRepoPaths` rewrite that previously failed at `expected 1, live 0` |
| AR-02 (transcript-less associate) | Step 12 re-associated a gone project with no surviving transcripts |
| AR-03 (`--json` ignored) | Step 14: `plan` and `verify` both emit JSON, and `verify` still exits 3 |
| AR-04 (duplicate values) | Step 5 planned **one** coalesced change for two slugs; step 7 confirmed both rewritten |

## Observations (not defects)

**Warm `doctor` was 14.1s, against 5.9s on 2026-07-28.** Same machine, a slightly larger tree
(56,761 vs 55,413 files). The difference is larger than the file-count delta explains, so it is
most likely OS cache state rather than a regression - this run had just finished a 3.4 GB copy and
a 134.7s cold sweep. Worth re-measuring on a quiet machine before treating the ~8s figure in
`ROADMAP.md` as authoritative. Not a blocker; `doctor` is read-only and its cost is bounded by the
tree size.

**`warnings: 0` on real data.** The declined-shape channel added for decision 2 reports nothing
here, because the maintainer's real `.claude.json` has no wrong-typed `githubRepoPaths` values.
That is the correct outcome, and it is also why the behavior is covered by unit tests rather than
resting on this run.

**The 12 unrepairable history values persist by design.** They resolve on no drive, so there is
nothing to repair them *to*. Their 818 lines remain damaged and reported. One of them
(`::\Backup - Data\:choesGPT\...`) carries a second corrupted segment that a drive-prefix rule
cannot fix, which is exactly the boundary D8 draws.

## Verdict

Hygiene gate (f) **PASSES**: the documented acceptance sequence completes cleanly on real data.

**Update, later on 2026-07-30 - partial retraction.** This report originally concluded that no
technical gate remained for the v1.0.0 tag. That conclusion was wrong, and it is retracted here
rather than silently rewritten. An adversarial code audit the same day found data-loss and
false-success paths that this sequence never exercises: rollback of unbacked sidecar files, a
missing source folder reported as a successful move, settings fail-open overwrite, and more.
Those findings were independently verified against source and are now the S-04 safety closeout
([S-04_safety-closeout/spec.md](S-04_safety-closeout/spec.md)), hygiene gate (g), blocking the
tag alongside the S-01 sign-off. The lesson, recorded so it is not relearned: a passing run
proves the exercised path, and this sequence exercises the happy path. The next acceptance
revision adds an adversarial matrix (S-04 phase 17.11).

## Cleanup

The scratch home, backups, and archive were deleted on 2026-07-30 after this report merged.
The S-04 adversarial run (phase 17.11) will create a fresh copy with
`scripts/new-scratch-home.ps1`.
