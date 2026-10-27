---
title: "Maintainer to-do - the single human checklist"
type: checklist
updated: 2026-10-27
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

### 1.2 D10: transcripts - **REWRITE DONE 2026-07-31; your READ remains [HUMAN]**

**Done (2026-07-31, at your direction):** the history rewrite. `git filter-repo` removed
from ALL history: the four real transcript blobs (18.1 MB) and the four session logs once
committed under `_agent-context/session-log/`. Force-pushed with `--force-with-lease`; the
rewritten HEAD tree is byte-identical to the pre-rewrite HEAD (verified by tree hash), CI
green on the new head `50f6006`. Preserved locally before the rewrite: transcripts at
`_local/fixture-archive/real-transcripts/`, a full pre-rewrite mirror at
`_local/pre-rewrite-mirror.git`, and the old-to-new commit map at
`_local/rewrite-2026-07-31-commit-map.txt` (key mappings: 9384619 -> 50f6006,
69ace60 -> f0c0068, e50eba2 -> fe8b4e1, c1d8969 -> c96b24b; SHAs cited in dated reports
are pre-rewrite IDs and stay as historical record).

**Still yours:**

1. **The read - now a decision sheet, not a scan.** Work from
   `_local/fixture-review/2026-08-05-d10-decision-sheet.md` (local-only, never committed),
   which supersedes the 2026-07-30 extraction aid. The aid's headline numbers, 64 emails
   and 172 URLs, are *occurrence* counts; a full re-scan of the archived originals
   reproduces those totals exactly (64 email hits, 336 URL hits = 172 non-well-known + 164
   suppressed) and resolves them to **3 distinct emails and 29 distinct URLs**. Transcripts
   quote the same commit trailer and README dozens of times, which is the whole gap.

   What the re-scan found, each with evidence in the sheet:

   - **Zero URLs carry a credential.** No token, key, signature, or `user:pass@host` in any
     of the 29. Nothing to revoke.
   - **Zero of the 3 emails need rotation.** An address is not a credential, and all three
     are already public: one is a vendor no-reply, one is your own GitHub no-reply (the
     author line on every commit you push), and the third is already the author email on
     156 public GitHub commits by a contributor to the public repo these transcripts
     discuss.
   - **Two PRIVATE repositories are named** in the published bytes; both were confirmed
     private on 2026-08-05. They are deliberately NOT named here - repeating them in a
     public file would republish exactly what the finding is about. The sheet identifies
     them. That is the one genuine decision left: accept the disclosure, or rename if the
     name itself reveals something unannounced.
   - **Zero real values survived into the shipping fixtures.** No email or URL from the
     originals appears anywhere under `test/fixtures/` today; the synthetic replacement was
     total.

   Calibration is unchanged: 0 forks/stars, but 60 clones from 28 unique sources before the
   rewrite. The content has left GitHub, so rotation would matter more than deletion ever
   could - it just turns out there is nothing here to rotate.
2. **GitHub Support request (optional but recommended).** A force-push makes old objects
   unreachable but GitHub still serves them by SHA (verified: the old blobs and commits
   still return via the API) and old PR diffs retain the content. Ask Support to remove
   cached views and run a GC:
   https://docs.github.com/en/site-policy/privacy-policies/github-privacy-statement -
   "remove sensitive data" flow for repository `prisant-labs/agent-workspace-tools`.

### 1.3 AC-58 surface removals - **CONFIRMED AND DONE 2026-07-30**

You approved the removal. All three options are gone end to end (CLI, core enums, plan
branches); nested projects are now a hard refusal that names the children; the collision guard
is unconditional; clap rejects the removed flags with exit 2. Docs, glossary, troubleshooting,
and the review guide are reconciled.

### 1.4 Adversarial acceptance run - **DONE 2026-07-31, gate (g) flipped**

Two runs, documented in
[acceptance-run-2026-07-31.md](release-plans/plan_v1.0.0/acceptance-run-2026-07-31.md).
Run 1 found four findings, all fixed same-day red-first (suite 175 -> 180): AR-05
(rollback panicked on the report.json its own apply output points at; now accepts it),
AR-06 (stale `--force` help text), AR-07 (`archive --json` emitted text), AR-08
(`associate` on a never-recorded path exited 4 instead of 2). Run 2 on a fresh scratch
copy passed every happy-path and adversarial step, including a real junction (plan
refuses, archive skips) and byte-identical whole-tree rollback. **No technical step
remains in the closeout; what is left is yours: 1.2 (D10 read) and 1.5 (S-01 sign-off).**

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

- **Branch protection on `main` - ENABLED 2026-07-31** (right after the D10 history
  rewrite, per your approved sequencing): the `build` check and PRs required, linear
  history required, force pushes and deletions blocked. Note for any future history
  operation: protection must be temporarily lifted first, which is now deliberate
  friction.
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

## 5. Field findings from real use, 2026-10-27

These two gaps came from a real project move. A repository was transferred and renamed on GitHub. It was cloned fresh to a new folder, and `awt associate --from <old> --to <new>` was run to carry its Claude Code state across. The run used the release binary built 2026-07-31. The behavior was then confirmed against the source at `847cb20`. The paths below are invented examples.

Whether either finding blocks the v1.0.0 tag is your call **[HUMAN]**. FF-1 is the stronger candidate, because it is a safety claim that does not hold. That is the same class of finding as the S-04 closeout.

### FF-1 (live-session guard misses terminal sessions) **[DELEGABLE]**

**What happened.** `associate` ran while the Claude Code session that launched it was still running in the source folder. The guard did not refuse, and the run reported success. The session kept writing afterwards. It recreated the old `projects/<encoded-old>/` folder, appended new transcript lines under the same session ID, re-added the old `~/.claude.json` project key on exit, and added new `history.jsonl` lines under the old path. The migration was correct when it ran, then went stale within minutes.

**Why.** `locks::detect_live` (`crates/awt-core/src/locks.rs`) only looks for `~/.claude/ide/*.lock`. Those files come from editor integrations. A plain terminal session never writes one, so the most common way of running Claude Code is invisible to the guard. The docs promise more than that. `docs/recipes.md` says to "close any Claude Code session working in the project first, or the lock guard will refuse". The refusal message says a lock means "a running Claude Code CLI may be editing". The README lists "a live lock" among its fail-closed guards. For a terminal session, the guard will not refuse.

**Possible signal (inferred, not documented by Anthropic).** Each interactive session appears to write `~/.claude/sessions/<pid>.json`. That file holds `pid`, `sessionId`, `cwd`, and `status`, along with other fields. A project-scoped guard could refuse when a live process's `cwd` matches `--src` or `--from`, or `--to`, and warn on any other live session. Verify the format against more than one Claude Code version before relying on it. Also confirm that a stale file whose process has died is ignored.

**Workaround until fixed.** The docs could say this outright: exit every session running in the source folder before `apply` or `associate`, and do not run `awt` from inside such a session.

### FF-2 (associate cannot finish into a destination that is already live) **[DELEGABLE - one design call is yours]**

**What happened.** A second `associate` pass, run to pick up the leftovers from FF-1, refused with exit 2. The message was `destination exists: claude.json already has a key for <new>`, and nothing was written, so the refusal itself was correct. The result, though, is that a half-migrated project has no supported way to finish.

**Why.** Two refusals stack. First, `build_plan` (`crates/awt-core/src/plan.rs`) unconditionally refuses when the destination key exists in `~/.claude.json`. The AC-58 removals made that guard unconditional. Second, if that passed, `MergeDir` in `apply.rs` would refuse on the transcript, because the same `<sessionId>.jsonl` existed under both paths. The code comment there says this "should never fire" because session file names are UUIDs. FF-1 is exactly how it fires: one session, migrated while still live, writing in both places.

**What a fix needs to decide (yours).**

- **Settings key.** Merge the old key into the new one, keeping the new key's values on conflict, or refuse as now but print the manual steps. In this case the old key held only `last*` session statistics, so a rule that "statistics-only keys may be dropped" would have been enough.
- **Same-UUID transcripts.** Append the old file's lines that the new file lacks, which keeps one file per session ID, or export the old copy as an archive and leave it out of the live state. Any choice must keep the existing guarantee that the destination's own transcript is never altered.
- **History lines.** These need no new design. The existing `project` field rewrite already handles them.

**What was done by hand this time.** The transcript tail was copied into the destination's `.claude-sessions/` export folder. The other leftovers were left as stale entries, following awt's own rule that stale references are left alone.

**Related, smaller.** `associate`'s default export writes `.claude-sessions/` inside the destination project folder. When that folder is a git repository, nothing stops the exported transcripts from being committed. They are full of local paths and conversation text. Consider printing a warning when the destination is a git work tree and `.claude-sessions/` is not ignored. Another option is to write a `.gitignore` inside the export folder.

---

## How to keep this file honest

Update it whenever a gate changes state, and prefer moving detail *out* of here into the owning
document. This file has now been wrong once - the no-technical-gate claim - and the correction
is kept visible at the top rather than edited away, because the failure mode it guards against
is exactly this: a summary drifting from the evidence underneath it.
