---
title: "Maintainer to-do - the single human checklist"
type: checklist
updated: 2026-11-03
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
That stopped being the whole list on 2026-11-03: 1.6 (acceptance re-run), 1.7 (FF-1 docs fix)
and 1.8 (F-1 and F-2 engine fixes) also blocked the tag, and all three are done. The re-run then
raised 1.9 (AR-09, `archive` copies less than it promises). You chose to fix it before the tag
(2026-11-04), and the fix is done, so what is left is 1.2 and 1.5.

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

PR #28 merged on 2026-11-03, so AC-5, AC-8, AC-13 and AC-14 are now closed by test, and the
review guide rates all four Proven. The guide also now marks AC-21 (live-session guard) as proven
for IDE sessions only, because of FF-1. Read that row with section 5 open. Its AC-17 (rollback
puts everything back) row was wrongly Proven until F-1 was fixed, and a freshness note says so.

### 1.6 Re-run the adversarial acceptance after the clap bump - **DONE 2026-11-04**

Every planned step passed on `main` at `df6985f`, including the F-1 and F-2 additions below. It
found one new issue, recorded at 1.9. Report:
[acceptance-run-2026-11-04.md](release-plans/plan_v1.0.0/acceptance-run-2026-11-04.md).

Decided 2026-11-03. PR #26 merged on 2026-10-24 and moved clap from 4.6.4 to 4.6.7. clap parses
the command line, which is exactly the surface the 2026-07-31 acceptance run exercised, so that
run no longer describes the binary that would be tagged. Re-run the full adversarial acceptance
on a fresh scratch home from `scripts/new-scratch-home.ps1` before the tag. Run it after the
1.7 documentation fix lands, so one run covers the final tree. PR #30 (serde_json, sha2 and
insta) stays held until after the tag, so this run does not include it.

The run must also cover the 1.8 fixes, because rollback and `list` both changed. Seed a plugin
state folder, then run `apply`, `rollback`, and the same `apply` again. Also confirm that
`list --html` refuses an existing file. The 2026-07-31 run passed with F-1 live, because its
rollback step compared original files and hashes and never retried the move.

### 1.7 Correct the live-lock claims in the docs (FF-1) - **DONE 2026-11-03**

Decided 2026-11-03: FF-1 blocks the tag on documentation only. See section 5. Every page that
promised more now says the guard sees IDE lock files only, and gives the workaround. The refusal
message is compiled into the binary, so it stays as it is until the v1.0.1 code fix. The S-01
spec's AC-21 wording is unchanged, because you sign that text. The review guide flags the gap.

### 1.8 Fix F-1 and F-2 in the engine - **DONE 2026-11-03, PR #32**

Decided 2026-11-03: both are fixed before the tag. See section 5.

### 1.9 Decide whether AR-09 blocks the tag **[DECIDED 2026-11-04: option A, fixed on a branch]**

Decided 2026-11-04: option A, fix it before the tag. The fix is in `archive.rs` with tests, and the docs are back to the full claim. It still needs a merge, and the archive steps of the acceptance run should be repeated on the merged build. See section 5.

`awt archive` does not copy subagent transcripts or most session artifacts, which AC-35 (archive
includes session artifacts) promises in part. See section 5.

- **A: fix it before the tag (recommended).** `archive` exists to protect session history from
  auto-delete, and 5.8 GB of subagent transcripts on this machine have no protection. The change
  looks small and confined to `archive.rs`, so only the archive steps should need repeating
  (inferred; the fix is not written). The docs would then go back to the full claim.
- **B: tag now and fix it in v1.0.1.** The docs already state the limit, so nobody is misled, and
  a move and a rollback are unaffected. You would also need to narrow AC-35 in the S-02 spec, or
  accept that it ships unmet.

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
  major is rebasing and merges when its fresh CI passes. **PR #30** (serde_json, sha2 and
  insta) is held until after the tag, decided 2026-11-03.
- **CI-3 signed binary channel** - still not a tag blocker; v1.0 ships source-first. Two build
  requirements were added on 2026-11-03: see the CI-3 list in `docs/release-runbook.md`.
- **P10 cross-volume, P11 Codex/Gemini adapters** - parked, promotable.
- **v2 "Taura" GUI** - deliberately gated behind the closeout plus a frozen, versioned
  `awt-core` contract (typed plan/verify/report models, stable ordering, schema version). The
  passing parity artifact (`plan --json`) exists, but freezing today's ad-hoc JSON as the GUI
  contract would bake in the exact inconsistencies the closeout is fixing. Sequence: closeout,
  then contract, then shell.

---

## 5. Field findings

FF-1 and FF-2 came from real use on 2026-10-27. F-1 and F-2 came from a packaging test on 2026-11-03, and both are fixed. AR-09, at the end of this section, came from the acceptance re-run on 2026-11-04.

FF-1 and FF-2 came from a real project move. A repository was transferred and renamed on GitHub. It was cloned fresh to a new folder, and `awt associate --from <old> --to <new>` was run to carry its Claude Code state across. The run used the release binary built 2026-07-31. The behavior was then confirmed against the source at `847cb20`. The paths below are invented examples.

Whether FF-1 or FF-2 blocks the v1.0.0 tag is your call **[HUMAN]**. FF-1 is the stronger candidate, because it is a safety claim that does not hold. That is the same class of finding as the S-04 closeout.

### FF-1 (live-session guard misses terminal sessions) **[DELEGABLE]**

**What happened.** `associate` ran while the Claude Code session that launched it was still running in the source folder. The guard did not refuse, and the run reported success. The session kept writing afterwards. It recreated the old `projects/<encoded-old>/` folder, appended new transcript lines under the same session ID, re-added the old `~/.claude.json` project key on exit, and added new `history.jsonl` lines under the old path. The migration was correct when it ran, then went stale within minutes.

**Why.** `locks::detect_live` (`crates/awt-core/src/locks.rs`) only looks for `~/.claude/ide/*.lock`. Those files come from editor integrations. A plain terminal session never writes one, so the most common way of running Claude Code is invisible to the guard. The docs promise more than that. `docs/recipes.md` says to "close any Claude Code session working in the project first, or the lock guard will refuse". The refusal message says a lock means "a running Claude Code CLI may be editing". The README lists "a live lock" among its fail-closed guards. For a terminal session, the guard will not refuse.

**Possible signal (inferred, not documented by Anthropic).** Each interactive session appears to write `~/.claude/sessions/<pid>.json`. That file holds `pid`, `sessionId`, `cwd`, and `status`, along with other fields. A project-scoped guard could refuse when a live process's `cwd` matches `--src` or `--from`, or `--to`, and warn on any other live session. Verify the format against more than one Claude Code version before relying on it. Also confirm that a stale file whose process has died is ignored.

**Workaround until fixed.** The docs could say this outright: exit every session running in the source folder before `apply` or `associate`, and do not run `awt` from inside such a session.

**Decision (2026-11-03): the code fix does not block the tag, and the documentation fix does.** Before the tag, correct `docs/recipes.md`, the README and any other page that promises the guard catches a running session. They must say the guard sees IDE lock files only, and give the workaround above. The guard fix ships in v1.0.1. The reason is that the failure leaves a stale migration rather than lost data, and a v1.0 guarantee should not rest on an undocumented file format. A false safety claim in the shipped docs is not acceptable either way. The refusal message text is compiled into the binary, so it changes with the v1.0.1 code fix.

### FF-2 (associate cannot finish into a destination that is already live) **[DELEGABLE - one design call is yours]**

**What happened.** A second `associate` pass, run to pick up the leftovers from FF-1, refused with exit 2. The message was `destination exists: claude.json already has a key for <new>`, and nothing was written, so the refusal itself was correct. The result, though, is that a half-migrated project has no supported way to finish.

**Why.** Two refusals stack. First, `build_plan` (`crates/awt-core/src/plan.rs`) unconditionally refuses when the destination key exists in `~/.claude.json`. The AC-58 removals made that guard unconditional. Second, if that passed, `MergeDir` in `apply.rs` would refuse on the transcript, because the same `<sessionId>.jsonl` existed under both paths. The code comment there says this "should never fire" because session file names are UUIDs. FF-1 is exactly how it fires: one session, migrated while still live, writing in both places.

**What a fix needs to decide (yours).**

- **Settings key.** Merge the old key into the new one, keeping the new key's values on conflict, or refuse as now but print the manual steps. In this case the old key held only `last*` session statistics, so a rule that "statistics-only keys may be dropped" would have been enough.
- **Same-UUID transcripts.** Append the old file's lines that the new file lacks, which keeps one file per session ID, or export the old copy as an archive and leave it out of the live state. Any choice must keep the existing guarantee that the destination's own transcript is never altered.
- **History lines.** These need no new design. The existing `project` field rewrite already handles them.

**Decisions (2026-11-03).**

- **Settings key: refuse as now, and print the manual steps.** No new write path into `~/.claude.json` is added. Dropping an old key automatically when it holds only `last*` statistics is the later improvement, once "statistics only" has a precise definition.
- **Same-UUID transcripts: export the old copy as an archive under `.claude-sessions/`, and leave it out of the live state.** This is the only option that keeps the guarantee that the destination's own transcript is never altered. Appending the missing lines would write into that file. It is also what was done by hand on 2026-10-27.

**What was done by hand this time.** The transcript tail was copied into the destination's `.claude-sessions/` export folder. The other leftovers were left as stale entries, following awt's own rule that stale references are left alone.

**Related, smaller.** `associate`'s default export writes `.claude-sessions/` inside the destination project folder. When that folder is a git repository, nothing stops the exported transcripts from being committed. They are full of local paths and conversation text. Consider printing a warning when the destination is a git work tree and `.claude-sessions/` is not ignored. Another option is to write a `.gitignore` inside the export folder.

### F-1 (rollback leaves a plugin folder behind) - **FIXED 2026-11-03, PR #32**

**What happened.** A packaging test ran `plan`, `apply`, `verify` and `awt rollback` on a synthetic home. Every original file came back byte-identical, but `.claude/plugins/data/<plugin>/state/<name>-<hash>` stayed behind under its new name. A retry of the same move passed `plan` and failed at `apply` with OS error 145 (exit 3), then rolled itself back. No data was lost in the test. The repository's own debug build reproduced it, so packaging did not cause it.

**Why.** `rollback.rs` worked out where each renamed folder went by swapping the encoded project name inside the old path, and it skipped any path without that name. A plugin-state folder is named by a hash, so it never has that name. A transcript folder stored in another case missed the same case-sensitive match, and that second shape was found by reading the code, not in the test. Rollback then restored the backed-up files into the old location, so the byte-identity proof passed. It read only original files, so it could not see the leftover. The review guide rated AC-17 (rollback puts everything back) Proven throughout.

**Decision (2026-11-03): fix it before the tag.** Rollback is the promise this tool makes about irreplaceable data, and the 1.6 re-run was already scheduled to cover the result.

**The fix.**

- The manifest records each rename's destination, and rollback uses it. Manifests written earlier still restore, because rollback rebuilds the destination from the two naming rules.
- `verify_rollback` now fails if a renamed folder is still there.
- `plan` refuses with exit 2 when a folder already occupies a rename destination, or when two folders holding state for two spellings of the source path would share one. `apply` re-checks before its first change.
- The refusal for a folder at both names moved into the rollback preflight. It used to fire after the project folder had already moved back.
- A refused automatic rollback is now reported. `apply_verified` used to drop that error, so a half-applied move read as cleaned up.

**What you may still find on disk.** A rollback run by an older build may have left a `<name>-<hash>` folder. `plan` now names it and refuses. Check first whether it holds a live project's state, then move it aside or delete it.

**Accepted as rare, not fixed (inferred by a review, not reproduced).**

- A manifest from an older build, from a failed apply whose destination already existed, could rename that foreign folder into place if the original folder was later deleted.
- A renamed folder that vanishes between the snapshot and the apply makes a correct rollback exit 3. The snapshot normally catches this, so it needs a race.

### F-2 (`list --html` replaces an existing file) - **FIXED 2026-11-03, PR #32**

**What happened.** `awt list --html <path>` overwrote whatever was at that path, with exit 0 and no warning. `list` otherwise only reads, so a mistyped path could destroy a file from a command nobody would expect to be dangerous.

**Decision (2026-11-03): the engine refuses.** It exits 2, before the scan, and leaves the file alone. There is no override flag, so refreshing an inventory now takes a delete or a new name. Add a flag later only if that proves annoying.

### AR-09 (`archive` copies less than it promises) **[FIXED 2026-11-04, decided at 1.9: option A]**

**What happened.** The 2026-11-04 re-run checked a first sweep against the scratch copy. All 3,957 top-level transcripts and 24,683 of 24,689 file-history files were copied. The six others are likely orphans of sessions with no surviving transcript (inferred). The sweep did not copy 10,405 subagent transcripts (5.8 GB), 3,666 `tool-results` files (837 MB), or anything from `session-env/`, `tasks/` or `todos/`.

**Why.** `archive_project_dir` in `crates/awt-core/src/archive.rs` copies only `*.jsonl` directly inside a project folder, plus `file-history/<sessionId>/`. AC-35 in the S-02 spec also names session-env, tasks and todos, and the design's layout names `tool-results` and `subagents`. Only file-history has a test, so the gap never showed.

**Effect.** Nothing was lost by running `archive`, and a move and a rollback were unaffected. Files it skipped remained exposed to auto-delete. The S-02 spec is unchanged, because the fix meets AC-35 as written.

**Fixed.** `archive` now copies every file below every project subfolder except `memory/` (so subagents, `tool-results/` and `workflows/`, including session folders with no transcript beside them), and `session-env/<id>/`, `tasks/<id>/` and the matching `todos/` files for every session id, beside the existing file-history. `--session` and the `SessionEnd` hook archive the same set for their one session. A file that cannot be read, such as a lock file held by a running session, is reported as failed and the sweep continues; the exit code stays 0 for that case. Writes go through a temp file, so a failed write leaves no partial copy, and an archive folder inside the walked tree does not copy itself. `awt associate --export` also uses this code and now refuses before re-associating if any export file failed. The pages that carried the narrower claim are restored. What to do next: merge the branch, then rerun the archive steps of the acceptance run against a scratch copy (inferred as sufficient, since the change is confined to `archive.rs`, `associate.rs` and the archive output in the CLI).

---

## How to keep this file honest

Update it whenever a gate changes state, and prefer moving detail *out* of here into the owning
document. This file has now been wrong once - the no-technical-gate claim - and the correction
is kept visible at the top rather than edited away, because the failure mode it guards against
is exactly this: a summary drifting from the evidence underneath it.
