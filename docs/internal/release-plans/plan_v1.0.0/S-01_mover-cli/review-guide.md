---
type: review-aid
title: "S-01 sign-off reading aid"
spec: ./spec.md
evidence: ./ac-traceability.md
created: 2026-07-30
updated: 2026-07-30
---

# S-01 sign-off reading aid

A companion for the human review that flips [`spec.md`](spec.md) from `draft` to `committed`.
It restates each acceptance criterion in plain language, says where its evidence lives, and
flags what currently contests it. It deliberately takes no position on whether you should sign.

## How to use this

Your review answers one question the tests cannot: **do these criteria describe the guarantees
you actually want from a tool that edits your irreplaceable data?** The suite passed at 110
tests while two release blockers were live, and passed at 143 while three data-loss paths were
open - not because tests lied, but because criteria bound what gets tested. Read each row
asking "is this the promise I want?", not "did this pass?".

Standing column vocabulary:

- **Proven** - a test directly asserts the outcome (per [ac-traceability.md](ac-traceability.md)).
- **Test-thin** - behavior present; the proving test is incomplete. Accepted as-is for v1.0
  unless you object.
- **Contested** - the S-04 safety closeout
  ([spec](../S-04_safety-closeout/spec.md)) has an open finding that cuts into this guarantee.
  Signing before the closeout means signing a promise the code does not currently keep.

## The criteria

| AC | The promise, in plain words | Evidence | Standing |
|---|---|---|---|
| AC-1 | A same-volume move is a rename: fast, atomic, and the old folder is gone afterwards | cross-volume guard + test | **Contested by AC-55**: if the source folder does not exist, apply currently *skips* the move, reports success, and exits 0 - so "the move completes" is not yet always true when the tool says it is |
| AC-3 | If something already exists at the destination, nothing happens at all | guard + exit-2 test | Proven for the default. **Contested by AC-58**: choosing `keep-dest`/`keep-src` silently bypasses this guard without implementing anything |
| AC-4 | A git worktree is never moved without an explicit override | guard test | Test-thin (the `--force` override path itself is untested) |
| AC-5 | The plan lists every store that references the project - nothing is touched that was not listed | per-store tests | Test-thin (no single test seeds all stores at once) |
| AC-6 | History is matched to a folder by what the transcripts *say*, never by folder-name guessing | index tests | Proven |
| AC-7 | When history could belong to two projects, the tool stops and makes you choose | fail-closed test (decision 7a) | Proven |
| AC-8 | The encoded transcript directory is renamed; the old one is gone | apply test | Test-thin (old-dir-gone not asserted) |
| AC-9 | The tool's path encoding is Claude Code's real one, verified against real directories | encoding test vs real dirs | Proven |
| AC-10 | After a move, every transcript `cwd` points at the new path and none at the old | golden end-to-end test | Proven |
| AC-11 | Path rewrites are surgical: anchored, never bare substring | golden counts test | Proven |
| AC-12 | Things that merely *look* like the project name (package names, branches, prose) come through byte-identical | golden counts test | Proven |
| AC-13 | Every variant key in `.claude.json` migrates; the file still parses; entry count unchanged | detect/plan tests + escaping suite | Test-thin (no apply-then-readback integration test), though the AR-01/AR-04 regression suite now covers the raw-byte layer |
| AC-14 | `history.jsonl` entries follow the project to its new path | detect/plan tests | Test-thin (same gap) |
| AC-15 | The dry run shows everything and writes nothing | byte-identical-after-plan test | Proven |
| AC-16 | Before the first write, a backup exists containing the original of every file the run will modify | sha256 snapshot test | **Contested by AC-54**: the promise reads "every file the run will modify", and holds for those - but rollback *deletes* files the run never modified and never backed up (sidecars in a renamed directory). Decide whether this criterion says what you mean: is the unit of protection the *file touched* or the *tree affected*? |
| AC-17 | Rollback puts everything back the way it was | restore tests + byte-identity proof (AC-17v) | **Contested by AC-54**: restores what was backed up, byte-proven - and can destroy unbacked sidecars in the same directory while reporting success. The 5/5 proof's denominator is the manifest, not the tree |
| AC-18 | Verify passes only if every promised postcondition actually holds on disk | pass-case test | **Contested by AC-57**: verify checks old-key absence but not new-key presence, ignores the folder move, skips malformed history lines, and treats some read errors as absence - so a green verify does not yet check everything this criterion promises |
| AC-19 | Running apply twice is safe: the second run refuses (exit 2), which is the "already done" signal | behavior + decision 19b | Proven |
| AC-20 | An unrecognized store shape stops everything before any write | state-untouched test | Proven |
| AC-21 | A live Claude Code process blocks the run unless you force it | lock-detection test | Proven |
| AC-22 | Every apply leaves a machine-readable record of what happened | report.json tests | Proven |
| AC-23 | plan / apply / verify / rollback all exist as commands | CLI tests | Test-thin (verify/rollback tested via library, not binary) |
| AC-24 | Exit codes are distinct and scriptable | exit-code mapping test | Proven |
| AC-26 | Zero LLM and zero network, structurally enforced | lockfile deps-guard test | Proven |

## What is NOT in these criteria

Worth noticing during the read, because a sign-off blesses the boundary as well as the content:

- **No criterion covers settings writes** - which is where the fail-open overwrite lives
  (S-04 AC-56). S-01 predates `archive`/`associate`/`repair`; their criteria live in S-02/S-03.
- **No criterion demands the source folder exist** before a move is planned (S-04 AC-55).
- **No criterion constrains I/O failure behavior** (error versus silently-empty scan;
  S-04 AC-59).
- The `--recursive` and scope flags are not promised by any S-01 criterion - which is exactly
  why removing them (AC-58) needs no spec amendment here.

The closeout spec turns each of these into its own criterion rather than stretching S-01's.

## Open questions the spec itself records

- **OQ-1 (format-version policy):** sniff shapes versus pin observed versions. Backstopped by
  AC-20 today; fine to leave open past v1.0 if you say so explicitly.
- OQ-2 (attribution default) was resolved as decision 7a - fail closed, resolver deferred.

## Mechanics of signing

When - and only when - you are satisfied, and preferably after the S-04 closeout lands:

1. Tick the 24 fulfillment checkboxes in [`spec.md`](spec.md) against the table above.
2. Flip frontmatter `status: draft` to `status: committed`; drop
   `requires-human-review: true`.
3. Note the date and anything you overrode in the spec's history or the release plan.
4. Re-check the hygiene gates in [`plan_v1.0.0.md`](../plan_v1.0.0.md) - gate (a) should flip.
