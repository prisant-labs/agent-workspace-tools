---
id: S-03
title: "Implementation plan: awt repair --drive-letter"
type: implementation-plan
status: complete
created: 2026-07-30
updated: 2026-07-30
linked-spec: ./spec.md
target-release: v1.0.0
ac-coverage: complete
---

# Implementation plan: `awt repair --drive-letter`

Executed test-first, red-green per step, per `AGENTS.md`. Every test asserts on the **raw bytes**
of `history.jsonl`, not on parsed values, per the convention AR-01 established.

## Design

A new `awt-core` module, `repair.rs`, built from pure functions over the injected `FileSystem` so
the whole feature is testable against `MemoryFileSystem` and never touches a real home in tests.

It reuses the existing write machinery rather than growing a parallel one: repairs are expressed
as the same `Change::RewriteFile` with `RewriteRule` entries that the mover uses, so backup,
count-checked apply, verify, and rollback all come for free and behave identically.

```
scan_malformed(text)            -> distinct "::"-prefixed project values + line counts
classify(fs, value, drives)     -> Repairable(letter) | NoCandidate | Ambiguous(letters)
present_drives(fs)              -> drive letters that exist, via the FileSystem trait
build_repair_plan(fs, home)     -> RepairPlan { repairs, unrepairable, ambiguous, change }
```

`present_drives` probes `A:/` through `Z:/` through the trait rather than calling any OS API, so
tests control the drive set exactly and the function stays deterministic.

## Phase / AC coverage

| Phase | Work | AC |
|-------|------|----|
| 16.1 | `scan_malformed` + `classify` + `present_drives`, pure, fully unit-tested | AC-45, AC-46 |
| 16.2 | `build_repair_plan` producing a `Change::RewriteFile`; dry-run rendering | AC-45, AC-47, AC-53 |
| 16.3 | `awt repair` CLI wiring: dry run by default, `--apply` writes through the standard backup/apply path | AC-47, AC-48, AC-50 |
| 16.4 | Verification, idempotency, and isolation guarantees | AC-49, AC-51, AC-52 |

## Completion Status

Last updated 2026-07-30. All phases complete; 14 new tests, workspace suite green.

| Phase | Status |
|-------|--------|
| 16.1 | Complete - `scan_malformed`, `classify`, `present_drives` in `crates/awt-core/src/repair.rs` |
| 16.2 | Complete - `build_repair_plan` emits a single count-checked `RewriteFile` change |
| 16.3 | Complete - `awt repair --drive-letter [--apply]`, dry run by default |
| 16.4 | Complete - verify, idempotency, and store-isolation tests |

## Test map

| Test | AC |
|------|----|
| `scan_finds_malformed_values_with_line_counts` | AC-45 |
| `classify_repairable_when_exactly_one_drive_resolves` | AC-46 |
| `classify_refuses_when_no_drive_resolves` | AC-46 |
| `classify_refuses_when_two_drives_resolve` | AC-46 |
| `classify_ignores_a_well_formed_path` | AC-45 |
| `dry_run_writes_nothing` | AC-47 |
| `plan_expected_count_matches_live_occurrences` | AC-48 |
| `apply_repairs_only_the_unambiguous_values` | AC-46, AC-52 |
| `apply_preserves_unrelated_lines_byte_for_byte` | AC-52 |
| `apply_preserves_line_count` | AC-49 |
| `repair_is_idempotent` | AC-51 |
| `repair_leaves_other_stores_untouched` | AC-52 |
| `json_output_reports_declined_sets` | AC-53 |
| `cli_repair_dry_run_then_apply` (integration) | AC-47, AC-48 |
| `invalid_utf8_is_refused_and_nothing_is_written` | AC-53a |

## Notes

The `Ambiguous` case has no instance in the maintainer's real data (measured: zero values resolve
on more than one drive). It is implemented and tested anyway, because the guard is the reason the
feature is defensible, and a guard that has never executed is an assumption rather than a
behavior.
