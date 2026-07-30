---
version: v1.1.0
title: "Release plan: v1.1.0"
type: release-plan
status: in-progress
created: 2026-07-30
updated: 2026-07-30
includes:
  - S-03
spec-count: 1
plan-count: 1
checklist-complete: false
theme: "Repair: fix Claude state that is damaged rather than merely stale"
---

# Release Plan: v1.1.0

## Theme

**Repair.** v1.0 relocates state and protects it. v1.1 opens a third capability: fixing state
that is *damaged* rather than merely stale, under the same fail-closed rules.

## Context

v1.0's model has two health states for a path reference: correct, or stale (the folder is gone).
The 2026-07-28 acceptance run found a third that the model had no name for: **damaged** - a value
that is neither correct nor legitimately stale, but corrupted, and mechanically recoverable.

46 distinct `project` values in the maintainer's live `history.jsonl`, spanning **3,121 lines**,
have had their drive letter replaced by a colon: `::\Projects\X` where `E:\Projects\X` belongs.
`awt doctor` correctly reports these as stale and correctly refuses to guess at them, which is the
v1.0 design working as intended. But "refuses to guess" and "cannot repair" are different claims,
and measurement showed the difference is large: **34 of the 46 resolve on exactly one drive, and
none resolve on more than one.** There is nothing to guess.

(An earlier hand analysis reported 45 and 33. It used a case-insensitive unique, which collapsed
`::\projects\product-on-purpose\pm-skills` and `::\Projects\...\pm-skills` into one entry. The
tool counts them separately, which is correct: the rewrite is a case-sensitive byte splice, so each
variant is a distinct literal needing its own rule. The recoverable line count, 2,303, was
unaffected and is the figure that matters.)

This release adds `awt repair --drive-letter` under exactly the rule the rest of the tool uses:
act when there is one answer, refuse when there is not.

Program context: `docs/ROADMAP.md`. The finding and its measurements:
[`../plan_v1.0.0/acceptance-run-2026-07-28.md`](../plan_v1.0.0/acceptance-run-2026-07-28.md),
decision D6 in [`../plan_v1.0.0/plan_v1.0.0.md`](../plan_v1.0.0/plan_v1.0.0.md).

**This release does not gate the v1.0.0 tag.** v1.0 ships first.

## Also parked for v1.x

Promotable into this plan or a later one when scheduled, neither started:

- **P10 cross-volume move (AC-2)** - behind the existing copy primitives.
- **P11 Codex and Gemini adapters (AC-27)** - behind the existing store-adapter boundary.

---

## Aggregation

| id | title | spec-status | plan-status | AC-coverage | has-plan? |
|----|-------|-------------|-------------|-------------|-----------|
| S-03 | `awt repair --drive-letter` - recover history entries with a corrupted drive prefix | committed | in-progress | complete | yes |

---

## Hygiene Gates

| Gate | Condition | Status |
|------|-----------|--------|
| (a) Spec status | Every effort's `spec.md` is `committed` or `fulfilled` | PASS - S-03 committed (authored from measured data, maintainer-approved 2026-07-30) |
| (b) Coupled plan | Every effort has an `implementation-plan.md` | PASS |
| (c) AC coverage | Every plan has `ac-coverage: complete` | PASS |
| (d) Phases done | Every completion table shows every phase Done | PASS - phases 16.1-16.4 complete |
| (e) Staleness | No `spec.md` edited after its sibling plan | PASS |
| (f) Acceptance run | Manual acceptance run passes on real data | PENDING - to be folded into the next full run |

---

## Doc-Update Checklist

| Doc | Update | Done |
|-----|--------|------|
| `docs/reference/commands.md` | An `awt repair` section matching `--help` | [x] |
| `docs/recipes.md` | A recipe for the damaged-history case | [x] |
| `docs/faq.md` | What "damaged" means versus stale | [x] |
| `docs/troubleshooting.md` | Exit codes for repair | [x] |
| `docs/index.md` | Rows for anything new | [x] no new docs |
| Root `CHANGELOG.md` | Entry under an unreleased v1.1.0 heading | [x] |
| `docs/CHANGELOG.md` | Doc-impact entry | [x] |
| `docs/ROADMAP.md` | Version map and status reflect v1.1.0 | [x] |
| Workspace `Cargo.toml` | Version bump at tag time, not before | [ ] at tag |
| Git tag `v1.1.0` | After v1.0.0 ships and a clean acceptance run | [ ] |

---

## Open Questions / Decisions

| ID | Title | Resolution | Status | Updated |
|----|-------|------------|--------|---------|
| D7 | What caused the corruption? | Unknown; repair ships without a root cause | Open | 2026-07-30 |
| D8 | Should repair generalise beyond the drive prefix? | No - narrow by design | Resolved | 2026-07-30 |

### D7: What caused the corruption? (Open)

**Summary.** The transformation is systematic, not random: every capital `E` became `:`.
`E:\Projects` became `::\Projects`, and `EchoesGPT` became `:choesGPT` in the same value. That is
a single-character substitution applied across the file, not bit rot or a truncated write.

**Context.** It predates awt. Nothing in this tool has ever written `history.jsonl` on the
maintainer's machine outside a scratch copy. The cause is therefore upstream: Claude Code itself,
an editor, a sync client, or a hand-run script.

**Why it matters.** Repair without a root cause is a treadmill: if the cause is live, corruption
returns and `repair` becomes a chore rather than a fix.

**Desired outcome.** Either the cause is identified, or repair ships with the risk stated.

**Recommendation.** Ship repair regardless - 2,303 recoverable lines are worth recovering today,
and the operation is safe and idempotent. But record the current corrupted set, and if a later
`doctor` run shows *new* `::` values, that is evidence the cause is live and worth chasing.

---

> **Maintainer decision:** _(pending)_
>
> * **Status:** Open
> * **Choice:** (none)
> * **Reasoning:** (none)
> * **Decided by / date:** (none)

### D8: Should repair generalise beyond the drive prefix? (Resolved)

**Summary.** `repair` could be a general "fix damaged paths" command rather than one narrow
transformation.

**Options.**

* **A: general repair** - infer any malformed path from context. Powerful and unbounded, and it
  is exactly the guessing the tool exists not to do.
* **B: one named transformation per known corruption**, each with its own flag and its own
  provable guard. `--drive-letter` is the first.

**Recommendation and outcome:** B. A repair command is the most dangerous surface this tool could
grow, because it writes based on inference rather than on a path the user supplied. Keeping each
transformation narrow, named, and separately guarded means every repair is auditable and the blast
radius of a mistaken inference is bounded to one known shape.

---

> **Maintainer decision:** narrow by design.
>
> * **Status:** Resolved
> * **Choice:** Option B, one named transformation per known corruption
> * **Reasoning:** A general repair is unbounded inference, which contradicts the tool's central
>   promise. Narrow transformations stay provable.
> * **Decided by / date:** authored 2026-07-30 on the maintainer's approval of D6 option B

---

## Notes

Created 2026-07-30 when the maintainer approved D6 option B. The measurements quoted throughout
come from the live machine on that date and are reproducible with `awt doctor`.
