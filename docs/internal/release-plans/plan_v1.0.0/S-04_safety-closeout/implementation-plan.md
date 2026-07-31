---
id: S-04
title: "Implementation plan: v1 safety closeout"
type: implementation-plan
status: in-progress
created: 2026-07-30
updated: 2026-07-30
phases-complete: [17.1, 17.2, 17.3, 17.4, 17.5, 17.6, 17.7, 17.8, 17.9, 17.10]
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
| 17.5 | Failure-injecting `FailingFs` test double; `walk_files_strict` for backup and merge-apply (a read_dir failure aborts before any write); `read_dir_optional` (missing root = empty, real error = error) in plugin detect/audit/verify; verify hard-errors treated like failed checks in `apply_verified` (rollback, not strand); projects verify reports unreadable dirs instead of counting zero | AC-59 | **Complete 2026-07-30** |
| 17.6 | Plan-derived verification: `verify` takes the applied plan and asserts every planned json splice landed (destination anchor present, source anchor gone, raw bytes); malformed `history.jsonl` lines are failures, not skips; folder postconditions landed with AC-55. Scope-awareness deliberately NOT plumbed: the `minimal`/`full` tiers are slated for removal under AC-58 (17.9), after which Standard-only verification is scope-correct by definition | AC-57 | **Complete 2026-07-30** (scope caveat resolves via 17.9) |
| 17.7 | Plugin hash derived from every recorded `cwd` spelling that normalizes to src (via `ProjectIndex.cwds`), caller spelling kept as fallback; detect AND verify use the same derivation; case/separator variant tests for both | AC-60 | **Complete 2026-07-30** |
| 17.8 | Path confinement done: hook transcript paths reject `..`/`.` components and are canonicalized against the real filesystem before containment is checked (the lexical prefix check accepted `<projects>/../../x`); `FileSystem::is_reparse_point` reads NTFS attributes; mutation walks REFUSE junctions at plan time (guard, exit 2) with a TOCTOU re-check in the snapshot; the archive walk SKIPS them (best-effort sweep must not abort on one link); real `mklink /J` junction test through the binary | AC-61 | **Complete 2026-07-31** |
| 17.9 | Surface reduction executed on maintainer confirmation: `Collision` enum, `Scope` enum, and the three CLI flags deleted end to end (CLI arg parsing, `PlanOpts`, `AssociateOpts`, `Ctx.scope`, the Minimal/Full plan branches); nested projects are a hard refusal (`NestedProjects`, exit 2) naming the children; collision guard unconditional; clap rejects the removed flags (exit 2) with a test; docs, glossary, troubleshooting, and review guide updated | AC-58 | **Complete 2026-07-30** |
| 17.10 | Synthetic fixtures done: `scripts/generate-reference-fixtures.py` deterministically produces both transcripts preserving exactly the locked properties (2,082 anchored rewrites split 227/54 and 1,240/534/27, the 10/55 preserved mentions, line counts 329/2,285, all-lines-parse); 18.1 MB of real conversation replaced by ~540 KB synthetic; golden tests pass unchanged; fixtures README rewritten (everything synthetic, regeneration rules); history removal is D10 | AC-62 | **Complete 2026-07-31** (engineering half; D10 publication half is the maintainer's) |
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
| `backup_walk_read_failure_aborts_before_any_write` | AC-59 |
| `apply_verified_rolls_back_when_verify_itself_errors` | AC-59 |
| `verify_reports_malformed_history_lines` | AC-57 |
| `verify_with_plan_catches_a_missing_destination_anchor` | AC-57 |
| `plugin_state_is_found_when_src_is_spelled_differently` | AC-60 |
| `plugin_verify_catches_a_leftover_dir_under_the_recorded_spelling` | AC-60 |
| `nested_projects_are_a_hard_refusal` | AC-58 |
| `a_project_with_no_nested_children_still_plans` | AC-58 (guard scoped correctly) |
| `destination_key_collision_always_refuses` | AC-58 |
| `removed_options_are_rejected_outright` (binary) | AC-58 |
| `strict_walk_refuses_a_reparse_point` | AC-61 |
| `plan_refuses_when_the_project_state_dir_contains_a_junction` | AC-61 |
| `apply_refuses_a_junction_created_after_planning` (TOCTOU) | AC-61 |
| `archive_skips_a_reparse_subtree_and_archives_the_rest` | AC-61 |
| `apply_refuses_a_real_junction_inside_project_state` (binary, real mklink /J) | AC-61 |
| `hook_stdin_rejects_a_dotdot_escape` (binary) | AC-61 |
| `transcript_confinement_*` (4 unit tests incl. `..` escape on real dirs) | AC-61 |
| golden suite unchanged against synthetic fixtures | AC-62 |

## Decision dependencies

- **AC-58 removals** proceed on the spec's default (remove) unless the maintainer chooses
  implementation instead. Removal is reversible; shipping inert safety flags is not honest.
- **AC-62 / D10**: the engineering half (synthetic fixtures) does not wait on the publication
  decision; the history-rewrite half cannot proceed without it.

## Exit criteria

- Every Critical and High AC has a red-first regression test and is green.
- The adversarial acceptance run passes with a dated report.
- Hygiene gate (g) flips to PASS; S-01 sign-off then proceeds against accurate evidence.
