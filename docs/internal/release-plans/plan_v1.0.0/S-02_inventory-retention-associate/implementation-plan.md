---
id: S-02-plan
title: "Implementation plan wrapper: v1.1 features (phases 13-15)"
type: implementation-plan
status: in-progress
created: 2026-07-11
updated: 2026-07-11
linked-spec: spec.md
linked-release: ../plan_v1.0.0.md
canonical-plan: ../../../../superpowers/plans/2026-07-10-claude-project-mover.md
ac-coverage: complete
phase-count: 3
---

# S-02 Implementation Plan Wrapper - v1.1 features (phases 13-15)

The canonical TDD implementation plan for phases 13-15 lives at
[`../../../../superpowers/plans/2026-07-10-claude-project-mover.md`](../../../../superpowers/plans/2026-07-10-claude-project-mover.md).
This wrapper exists so the release folder
(`plan_v1.0.0/S-02_inventory-retention-associate/`) is self-describing: a reviewer
or contributor can open this directory and immediately find both the pointer spec
(in [`spec.md`](spec.md), with the canonical AC at `docs/features/`) and a pointer
to the canonical plan with a phase-level AC coverage map. The canonical plan is the
authoritative source for test structures, fixture paths, and phase-by-phase
implementation detail; this file is a navigation aid only.

## Phase to AC Coverage

| Phase | Feature | AC Coverage |
|-------|---------|-------------|
| 13 | F13 inventory (`cpm list`): session-keyed linkage + terminal/json/html rendering | AC-28, AC-29, AC-30, AC-31, AC-32, AC-33 |
| 14 | F14 archive engine: content-hash dedup, bulk + hook + retention (`cpm archive`) | AC-34, AC-35, AC-36, AC-37, AC-38, AC-39 |
| 15 | F15 re-associate/export (`cpm associate --from --to`) | AC-40, AC-41, AC-42, AC-43, AC-44 |

## Completion Status

| Phase | Status |
|-------|--------|
| 13 | Not started |
| 14 | Not started |
| 15 | Not started |
