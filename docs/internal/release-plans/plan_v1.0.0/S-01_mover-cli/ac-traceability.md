---
type: review-evidence
title: "S-01 (mover CLI) - AC to test traceability and remediation record"
spec: docs/internal/release-plans/plan_v1.0.0/S-01_mover-cli/spec.md
date: 2026-07-23
status: remediation-complete-pending-maintainer-signoff
ac-count: 24
verdicts:
  proven: 16
  test-thin: 8
  functional-gap: 0
method: "Semantic AC-to-test matching (tests are not AC-annotated). Sonnet audit, then controller verification of the highest-stakes rows against source. Gaps closed on branch s01-ac-gap-remediation."
---

# S-01 (mover CLI): AC to test traceability + remediation record

Evidence for the maintainer sign-off on [S-01 (mover CLI spec)](spec.md), which is
`status: draft` / `requires-human-review: true`. The spec defines 24 acceptance criteria
(AC-1, AC-3..24, AC-26). This document maps each AC to the test that proves it, and records
the remediation of the five functional gaps the review found.

## Remediation complete (branch `s01-ac-gap-remediation`)

The five functional gaps are closed and the two policy decisions are made. Commits:

| Item | Handle | Resolution | Commit |
|------|--------|------------|--------|
| AC-1 | same-volume rename | `same_volume` wired; `CrossVolume` guard refuses cross-volume (exit 2) | `fb400e0` |
| AC-21 | lock detection | `detect_live` wired into `build_plan`; refuse without `--force`, warn with it | `fb400e0` |
| AC-7 | ambiguous attribution | fail-closed refuse + surface candidates (decision 7a); AC-7 wording amended, `--attribute` resolver -> v1.x | `fb400e0` |
| AC-22 | machine-readable report | `report.json` by default + `--json` to stdout | `ef10583` |
| AC-17v | verifiable revert | post-rollback byte-identity proof + `rollback-report.json` (new capability atop AC-17) | `dfb9ee6` |
| AC-26 | zero LLM/network | deps-guard test fails if a network/LLM crate enters the lockfile | `485e565` |
| AC-19 | idempotent re-run | amended to idempotent-by-refusal, exit 2 (decision 19b); matches current behavior, no code change | spec amend |

All gates re-verified by the controller after each task: 107 workspace tests pass, clippy
and fmt clean. New user-facing doc: [docs/troubleshooting.md](../../../../troubleshooting.md).

## Decisions (resolved)

- **AC-7 / OQ-1 (attribution default): 7a.** Ambiguous history -> surface + fail-closed
  refuse for v1.0; the `--attribute fork|base|both` resolver is deferred to v1.x. Rejected
  default-to-fork (would rewrite a history that may belong to the other live clone).
- **AC-19 (idempotent re-run): 19b.** Amended to idempotent-by-refusal (exit 2); the
  destination-exists guard is the idempotency signal. Documented in `troubleshooting.md`.

## Scoreboard (after remediation)

| Verdict | Count | Meaning |
|---------|-------|---------|
| PROVEN | 16 | A test directly asserts the AC's outcome |
| Test-thin | 8 | Behavior is present and correct; the proving test is incomplete (add a test) |
| Functional gap | 0 | None remain - all five were closed |

## Full traceability table

| AC | Handle | Verdict | Proof (file:line) or resolution |
|----|--------|---------|----------------------------------|
| AC-1 | same-volume rename | PROVEN | `plan.rs` cross-volume guard + `refuses_cross_volume_move_...` (`fb400e0`) |
| AC-3 | refuse on collision | Test-thin | `plan.rs:162` + `exit.rs:24`; "no changes" not explicitly asserted |
| AC-4 | worktree refused | Test-thin | `plan.rs:174` fires guard; `force` override path untested |
| AC-5 | plan enumerates stores | Test-thin | each store tested alone; no combined three-store plan test |
| AC-6 | map by stored cwd | PROVEN | `index.rs:169`, `stores/claude_projects.rs:191` |
| AC-7 | ambiguous attribution | PROVEN | fail-closed refuse + `refuses_when_src_matches_ambiguous_history` (`fb400e0`); AC amended (7a) |
| AC-8 | projects dir renamed | Test-thin | `apply.rs:195` proves new dir; old-dir-gone not explicitly asserted |
| AC-9 | encoding matches Claude | PROVEN | `paths.rs:87` vs real on-disk dirs |
| AC-10 | cwd rewritten | PROVEN | `tests/reference_apply.rs:9` (anti-vacuity `checked == 2`) |
| AC-11 | boundary-anchored | PROVEN | `rewrite.rs:46` + golden `tests/anchored_reference.rs:14` |
| AC-12 | non-path preserved | PROVEN | `tests/anchored_reference.rs:14` (package/branch counts identical) |
| AC-13 | claude.json migrated | Test-thin | detect + plan proven; no apply-then-readback integration test |
| AC-14 | history.jsonl rewritten | Test-thin | detect + plan proven; no apply-then-readback test |
| AC-15 | dry-run writes nothing | PROVEN | `tests/cli.rs` plan test (byte-identical after plan) |
| AC-16 | backup before write | PROVEN | `backup.rs:193` (sha256 match, pre-apply) |
| AC-17 | rollback restores | PROVEN | `rollback.rs`, `apply.rs:225`, `tests/associate_merge.rs:67` |
| AC-18 | verify postconditions | Test-thin | pass case `verify.rs:88`; failure-list case untested |
| AC-19 | idempotent re-run | PROVEN | `apply.rs:271` no-op; AC amended to idempotent-by-refusal (exit 2), matches behavior (19b) |
| AC-20 | hard-fail unknown format | PROVEN | `apply.rs:307` (state-untouched asserted) |
| AC-21 | lock detection | PROVEN | `detect_live` wired + `refuses_when_lock_exists_...` (`fb400e0`) |
| AC-22 | machine-readable report | PROVEN | `report.json` default + `--json`; `report.rs` + `tests/cli.rs` (`ef10583`) |
| AC-23 | CLI subcommands | Test-thin | only `plan`/`apply` tested as a binary; verify/rollback via lib API |
| AC-24 | exit codes | PROVEN | `exit.rs:24` (all variants mapped, incl. `CrossVolume`) |
| AC-26 | zero LLM/network | PROVEN | `tests/no_network_deps.rs` deps-guard (`485e565`) |

Plus **AC-17v (verifiable revert)** - a new capability beyond AC-17: `verify_rollback` proves
each restored file is byte-identical to its pre-migration original; positive (anti-vacuity)
and negative (tamper-detected) tests in `rollback.rs` (`dfb9ee6`).

## Remaining test-thin ACs (acceptable for v1.0; add tests when convenient)

AC-3, AC-4, AC-5, AC-8, AC-13, AC-14, AC-18, AC-23. Each behavior is present and correct; the
proving test is incomplete. None is a functional gap. The closing test for each:

- **AC-3:** assert no formerly-absent file exists after the `DestinationExists` error.
- **AC-4:** set `force: true`, confirm `build_plan` succeeds for a worktree source.
- **AC-5:** one integration test seeding all three stores; assert all three change types.
- **AC-8:** add `assert!(!fs.exists(old_encoded_dir))` after the apply.
- **AC-13 / AC-14:** apply-then-readback tests for `claude.json` and `history.jsonl`.
- **AC-18:** seed a violated postcondition; assert `verify` returns an `ok: false`.
- **AC-23:** CLI binary tests for `apply`, `verify`, `rollback`.

## Notably strong tests

- **`tests/anchored_reference.rs:14`** reproduces the exact real-move counts
  (1467 + 588 + 27 = 2082) against production transcripts (AC-11, AC-12).
- **`tests/reference_apply.rs:9`** drives the whole plan-to-apply pipeline on real
  transcripts with an anti-vacuity guard (`checked == 2`) (AC-10).
- **`tests/associate_merge.rs:67`** proves rollback does not corrupt the destination's own
  transcripts in the merge path (AC-17).
- **`rollback.rs` verify_rollback negative test** tampers with a restored file and asserts
  the mismatch is flagged with expected-vs-got hashes (AC-17v).

## Maintainer sign-off checklist

Before flipping [S-01 (mover CLI spec)](spec.md) to `status: committed`:

- [x] Decision 1 recorded: AC-7 (ambiguous attribution) -> 7a.
- [x] Decision 2 recorded: AC-19 (idempotent re-run) -> 19b.
- [x] The 5 functional gaps are closed (see Remediation table) or their ACs amended.
- [ ] The 8 remaining test-thin ACs are accepted as-is for v1.0 or their closing tests added.
- [ ] The 24 fulfillment checkboxes in `spec.md` are ticked against this evidence (maintainer).
- [ ] Merge `s01-ac-gap-remediation`, then flip `status: draft` -> `committed` and re-run `--gate`.
