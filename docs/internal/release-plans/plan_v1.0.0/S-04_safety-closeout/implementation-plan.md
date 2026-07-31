---
id: S-04
title: "Implementation plan: v1 safety closeout"
type: implementation-plan
status: in-progress
created: 2026-07-30
updated: 2026-07-30
phases-complete: [17.1, 17.2, 17.3, 17.4]
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
| 17.2 | Whole-tree rollback for directory renames: recursive snapshot (every file, not top-level `*.jsonl`), rollback renames the directory back BEFORE restoring modified files, no delete step; refuses loudly if both old and new dirs exist; tree-map regression tests incl. nested sidecars, binary files, injected mid-apply failure, and a real-filesystem end-to-end via the binary | AC-54 | **Complete 2026-07-30** |
| 17.3 | Source-existence guard in `build_plan` (`SourceMissing`, exit 2, ordered AFTER dest-exists so AC-19's idempotency signal is preserved); fatal missing `MoveTree` source in apply; `verify` asserts destination-present and source-absent whenever the manifest records a folder move; associate (move_folder=false) explicitly exempt | AC-55 | **Complete 2026-07-30** |
| 17.4 | Settings fail-closed: only `NotFound` initializes; read/parse/UTF-8/non-object failures propagate as exit 4 with the file untouched; atomic tmp+rename write; unrelated-key preservation test | AC-56 | **Complete 2026-07-30** |
| 17.5 | Failure-injecting `FileSystem` double; read errors become errors across plan/backup/apply/verify; optional-root policy documented in one place | AC-59 | Not started |
| 17.6 | Plan-derived verification: scope-aware, destination-key presence, folder postcondition, malformed-line and read-error failures | AC-57 | Not started |
| 17.7 | Plugin hash from recorded `cwd` via `ProjectIndex`; case/separator variant tests | AC-60 | Not started |
| 17.8 | Path confinement: canonicalize hook transcript paths, reject traversal, refuse reparse points in mutated/archived trees; Windows junction tests | AC-61 | Not started |
| 17.9 | Surface reduction: remove `keep-dest`/`keep-src`, `--recursive`, and `minimal`/`full` scopes from the CLI (default decision per AC-58); docs and reference updated; refusal tests for nested projects | AC-58 | Not started |
| 17.10 | Synthetic fixture replacement: procedural generation of the shapes and counts the golden tests need; re-lock counts; remove real transcripts from the working tree (history decision is D10, maintainer) | AC-62 | Not started |
| 17.11 | Adversarial acceptance run: revised matrix (missing source, sidecar project, malformed settings, junction, invalid UTF-8, case-variant plugin path) against a fresh scratch copy; dated report | gate (g) | Not started |

Medium follow-ups AC-63..AC-65 are scheduled after 17.11 or into v2 prework, whichever comes
first; they do not gate the tag.

## Test map (phases complete so far)

| Test | AC |
|------|----|
| `rollback_restores_the_complete_tree_including_unbacked_sidecars` | AC-54 |
| `auto_rollback_after_midapply_failure_preserves_sidecars` | AC-54 |
| `apply_then_rollback_restores_sidecars_on_the_real_filesystem` (binary, real FS) | AC-54 |
| `plan_refuses_when_the_source_folder_does_not_exist` | AC-55 |
| `associate_still_plans_without_a_source_folder` | AC-55 (guard scoped correctly) |
| `apply_hard_fails_if_the_source_vanishes_between_plan_and_apply` | AC-55 |
| `verify_fails_when_the_folder_move_did_not_actually_happen` | AC-55 |
| `set_retention_refuses_malformed_settings_and_touches_nothing` | AC-56 |
| `set_retention_refuses_invalid_utf8_settings` | AC-56 |
| `set_retention_refuses_a_non_object_root` | AC-56 |
| `install_hook_refuses_malformed_settings_and_touches_nothing` | AC-56 |
| `uninstall_hook_refuses_malformed_settings_and_touches_nothing` | AC-56 |
| `a_missing_settings_file_still_initializes` | AC-56 (over-correction guard) |
| `settings_writes_preserve_unrelated_keys` | AC-56 |
| `invalid_utf8_is_refused_and_nothing_is_written` | AC-53a |

## Decision dependencies

- **AC-58 removals** proceed on the spec's default (remove) unless the maintainer chooses
  implementation instead. Removal is reversible; shipping inert safety flags is not honest.
- **AC-62 / D10**: the engineering half (synthetic fixtures) does not wait on the publication
  decision; the history-rewrite half cannot proceed without it.

## Exit criteria

- Every Critical and High AC has a red-first regression test and is green.
- The adversarial acceptance run passes with a dated report.
- Hygiene gate (g) flips to PASS; S-01 sign-off then proceeds against accurate evidence.
