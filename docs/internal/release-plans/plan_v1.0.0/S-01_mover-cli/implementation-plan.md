---
id: S-01-plan
title: "Implementation plan wrapper: v1 mover (phases 1-9)"
type: implementation-plan
status: complete
created: 2026-07-11
updated: 2026-07-11
linked-spec: spec.md
linked-release: ../plan_v1.0.0.md
canonical-plan: ../../../../superpowers/plans/2026-07-10-claude-project-mover.md
ac-coverage: complete
phase-count: 9
---

# S-01 Implementation Plan Wrapper - v1 mover (phases 1-9)

The canonical TDD implementation plan for the v1 mover lives at
[`../../../../superpowers/plans/2026-07-10-claude-project-mover.md`](../../../../superpowers/plans/2026-07-10-claude-project-mover.md).
This wrapper exists so the release folder (`plan_v1.0.0/S-01_mover-cli/`) is
self-describing: a reviewer or contributor can open this directory and immediately
find both the acceptance criteria (in [`spec.md`](spec.md)) and a pointer to the
canonical plan with a phase-level AC coverage map. The canonical plan is the
authoritative source for test structures, fixture paths, and phase-by-phase
implementation detail; this file is a navigation aid only.

## Phase to AC Coverage

| Phase | Delivers | AC Coverage |
|-------|----------|-------------|
| 1 | Workspace, `FileSystem` trait + Memory impl, fixtures, no-network + CI dep-gate | AC-16 (foundations) |
| 2 | `encode_project_dir` (corrected: `[^A-Za-z0-9] -> -`) + reverse `ProjectIndex` | AC-6, AC-9 |
| 3 | `Store` trait, `probe`/`detect`/`audit`, 6 adapters' read paths + `sweep.unknown` | AC-5 |
| 4 | `cpm doctor` + `cpm scan` - read-only, exit codes, report | AC-5, AC-15 (partial) |
| 5 | Anchored rewrite engine + `buildPathRules`, count-checked, golden test | AC-10, AC-11, AC-12 |
| 6 | `plan` (diff + machine plan), collision + nested + worktree detection | AC-3, AC-4, AC-7, AC-13 (plan), AC-15 |
| 7 | `snapshot`/backup + manifest, transactional `apply`, folder-move-last | AC-1, AC-8, AC-10, AC-13, AC-14, AC-16, AC-22 |
| 8 | `verify` + auto-rollback, idempotency, hard-fail, lock detect | AC-18, AC-19, AC-20, AC-21 |
| 9 | `rollback` from manifest, CLI complete, exit-code contract | AC-17, AC-23, AC-24, AC-26 |

## Completion Status

Last updated 2026-07-21. The live task-level ledger is `.superpowers/sdd/progress.md`
(gitignored); this table is the human-facing summary.

All nine phases are implemented, tested (96 workspace tests green, clippy and fmt clean),
and pushed to `main`. Post-implementation adversarial reviews drove the hardening fixes noted.

| Phase | Status |
|-------|--------|
| 1 | Complete (workspace + CI `d17ec2f`, `FileSystem` trait `b6bbf41`, golden fixtures `25dd5ab`) |
| 2 | Complete (path encoding + reverse `ProjectIndex` with ambiguity/stale resolution) |
| 3 | Complete (`Store` trait + six adapter read paths + `sweep.unknown`, `c3401d9`) |
| 4 | Complete (`cpm doctor` + `cpm scan` + CLI, `afc8ecf`, `9c824e0`; doctor honesty checkpoint passed) |
| 5 | Complete (anchored rewrite engine + golden reference-move test, `76226bc`) |
| 6 | Complete (`plan` + guards + nested detection + `render_plan`, `e7ec6d0`, `056bd84`) |
| 7 | Complete (snapshot backup + transactional apply + end-to-end golden, `e2aa334`) |
| 8 | Complete (per-store + aggregate verify, rollback, `apply_verified` auto-rollback, locks, `a2801e8`, `c758613`) |
| 9 | Complete (full CLI `plan`/`apply`/`verify`/`rollback` + exit-code contract, `2285784`) |
| review | Fail-loud read hardening from the F13 review, `d66c49b` |
