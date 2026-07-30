---
id: S-04
title: "Implementation plan: v1 safety closeout"
type: implementation-plan
status: in-progress
created: 2026-07-30
updated: 2026-07-30
linked-spec: ./spec.md
target-release: v1.0.0
ac-coverage: complete
---

# Implementation plan: v1 safety closeout

Executed test-first, red-green per phase. Every phase opens with failing tests that reproduce
the defect, then the fix, then raw-byte or tree-level assertions. Estimated 7-10 engineering
days end to end.

## Phase order and rationale

Ordered by blast radius: the rollback defect can destroy data during the *recovery* path, so it
goes first; the surface reductions go late because deleting a flag is cheap once semantics are
settled; the adversarial acceptance run is last because it certifies the sum.

| Phase | Work | AC | Status |
|-------|------|----|--------|
| 17.1 | Repair refuses invalid UTF-8 | AC-53a | **Complete** (landed with the spec: strict `from_utf8` in `build_repair_plan`, red-then-green test, docs updated) |
| 17.2 | Whole-tree rollback for directory renames: manifest records `from`/`to`, rollback renames the directory back before restoring modified files; tree-hash regression tests incl. sidecars, binaries, injected apply/verify failures | AC-54 | Not started |
| 17.3 | Source-existence guard in `build_plan`; fatal missing `MoveTree` source in apply; `verify` asserts source-absent and destination-present after a folder move | AC-55 | Not started |
| 17.4 | Settings fail-closed: only `NotFound` initializes; all other read/parse failures propagate as exit 4; atomic write; malformed/invalid-UTF-8/non-object-root/read-denied tests | AC-56 | Not started |
| 17.5 | Failure-injecting `FileSystem` double; read errors become errors across plan/backup/apply/verify; optional-root policy documented in one place | AC-59 | Not started |
| 17.6 | Plan-derived verification: scope-aware, destination-key presence, folder postcondition, malformed-line and read-error failures | AC-57 | Not started |
| 17.7 | Plugin hash from recorded `cwd` via `ProjectIndex`; case/separator variant tests | AC-60 | Not started |
| 17.8 | Path confinement: canonicalize hook transcript paths, reject traversal, refuse reparse points in mutated/archived trees; Windows junction tests | AC-61 | Not started |
| 17.9 | Surface reduction: remove `keep-dest`/`keep-src`, `--recursive`, and `minimal`/`full` scopes from the CLI (default decision per AC-58); docs and reference updated; refusal tests for nested projects | AC-58 | Not started |
| 17.10 | Synthetic fixture replacement: procedural generation of the shapes and counts the golden tests need; re-lock counts; remove real transcripts from the working tree (history decision is D10, maintainer) | AC-62 | Not started |
| 17.11 | Adversarial acceptance run: revised matrix (missing source, sidecar project, malformed settings, junction, invalid UTF-8, case-variant plugin path) against a fresh scratch copy; dated report | gate (g) | Not started |

Medium follow-ups AC-63..AC-65 are scheduled after 17.11 or into v2 prework, whichever comes
first; they do not gate the tag.

## Decision dependencies

- **AC-58 removals** proceed on the spec's default (remove) unless the maintainer chooses
  implementation instead. Removal is reversible; shipping inert safety flags is not honest.
- **AC-62 / D10**: the engineering half (synthetic fixtures) does not wait on the publication
  decision; the history-rewrite half cannot proceed without it.

## Exit criteria

- Every Critical and High AC has a red-first regression test and is green.
- The adversarial acceptance run passes with a dated report.
- Hygiene gate (g) flips to PASS; S-01 sign-off then proceeds against accurate evidence.
