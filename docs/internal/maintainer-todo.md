---
title: "Maintainer to-do - the single human checklist"
type: checklist
updated: 2026-07-30
---

# Maintainer to-do

**This file is the canonical human to-do list for this repo.** If you only open one document to
answer "what is left for me to do", open this one.

Legend: **[HUMAN]** only you can do this. **[DELEGABLE]** an agent or contributor can do it, you
review.

## State correction (2026-07-30)

An earlier revision of this file said no technical gate remained for the v1.0.0 tag. **That was
wrong.** The 2026-07-30 acceptance run passed, but it exercises the happy path; an adversarial
audit the same day found data-loss and false-success paths it never touches, each independently
verified against source before being accepted. Those findings are now the S-04 safety closeout
and they block the tag. The wrong claim is retracted here and in the roadmap, the README, and
the acceptance report itself.

---

## 1. Blocking the v1.0.0 tag

### 1.1 The v1 safety closeout (S-04) **[DELEGABLE - two decisions inside are yours]**

[`S-04_safety-closeout/spec.md`](release-plans/plan_v1.0.0/S-04_safety-closeout/spec.md), with
the phase order in its
[implementation plan](release-plans/plan_v1.0.0/S-04_safety-closeout/implementation-plan.md).
Estimated 7-10 engineering days. The three Criticals, in one line each:

- **Rollback can delete data it never backed up**: a directory rename snapshots only top-level
  `*.jsonl`, then rollback recursively deletes the renamed directory - sidecars and nested files
  are destroyed by the undo.
- **A missing source folder is a silent success**: `apply` skips the move, records it as
  applied, exits 0, and verify never checks the folder.
- **A malformed `settings.json` is replaced with an empty one** by the next settings write.

**Progress: all three Criticals AND three of the six Highs are closed (2026-07-30, red-first,
164 tests green).** AC-57: verification now derives from the applied plan - every planned json
splice is checked as raw bytes, malformed history lines fail, and a verify that errors rolls
the apply back instead of stranding it. AC-59: mutation-path reads fail closed, proven with a
failure-injecting filesystem. AC-60: plugin state is found via the recorded `cwd` spelling, so
a case or separator variant of the path no longer silently strands plugin dirs.

Earlier progress note: Rollback
now snapshots recursively and renames the directory back wholesale, so the complete tree
returns - proven at tree level and end-to-end on the real filesystem. A missing source refuses
at plan AND apply, and verify checks the folder postconditions. Settings writes fail closed and
write atomically. AC-53a closed earlier the same day. Remaining: the Highs (AC-57..AC-62) and
the adversarial acceptance run; your two calls inside them are 1.2 and 1.3 below.

### 1.2 Decide D10: the real transcripts published in `test/fixtures/` **[HUMAN]**

Four real session transcripts, 18.1 MB, unredacted by recorded policy, in a public repository
since 2026-07-24, and present in Git history regardless of future deletion. Two questions only
you can answer, recorded at
[D10 in the release plan](release-plans/plan_v1.0.0/plan_v1.0.0.md):

1. Read them with fresh eyes: does anything in there need rotation or notification?
2. Removal-going-forward, or coordinated history rewrite (invalidates clones)?

**Your read is now cheap:** the extraction aid at
`_local/fixture-review/2026-07-30-extraction-aid.md` (local-only, never committed) indexes
every pattern hit with file and line, pre-triaged: the 60 `sk-` strings are kebab-case task
IDs, not keys; the 820 base64 runs are dominated by embedded media; the read that matters is
64 emails and 172 non-well-known URLs. You approved the sequence aid -> read -> rewrite; the
rewrite happens after your read and after the synthetic fixtures land, and branch protection
goes on right after the rewrite.

Exposure data, for calibration: 0 forks, 0 stars, 0 views in 14 days, but 60 clones from 28
unique sources (automated crawlers). The content has left GitHub; rotation, if anything needs
it, matters more than deletion.

Synthetic replacement fixtures proceed under S-04 AC-62 regardless.

### 1.3 AC-58 surface removals - **CONFIRMED AND DONE 2026-07-30**

You approved the removal. All three options are gone end to end (CLI, core enums, plan
branches); nested projects are now a hard refusal that names the children; the collision guard
is unconditional; clap rejects the removed flags with exit 2. Docs, glossary, troubleshooting,
and the review guide are reconciled.

### 1.4 Adversarial acceptance run **[DELEGABLE, you own the verdict]**

After the closeout: the revised matrix (missing source, sidecar-bearing project, malformed
settings, junction-in-tree, invalid UTF-8, case-variant plugin path) on a fresh scratch copy.
This is S-04 phase 17.11 and flips hygiene gate (g).

### 1.5 Sign off S-01 **[HUMAN - nobody else can do this]**

[`S-01_mover-cli/spec.md`](release-plans/plan_v1.0.0/S-01_mover-cli/spec.md) is `status: draft`
with `requires-human-review: true`; it was carved by an agent and has never been read back by a
human. Do this **after** the closeout, so you are signing the criteria against the code as it
will actually ship.

Two aids exist now:

- [`review-guide.md`](release-plans/plan_v1.0.0/S-01_mover-cli/review-guide.md) - each AC in
  plain language, its evidence, and what currently contests it. Built to make the read cheaper,
  not to nudge the verdict.
- [`ac-traceability.md`](release-plans/plan_v1.0.0/S-01_mover-cli/ac-traceability.md) - the
  AC-to-test mapping.

The question that matters is not "do the tests pass" (they did, through two release blockers):
it is **"do these criteria describe the guarantees I actually want?"**

---

## 2. Decisions

| ID | State | What |
|---|---|---|
| D7 (corruption cause) | Open, no action needed | A tripwire, not a task: if a future `doctor` shows **new** `::` values beyond the 12 known leftovers, the cause is live - investigate then. Recorded in [the release plan](release-plans/plan_v1.0.0/plan_v1.0.0.md) |
| D9 (fold repair into v1.0.0) | Resolved 2026-07-30 | Your call, executed: S-03 folded in, the v1.1.0 plan retired, changelogs merged |
| D10 (fixture publication) | **Open - see 1.2** | The one decision that should not wait |

---

## 3. The tag ceremony **[HUMAN]**

Only once section 1 is clear. Full detail in [`docs/release-runbook.md`](../release-runbook.md):

1. Confirm CI green on the exact commit.
2. Flip the README badge and remove the pre-release callout (not before - it would be false).
3. Date the `[1.0.0] - unreleased` heading in the root `CHANGELOG.md`.
4. `git tag -a v1.0.0 -m "v1.0.0"`, push the tag.
5. Update `docs/ROADMAP.md` Section 7 with the tag date.

Scratch homes from past acceptance runs are already deleted; the adversarial run creates its
own with `scripts/new-scratch-home.ps1`.

---

## 4. Not blocking anything

- **Branch protection on `main`** - recommended (require the CI check, require PRs). Repo
  settings, two minutes, your GitHub admin.
- **Dependabot queue processed 2026-07-30** (your call: process now): actions/checkout 4->7,
  actions/cache 4->6, and the grouped cargo minor/patch bumps merged CI-green; the sha2 0.11
  major is rebasing and merges when its fresh CI passes.
- **CI-3 signed binary channel** - still not a tag blocker; v1.0 ships source-first.
- **P10 cross-volume, P11 Codex/Gemini adapters** - parked, promotable.
- **v2 "Taura" GUI** - deliberately gated behind the closeout plus a frozen, versioned
  `awt-core` contract (typed plan/verify/report models, stable ordering, schema version). The
  passing parity artifact (`plan --json`) exists, but freezing today's ad-hoc JSON as the GUI
  contract would bake in the exact inconsistencies the closeout is fixing. Sequence: closeout,
  then contract, then shell.

---

## How to keep this file honest

Update it whenever a gate changes state, and prefer moving detail *out* of here into the owning
document. This file has now been wrong once - the no-technical-gate claim - and the correction
is kept visible at the top rather than edited away, because the failure mode it guards against
is exactly this: a summary drifting from the evidence underneath it.
