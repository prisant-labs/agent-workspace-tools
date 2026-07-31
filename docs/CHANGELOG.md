# Documentation Changelog

How the planning documents have changed, and how each change affects the others.
Newest first. This is a doc-impact log, not a code changelog.

## 2026-07-31 - AC-61 and AC-62 closed; every code AC in the safety closeout is done

Phases 17.8 and 17.10, red-first where the behavior changed. Workspace suite 175 tests.
Only the adversarial acceptance run (17.11) and the maintainer's D10/S-01 halves remain.

- **AC-61 (path confinement).** The hook's `transcript_path` check was lexical - lowercase
  prefix comparison - which `<projects>/../../outside` defeats byte-for-byte. It now rejects
  `..`/`.` components outright and canonicalizes both sides against the real filesystem
  before requiring containment; a transcript that does not exist is refused. New
  `FileSystem::is_reparse_point` reads real NTFS reparse attributes (junctions included,
  which std's `is_symlink` has not always reported). Policy split by operation and recorded
  as such: mutation walks REFUSE a junction at plan time (a guard, exit 2, with a TOCTOU
  re-check in the snapshot walk), archive SKIPS it (a protective sweep across all projects
  must not abort on one link). Proven with a real `mklink /J` junction through the binary,
  and a real `..`-escape through hook stdin.
  Worth recording: the first implementation refused inside the snapshot, and the real-FS
  test caught it surfacing as exit 3 ("verification failed") because `apply_verified` wraps
  apply errors - the guard was moved to plan time where a refusal belongs.
- **AC-62 (synthetic fixtures, engineering half).** `scripts/generate-reference-fixtures.py`
  deterministically generates both reference transcripts, preserving exactly what the golden
  tests lock - 2,082 anchored rewrites (227/54 and 1,240/534/27), the 10/55 preserved
  package/branch mentions, line counts 329/2,285, every line valid JSON - and nothing else
  from the originals. 18.1 MB of real conversation replaced by ~540 KB of synthetic data;
  golden tests pass unchanged. `test/fixtures/README.md` rewritten: everything synthetic,
  regeneration rules, and an honest record of what was published until 2026-07-31 and why it
  left. History removal remains the maintainer's D10 call.

## 2026-07-30 (later still) - AC-58 executed, dependabot queue processed, D10 read aid built

Four maintainer decisions collected and executed in one sitting: remove the inert options
(AC-58), aid-read-rewrite for the published transcripts (D10), branch protection after the
D10 rewrite, dependabot processed now.

- **AC-58 executed.** `Collision` and `Scope` enums, the Minimal/Full plan branches,
  `Ctx.scope`, and the three CLI flags deleted end to end; 22 files swept. Nested projects
  are a hard refusal (`NestedProjects`, exit 2) that names the children - the removed
  `--recursive` only ever silenced that warning. The collision guard is unconditional. clap
  rejects the removed flags with exit 2, pinned by a binary test. Workspace suite 168 tests.
  Docs: commands.md flag table and notes, troubleshooting guard entry, glossary Scope entry
  rewritten as historical, quickstart guard list, review-guide AC-3 flips to Proven and the
  AC-18 scope caveat closes, AC-44 amended in the S-02 canonical and pointer specs
  ("refuse by default" becomes "refuse, full stop").
- **D10 read aid** (local-only, `_local/fixture-review/`): every email/URL/base64/secret-shape
  hit across the four fixture transcripts with file and line, pre-triaged - the 60
  `sk-`-prefixed strings are kebab-case task IDs, not keys. Exposure data recorded: 0 human
  views in 14 days but 60 clones from 28 unique sources, which shifts the priority from
  deletion to rotation-if-needed.
- **Dependabot**: checkout 4->7, cache 4->6, and the grouped cargo bumps merged CI-green; the
  sha2 major rebases behind them.

## 2026-07-30 (late night) - three S-04 Highs closed (AC-57, AC-59, AC-60)

Phases 17.5-17.7, red-first; workspace suite 164 tests (158 before).

- **AC-57 (plan-derived verification).** `verify` now accepts the applied plan and asserts
  every planned `claude.json` splice actually landed - destination anchor present, source
  anchor gone, checked as **raw bytes** so the check lives at the same layer the write does.
  Malformed `history.jsonl` lines are verification failures instead of silently skipped.
  Scope-awareness was deliberately not plumbed: the unused `minimal`/`full` tiers are slated
  for removal under AC-58, after which Standard-only verification is scope-correct by
  definition.
- **AC-59 (I/O failure is never absence).** New `walk_files_strict` and `read_dir_optional`
  primitives in `fs.rs`: mutation-path walks (backup, merge-apply) abort on an unreadable
  subtree instead of snapshotting around it; plugin-state scans distinguish "no plugins dir"
  from "could not read it". `apply_verified` treats a verify that ERRORS exactly like a verify
  that fails - rollback, not a stranded applied-but-unproven migration (the old `?` bubbled
  past the rollback branch). An unreadable transcripts dir is a failed check, not "zero
  stale". All proven with a failure-injecting `FailingFs` test double.
- **AC-60 (plugin hash from recorded spelling).** Plugin-state detection and verification
  derive candidate hashes from every recorded `cwd` spelling that normalizes to the source
  path (the reverse index already retained the original spellings), with the caller's own
  spelling as fallback. Previously `e:/projects/a` resolved every store except the plugin dir
  recorded under `E:\Projects\A` - and verify repeated the identical wrong derivation, so the
  stranded dir also passed verification.
- Doc impact: gate (g) progress, S-04 phase table and test map, review-guide AC-18 flips to
  Proven (with the scope caveat named), maintainer-todo, root CHANGELOG Fixed entries.

## 2026-07-30 (night) - the three S-04 Criticals closed (AC-54, AC-55, AC-56)

Phases 17.2-17.4 of the safety closeout, executed red-first; every test failed against the old
code before its fix. Workspace suite 158 tests (144 before).

- **AC-54 (rollback tree loss).** The snapshot for a directory rename is now recursive - every
  file, not top-level `*.jsonl` - and rollback renames the whole directory back BEFORE
  restoring modified files, replacing the restore-then-`remove_dir_all` sequence that destroyed
  unbacked sidecars during the undo. If both old and new directories exist at rollback time it
  refuses loudly rather than merging blind. Proven by tree-map comparison (nested markdown,
  binary blobs, nested transcripts), an injected mid-apply failure exercising auto-rollback
  with the directory already renamed, and an end-to-end run of the real binary on the real
  filesystem, since the fix's core operation is a directory rename and an in-memory model
  could lie about that.
- **AC-55 (missing-source false success).** `build_plan` refuses a folder move whose source is
  not a directory (new `SourceMissing`, exit 2), ordered AFTER the destination-exists guard so
  a re-run of a completed move still reads as "already done" (AC-19's documented idempotency
  signal) rather than "source not found". A source that vanishes between plan and apply is a
  hard failure that triggers auto-rollback. Verify now asserts destination-present and
  source-absent whenever the manifest records a folder move. `associate` is explicitly exempt
  - gone folders are its purpose - with a test pinning that.
- **AC-56 (settings fail-open).** `load_settings` fails closed: only file-not-found may
  initialize a fresh object; read failures, parse failures, invalid UTF-8, and a non-object
  root all refuse (exit 4) with the file untouched. Writes go through a temp file and rename.
- **Doc impact:** troubleshooting gains the source-not-found guard entry; the review guide's
  contested standings for AC-1, AC-16, and AC-17 flip to Proven (AC-18 stays contested by
  AC-57); the S-04 implementation plan records phases and test map; root CHANGELOG's Fixed and
  Known-issues sections updated; maintainer-todo 1.1 reflects progress.
- **Test-suite side effect:** the three CLI plan tests that seeded from the golden fixture
  hardcoded a source path that only exists on the original dev machine; the new source guard
  exposed that latent portability bug. They now build a real temp source dir, so they test the
  same behavior on any machine, CI included.

## 2026-07-30 (evening) - adversarial audit, S-04 safety closeout, v1.1.0 folded into v1.0.0, retractions

An external adversarial code audit (read-only, report kept local per repo convention) found
data-loss and false-success paths that the passing acceptance run never exercises. Every
blocking finding was independently verified against source before being accepted. The verdict
below this entry - "no technical gate remains" - is **retracted**, visibly rather than by
rewriting history.

### New documents

- **`plan_v1.0.0/S-04_safety-closeout/`** - CREATED: spec (AC-54..AC-65) and implementation
  plan. The Criticals: rollback of a directory rename backs up only top-level `*.jsonl` then
  recursively deletes the renamed directory, destroying unbacked sidecars (AC-54); a missing
  source folder is silently skipped and reported as a successful move (AC-55); a malformed
  `settings.json` is replaced with a nearly-empty object on the next settings write (AC-56).
  Highs include plan-derived verification, inert advertised flags, I/O-failure-as-absence,
  plugin hash from caller spelling, lexical path confinement, and the real-transcript fixtures.
  New hygiene gate (g) blocks the tag until closed.
- **`plan_v1.0.0/S-01_mover-cli/review-guide.md`** - CREATED: the sign-off reading aid. Each AC
  in plain language with its evidence and standing (Proven / Test-thin / Contested), what the
  criteria deliberately do not cover, and the mechanics of signing. Takes no position on the
  verdict.

### Fold: v1.1.0 into v1.0.0 (D9, maintainer decision)

- `S-03_history-drive-letter-repair/` moved under `plan_v1.0.0/`; `plan_v1.1.0/` retired. Its
  D7 (corruption cause - now with a precise tripwire: more distinct `::` values than the 12
  known leftovers means the cause is live) and D8 (repair stays narrow) moved into the v1.0.0
  plan; D9 records the fold itself; D10 (fixture publication response) opened as a maintainer
  decision. Root `CHANGELOG.md` sections merged under `[1.0.0]`; ROADMAP version map, index,
  and governance updated.

### Code landed with this entry

- **AC-53a**: `repair` refuses a `history.jsonl` that is not valid UTF-8 (exit 4) instead of
  planning against a lossy decode - my own module from a day earlier violating the AGENTS.md
  never-lossy-rewrite invariant. Red-first test; S-03 spec amended; commands.md updated.

### Impact on existing documents

- **`acceptance-run-2026-07-30.md`** - verdict carries a dated partial retraction: gate (f)
  stands as happy-path evidence; the no-gate-remains conclusion does not.
- **`docs/internal/maintainer-todo.md`** - rewritten around the closeout, with the state
  correction pinned at the top. v2 sequencing recorded as closeout -> frozen core contract ->
  shell.
- **`docs/ROADMAP.md`** - section 7 rewritten; the doctor timing claim replaced with measured
  ranges (warm 5.9-14.1s, cold 98-135s, cache-dominated) instead of a single quotable number.
- **`README.md`** - pre-release callout now names the closeout and the rollback risk.

## 2026-07-30 (later) - acceptance run PASSED; no technical gate remains for v1.0.0

- **`docs/internal/release-plans/plan_v1.0.0/acceptance-run-2026-07-30.md`** - CREATED. The clean
  end-to-end run at `e50eba2`: 15 of 15 steps passed, no new defects, every exit code matched the
  contract, and all four findings from the 2026-07-28 run re-confirmed fixed on real data.
  Deliberately run against a **fresh** scratch copy rather than the previous one, which had been
  mutated by the fix-verification work and so could no longer answer "does this work on untouched
  real data".
- **`plan_v1.0.0.md`** - hygiene gate (f) flips to PASS. The tag row now reads blocked *only* by
  gate (a), the S-01 sign-off.
- **`docs/ROADMAP.md`**, **`docs/internal/maintainer-todo.md`** - updated to say the same thing in
  one sentence: the only thing left is a human reading a spec.
- **`docs/index.md`** - a row for the passing run, and the earlier one relabelled so the pair reads
  as a sequence rather than two competing reports.

Two observations recorded in the report rather than fixed: warm `doctor` measured 14.1s against
5.9s on 2026-07-28 on a slightly larger tree, which is more than the file-count delta explains and
is probably OS cache state after a 3.4 GB copy - worth re-measuring on a quiet machine before
treating the ~8s figure in ROADMAP as authoritative. And the declined-shape warning channel
reported zero on real data, which is the correct outcome and the reason that behavior rests on
unit tests rather than on this run.

## 2026-07-30 - v1.1.0 opened: repair, type warnings, dependabot

The maintainer resolved the three decisions left open after the acceptance run. All three were
approved as recommended and implemented in the same pass.

### New documents

- **`docs/internal/release-plans/plan_v1.1.0/`** - CREATED, with `plan_v1.1.0.md`, the S-03 spec
  (`awt repair --drive-letter`, AC-45..AC-53), and its implementation plan. Theme: repair state
  that is *damaged* rather than merely stale. Explicitly does not gate the v1.0.0 tag. Records
  D7 (what caused the corruption - open) and D8 (repair stays narrow - resolved).

### Impact on existing documents

- **`docs/reference/commands.md`** - an `awt repair` section, and the `doctor` entry now documents
  the warnings channel.
- **`docs/recipes.md`** - a recipe for the damaged-history case, with the stale-versus-damaged
  distinction called out.
- **`docs/faq.md`** - two entries: stale versus damaged, and what the warnings section means.
- **`docs/troubleshooting.md`** - why `repair` reporting things it will not repair is correct
  behavior and still exit 0.
- **`docs/internal/maintainer-todo.md`** - all three open decisions closed. D7 is now the only
  decision waiting on the maintainer.
- **Root `CHANGELOG.md`** - a `[1.1.0] - unreleased` section above the 1.0.0 one.

### A correction worth recording

The earlier figures for the corruption (45 distinct values, 33 repairable) were measured with a
PowerShell `Sort-Object -Unique`, which is **case-insensitive** and collapsed
`::\projects\...\pm-skills` and `::\Projects\...\pm-skills` into one entry. The tool counts 46 and
34, which is correct: the rewrite is a case-sensitive byte splice, so each case variant is a
distinct literal needing its own rule. The recoverable line count, 2,303, was unaffected. The
figures are corrected in the spec, the v1.1.0 plan, the acceptance-run report, and the module and
test documentation.

## 2026-07-28 (later) - AR-02, AR-03, and AR-04 fixed; all acceptance findings closed

Follow-up to the acceptance run below. The maintainer's call on the two open findings was "fix
now", and a fourth defect surfaced during the work.

- **AR-04 is new**, found while re-verifying AR-01 rather than during the run. Two
  `githubRepoPaths` slugs can hold the same path value; each planned its own edit expecting one
  match, while each edit counts across the whole file and saw two. It was invisible until AR-01
  was fixed, because the anchor previously matched nothing at all - the two defects were stacked
  in one expression. Recorded as its own section in the acceptance-run report, including why a
  changed error message (`live 0` becoming `live 2`) is progress rather than success.
- **`docs/reference/commands.md`** - the `--json` caveat is removed; the flag now works on every
  subcommand, and the entry documents the plan model as the v2 parity contract.
- **`docs/recipes.md`** - the AR-02 limitation callout is replaced with the positive behavior:
  `associate` handles expired transcripts, which is the normal case rather than an edge case.
- **`docs/ROADMAP.md`**, **`plan_v1.0.0.md`**, **`docs/internal/maintainer-todo.md`** - all four
  findings marked fixed. The only technical gate left is a clean end-to-end acceptance re-run;
  the only human gate is the S-01 sign-off.
- **Root `CHANGELOG.md`** - AR-02, AR-03, AR-04 moved from Known issues to Fixed, and an Added
  section records `plan --json` and `verify --json`.

Note for v2 planning: implementing `awt plan --json` means the GUI parity gate
(`GUI plan model == awt plan --json`) is now expressible against the shipped binary. It was
previously unwritable, and it gated the start of GUI work.

## 2026-07-28 - first acceptance run (FAILED), AR-01 fixed, user-doc suite, CI hardening

The manual acceptance run was executed for the first time and **failed**, finding a
release-blocking defect (AR-01), which was then fixed in the same session. Documentation was
reconciled against reality in the same pass, and the user-facing doc set was filled out.

Two conventions in `AGENTS.md` and `CONTRIBUTING.md` were added as a direct consequence of how
AR-01 hid: every fixture must be referenced by a test, and rewrite tests must assert on raw file
bytes. Both are now also checklist items in the new PR template.

### New documents

- **`docs/internal/release-plans/plan_v1.0.0/acceptance-run-2026-07-28.md`** - CREATED. Full
  result of the run: 11 of 13 steps passed; AR-01 (release blocker: `claude.json`
  `githubRepoPaths` rewrite fails on JSON escaping) and AR-02 (`associate` refuses
  transcript-less projects). Includes root-cause analysis, the three test-coverage gaps that hid
  AR-01, reproduction steps, and non-defect observations.
- **`docs/internal/maintainer-todo.md`** - CREATED. The single canonical human to-do list,
  aggregating gates that were previously split across the release plan, the runbook, and the
  roadmap. Marks each item HUMAN or DELEGABLE.
- **`docs/faq.md`** - CREATED. Safety and trust, what a move actually changes, retention and the
  30-day cliff, recovery, platform and scope.
- **`docs/recipes.md`** - CREATED. Task-oriented walkthroughs including the already-moved-by-hand
  case, archive-hook setup, and scripting against the exit-code contract.
- **`docs/glossary.md`** - CREATED. The project's vocabulary, from `cwd` and reverse index through
  anchored rewrite, report-only, and the honesty checkpoint.
- **`SECURITY.md`** - CREATED. Private reporting via GitHub advisories, in-scope and out-of-scope
  lists, and the no-network / source-first distribution posture.
- **`scripts/README.md`** and **`scripts/new-scratch-home.ps1`** - CREATED. The scratch-home
  helper used by the acceptance run, plus its documentation.
- **`.github/ISSUE_TEMPLATE/{bug_report,feature_request,config}.yml`**,
  **`.github/PULL_REQUEST_TEMPLATE.md`** - CREATED. The bug form requires version, exit code, and
  output; the PR template requires raw-byte assertions and fixture wiring.

### Impact on existing documents

- **`docs/quickstart.md`**, **`docs/troubleshooting.md`** - FIXED. Both documented
  `awt rollback <path>` as a positional argument; the CLI requires `--report <path>`. Every
  copy-pasted rollback command in the docs was broken.
- **`crates/awt-cli/src/main.rs`** - the `--html` help text claimed the HTML inventory was written
  *instead of* the table when the code writes it *in addition to* the table. Fixed in the code,
  since `docs/reference/commands.md` was correct. `--src`, `--dst`, and `--report` had no help
  text at all and now do.
- **`docs/ROADMAP.md`** - Section 7 rewritten: v1.0 is feature-complete but NOT releasable. The
  "remaining work is all non-code" claim is now false and was removed. The `doctor` timing claim
  gained its missing caveat (5.9s warm, 98s cold on a 3.3 GB home).
- **`docs/internal/release-plans/plan_v1.0.0/plan_v1.0.0.md`** - hygiene gates re-evaluated for
  the first time since 2026-07-11. Gate (d) had read FAIL only because the table predated
  implementation; it is PASS. New gate (f) for the acceptance run, which reads FAIL. Doc checklist
  reconciled row by row. D4 and D5 resolved as superseded by events; D6 opened for the
  `history.jsonl` drive-letter corruption.
- **`AGENTS.md`** - the honesty-checkpoint baseline is restated as a procedure rather than fixed
  counts, which had drifted from 6/11/1 to 20/48/5/2 by store. Two new conventions added: every
  fixture must be referenced by a test, and rewrite tests must assert on raw bytes.
- **`docs/decisions/0001-project-name-agent-workspace-tools.md`** - the rename consequence is
  updated from future tense to landed (2026-07-24), and Confirmation records the one residue that
  survives in the maintainer's live `~/.claude`, which AR-02 currently prevents cleaning up.
- **`README.md`** - the pre-release callout now names the AR-01 blocker rather than saying the
  acceptance run has not happened. Status table corrected. Quick start uses the scratch-home
  script and warns that `--home` does not redirect the folder move.
- **`docs/index.md`** - rows added for every new document, per the orphan rule.
- **`.github/workflows/ci.yml`** - `push` restricted to `main` so a PR branch commit is built
  once rather than twice. A concurrency group alone does **not** fix that duplication, which was
  confirmed empirically on this branch: the push and `pull_request` events carry different
  `github.ref` values (`refs/heads/<branch>` versus `refs/pull/<n>/merge`), so they land in
  separate groups and neither cancels the other. The concurrency group is kept for the case it
  genuinely covers, abandoning a superseded run when the same branch is pushed twice quickly.
  Also: the no-network gate widened from `awt-core` alone to every workspace package,
  `cargo-audit` pinned and cached instead of recompiled each run, `--locked` added throughout,
  and a release build step added.

## 2026-07-24 - session logs move to `_local/_session-logs/` (gitignored)

New maintainer standard: session logs are local-only working notes, not repository artifacts.

- Moved every session log from `_agent-context/session-log/` to `_local/_session-logs/` and removed
  the now-empty `_agent-context/`. `_local/` is already gitignored, so logs are no longer tracked.
- The four previously-committed logs are removed from the working tree. Note: they remain in git
  history; removing them from history would require a rewrite (filter-repo/BFG + force-push), which
  was deliberately NOT done.
- Convention updated in `.gitignore`, `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, and ROADMAP
  Section 6. `CLAUDE.md` explicitly overrides the `jp-wrap-session` skill's default log path.

## 2026-07-23 - v1.0 docs refresh: status/architecture sync + acceptance-run + CONTRIBUTING

Documentation pass on branch `v1.0-docs-refresh`:
- README status/roadmap table refreshed (feature-complete, all 24 AC verified; the three
  remaining tag gates named); added a pointer to `docs/DESIGN.md` as the architecture doc.
- ROADMAP Section 7 brought current (110 tests; AC-gap remediation and hygiene done; remaining
  gates are the S-01 sign-off, the acceptance run, and the `cpm` -> `awt` rename decision).
- `docs/DESIGN.md`: fixed the stale "v1.1" labels on F13-F15 (they shipped in v1.0) and noted
  that the `--attribute` resolver is deferred (decision 7a).
- Added `docs/acceptance-run.md` (a detailed manual-acceptance walkthrough expanding
  release-runbook Section 2) and `CONTRIBUTING.md` (build/test/lint, conventions, PR flow).

## 2026-07-23 - v1.0 hygiene: quickstart, root changelog, version bump, cleaner errors

Release-prep hygiene on branch `v1.0-hygiene-display`:
- Added root `CHANGELOG.md` (user-facing, keepachangelog form) and `docs/quickstart.md` (a
  first-run walkthrough, safest path first). Both are indexed in `docs/index.md` and the README
  doc map.
- Bumped the workspace `Cargo.toml` version to 1.0.0; updated the README status banner (kept
  honest as "tag pending", not "released", since the tag is still gated).
- Added a `Display` impl for `CpmError` so guard errors surface their plain-language message
  rather than `Variant("...")`.

## 2026-07-23 - S-01 AC-gap remediation + troubleshooting doc

An AC-to-test traceability review of S-01 (mover CLI) found five acceptance criteria that
were detected but not enforced by the shipping build. Closed them on branch
`s01-ac-gap-remediation`:

- AC-1 (`fb400e0`): cross-volume guard - refuse (exit 2) rather than fail at apply.
- AC-21 (`fb400e0`): lock detection wired into `build_plan` (refuse without `--force`).
- AC-7 (`fb400e0`): ambiguous histories now fail closed and surface the candidates (7a);
  the AC-7 wording was amended - the `--attribute` resolver moves to v1.x.
- AC-22 (`ef10583`): machine-readable `report.json` written by default, plus `--json`.
- AC-17v (`dfb9ee6`): verifiable revert - a post-rollback byte-identity proof and
  `rollback-report.json`.
- AC-26 (`485e565`): a deps-guard test that keeps network/LLM crates out of the tree.

AC-19 was amended (19b) to idempotent-by-refusal (exit 2). New docs:
`docs/troubleshooting.md` (exit-code contract, idempotency, guard messages, report
artifacts) and the review evidence at
`docs/internal/release-plans/plan_v1.0.0/S-01_mover-cli/ac-traceability.md`.

## 2026-07-20 - v1.0 feature-complete (Phases 6-9 + F13-F15) with adversarial-review fixes

The engine and CLI are complete. Landed since the Phase 4-5 entry:
- Phase 6 (`e7ec6d0`, `056bd84`): adapter `plan()` methods + `build_plan` (destination-exists,
  git-worktree, and claude.json-collision guards; nested-project detection; folder move last).
- Phase 7 (`e2aa334`): snapshot backup with a sha256 manifest, transactional count-guarded
  `apply`, and an end-to-end golden that applies the reference move against the real 9 MB fixture.
- Phase 8 (`a2801e8`, `c758613`): per-store + aggregate `verify`, `rollback` (pulled forward from
  Phase 9 because `apply_verified` depends on it), `apply_verified` with auto-rollback on any
  failure, lock detection, idempotency, and hard-fail on unrecognized formats.
- Phase 9 (`2285784`): the full `cpm` CLI (`plan`/`apply`/`verify`/`rollback`) with the exit-code
  contract.
- F13 (`c31b1c0`): `cpm list` inventory (session linkage, health flags, 30-day-cliff ages).
- F14 (`be48c94`): `cpm archive` (content-hash incremental copy, INDEX + manifest, retention hook).
- F15 (`82a090f`): `cpm associate` (re-associate and/or from-scoped export; `PlanOpts.move_folder`).

Per-feature Codex adversarial reviews were run and drove three fixes:
- `d66c49b`: `list`/`ProjectIndex` now fail loud on a real read error (a missing projects dir still
  yields an empty result) instead of silently reporting zero - a refuse-rather-than-guess violation.
- `809e94e`: the archive manifest is now cumulative and atomic (it was emptied on incremental
  reruns), and the SessionEnd retention hook reads the hook JSON from stdin (`transcript_path`),
  the real Claude Code hook contract, rather than a nonexistent env var - so the hook actually
  archives.

Doc impact: `docs/ROADMAP.md` Section 7 rewritten from "Phase 6 next" to v1.0 feature-complete;
`docs/reference/commands.md` (per-subcommand reference) and `docs/release-runbook.md` (tag ceremony
plus signing/SmartScreen posture) added; `README.md` command list refreshed; `docs/index.md`
updated. Remaining before the v1.0 tag is the release ceremony and the manual real-machine apply
acceptance - both non-code.

## 2026-07-17 - Phases 4-5 shipped; sweep scope resolved; doctor made fast

Phase 4 (doctor + scan engine and the `cpm` CLI, commits `afc8ecf`, `9c824e0`) and Phase 5
(the anchored rewrite engine, commit `76226bc`) landed on `main`. Phase 5's golden test
reproduces the reference-move counts exactly (1467 + 588 + 27 = 2082) against the real 9 MB
fixture while asserting that package and branch mentions come through byte-identical - the
tool's "changes exactly the paths and nothing else" claim, now under test.

The Phase 3 review's open sweep-scope question was resolved (commit `e068a7e`). The sweep now
skips the owned, archival, and vendored regions (`plugins/`, `file-history/`, `backups/`): a
path match inside a backup or a file-history snapshot is correct by design, not rot, so
reporting it was the same "a match is not staleness" error as the C-1 audit bug, one layer up.
This cut `doctor` from ~345s to ~8s (it had been content-reading ~34k files, ~29k of them
vendored plugin files) and its sweep findings from 52 to 1. Sweep results moved from the
`stale` vector to a new `DoctorReport.report_only` vector, so the Phase 6 rewrite path cannot
reach an unowned region.

Doc impact: `docs/ROADMAP.md` Section 7 (Current status) updated from "execute Phase 1" to
Phases 1-5 complete, the doctor honesty checkpoint passed, and Phase 6 next. `docs/DESIGN.md`
store table sweep row updated to name the skipped regions and the report-only split. The
v0.1.0 milestone gate (the doctor honesty checkpoint) is now met and the tag is ready to cut.

## 2026-07-17 - Project renamed to agent-workspace-tools; first decision record

The project is renamed from `claude-project-mover` to `agent-workspace-tools`. The
old name described a single feature (move) and the `claude` prefix blocked the Codex
and Gemini adapters the roadmap already parks as v1.x work. The reasoning, the
rejected alternatives (`claude-project-manager`, `claude-session-tools`,
`claude-workspace-tools`, and `-utilities` as a suffix), and the deferred binary and
crate renames are recorded in the project's first formal decision record,
`docs/decisions/0001-project-name-agent-workspace-tools.md`. This adds the
`docs/decisions/` directory (MADR v4), now indexed in `docs/index.md`.

Past session logs, prior entries in this changelog, and the archival regions under
`~/.claude` keep the old name on purpose: they are historical records, not live
pointers, and rewriting them would be the same "a match is not staleness" error the
tool is built to avoid.

## 2026-07-13 - Reverse index redesigned against real data; LEAD-07 case-sensitivity closed

A scan of the maintainer's real machine (45 project dirs, 11,518 transcripts, run against a backup
copy) drove a redesign of `ProjectIndex`, and settled several numbers the docs had been asserting
without ever checking them.

**What the scan found.** One directory, `E--Projects-prisant-labs-obsidian-tag-visibility`, holds 17
transcripts recording THREE different paths: `prisant-labs\obsidian-tag-visibility` (9 transcripts,
still on disk), `prisant-labs\obsidian-tag-curator` (4, gone), and `github-jprisant\obsidian-tag-curator`
(2, gone). This is move residue: the project was relocated twice and its transcripts were physically
moved into the new folder without their internal path references being rewritten. It is exactly the
condition CPM exists to repair, sitting in the maintainer's own data.

**Why the old index was wrong.** `ProjectIndex::build` read the first transcript that yielded a `cwd`
and stopped. On this directory it produced the right answer only because the alphabetically-first
UUID happened to hold the live path - a coin flip. And it discarded the 6 stale references entirely,
so `doctor` could never have reported them.

**The redesign.** `build` now collects every distinct `cwd` per directory and resolves against what
still exists on disk: one recorded path resolves to it; several with exactly one survivor resolve to
the survivor and file the rest as `stale`; several survivors is genuine ambiguity and the tool refuses
rather than guesses. `ProjectIndex` gains `ambiguous` and `stale` fields. This is the first code in the
project to actually construct `CpmError::Ambiguous`, which had been defined and wired to exit code 2
since the original plan and never once raised.

**LEAD-07 (case-sensitivity half) closed as a direct consequence.** Resolution now asks the filesystem
whether a recorded path still exists, and NTFS answers case-insensitively while `MemoryFileSystem`
answered case-sensitively. That made a tracked-but-dormant divergence load-bearing: a transcript
recording `e:\projects\foo` against an on-disk `E:\Projects\Foo` would resolve one way in a test and
the other way on a real machine. `MemoryFileSystem` now models NTFS - case-insensitive lookup,
case-preserving output. The empty-directory and separator halves of LEAD-07 remain open.

**Corrections to asserted figures.** DESIGN.md said "dirs with no transcripts (16 of 45) have no
recoverable cwd". The real number is **15 of 45**, and those directories are **not empty** - they hold
transcripts that never recorded a `cwd`. A Task 2.2 review had also claimed empty directories were the
main population of `unresolved` and were untestable; the scan disproves it.

Doc impact: `docs/DESIGN.md` "Reverse index" rewritten with the resolution table and the corrected
figures. The Task 1.2 and Task 2.2 code blocks in
`docs/superpowers/plans/2026-07-10-claude-project-mover.md` are re-synced to the shipped code, each
carrying a repair note, so an agent transcribing from the plan cannot reintroduce either defect.

## 2026-07-12 - Task 2.1 reviewed; `same_volume` UNC defect repaired in code and plan

Task 2.1 (path encoding, commit `54e4c2b`) passed its spec + quality review gate, the last
unreviewed code on the branch. Spec compliance passed: the commit is a faithful transcription of
the plan. The review nonetheless found a real defect, because the defect was in the plan itself.

`same_volume` derived its volume root from `normalize_path(p).split('/').next()`. `split` emits an
empty field for the run before the first separator, so every UNC path (`\\server\share\...`)
reported a root of `""` and compared equal to every other UNC path: two different file servers read
as the same volume. Phase 6 (`plan.rs`) wires `same_volume` to the rename-versus-copy decision, and
a rename across file servers fails at the OS level. UNC is a Windows path form, so the Windows-only
v1 scope did not excuse it, and UNC appears nowhere in `docs/DESIGN.md`, `AGENTS.md`, or
`docs/ROADMAP.md` - it was never considered rather than deliberately deferred.

Repaired: `root()` now special-cases a leading `//` and returns `//server/share` as the volume
identity, with `same_volume_distinguishes_unc_servers_and_shares` pinning the contract.

Adversarial verification of that repair found the same bug a second time, in the verbatim path form:
`\\?\UNC\server\share\...` parsed `?` as the server and `unc` as the share, so every verbatim UNC
path collapsed to one root regardless of server. `std::fs::canonicalize` emits verbatim paths on
Windows, so the form arrives in practice. DESIGN.md names `dunce` as the verbatim-path mitigation,
but `dunce` is declared in `crates/cpm-core/Cargo.toml` and is **never called anywhere in the
source**: the mitigation is documented, not enforced. `root()` now strips the verbatim prefix
itself rather than trusting an upstream that does not exist, and
`same_volume_sees_through_verbatim_prefixes` pins it. Wiring `dunce` at the path-input boundary
remains open for the phase that first accepts user-supplied paths.

Doc impact: the corrected code and test are written back into the Task 2.1 section of
`docs/superpowers/plans/2026-07-10-claude-project-mover.md`, carrying a "Repaired 2026-07-12" note,
so an agent re-reading the plan cannot transcribe the defect back in. The function's doc comment
previously claimed it handled a "leading mount segment"; it never did, and now states the POSIX
limitation plainly. POSIX mount detection remains genuinely deferred under DESIGN.md "Platform
scope" and ROADMAP CI-2. The S-01 completion table, which still showed all nine phases as "Not
started", now reflects Phase 1 complete and Phase 2 in progress.

## 2026-07-12 - v2 GUI design brief

### New documents

- **`docs/internal/v2-gui-design-brief.md`** - self-contained handoff for a designer or design
  agent generating v2 GUI concepts. Inlines the F1-F15 feature and function breakdown, the user
  context and failure stakes, the CLI surface as a reference model, and the hard constraints the
  design must respect (AC-25 parity with the CLI's plan objects, plan-before-write, backup-before-
  write, refuse-rather-than-guess, zero network). Readable with no repo access and no machine-
  specific paths, so it can be handed to an agent outside this environment.

Doc impact: none on existing documents. The brief is derived from `docs/DESIGN.md`, the S-01
spec, `docs/features/v1.1-inventory-retention-reassociate.md`, and `docs/ROADMAP.md`; it restates
them for a design audience and is not a source of truth. Note that S-01 is still `status: draft`
with `requires-human-review: true`, so the acceptance criteria the brief describes are not yet
maintainer-signed.

## 2026-07-11 - Phase 1 built; preserved-mention counts corrected to measured totals

Phase 1 of the TDD plan executed (workspace scaffold + CI gates, FileSystem trait,
sanitized golden fixtures; commits d17ec2f, b6bbf41, 25dd5ab; final whole-branch
review: READY WITH NOTES). Doc impact: the plan's Reference data and DESIGN.md
correctness rule 2 recorded the larger transcript's per-file preserved-mention counts
(8/49) as totals; corrected against the captured fixtures to 10 (2+8) and 55 (6+49),
matching `test/fixtures/move.json` and `test/fixtures/README.md`. The three
rewrite-count constants (1467/588/27) were confirmed exact and are unchanged.

## 2026-07-11 - Release scaffolding (v1.0.0), roadmap, and repo hygiene

Scope decision of record (maintainer directive): v1 = everything the CLI does, v2 = the
GUI. The old "v1.0 mover" + "v1.1 features" numbering folds into release v1.0.0; old
phases 10-11 (cross-volume AC-2, Codex/Gemini AC-27) park as promotable v1.x backlog;
the GUI (old phase 12, AC-25) becomes v2.0.0.

### New documents

- **`docs/ROADMAP.md`** - program plan across v1 and v2: version map, code workstream
  with the doctor honesty-checkpoint gate, five-stage CI progression (CI-0 scaffold
  gates through CI-4 Tauri/updater), documentation matrix for three audiences
  (non-engineers, engineers, agents), governance rules.
- **`docs/internal/release-plans/plan_v1.0.0/`** - release plan per the jp-release-plan
  convention (`plan_v1.0.0.md`: theme, aggregation, hygiene gates, doc-update checklist,
  decisions D1-D5) plus two effort folders: `S-01_mover-cli/` (spec.md carved from the
  local-only umbrella spec CPM-01 - AC-1, AC-3..24, AC-26 now committed for the first
  time, status draft pending maintainer review - and an implementation-plan wrapper
  mapping phases 1-9 to AC) and `S-02_inventory-retention-associate/` (pointer spec to
  the committed F13-F15 spec with an AC-28..44 index, plus a phases 13-15 wrapper).
- **`README.md`** - rebuilt from the 1-line stub: problem, commands, safety model,
  status/roadmap, documentation map, contributor pointer (71 lines).
- **`LICENSE`** (MIT), **`.gitignore`** (_local/, .memsearch/, .impeccable/, target/,
  *.bak, OS noise), **`AGENTS.md`** (agent operating manual: hard invariants,
  conventions, environment notes, execution mode), **`CLAUDE.md`** (thin overlay),
  **`docs/index.md`** (documentation index with audience labels).

### Impact on existing documents

- **`_local/initial-discovery/04-requirements.md`** - SUPERSEDED in part (local-only,
  left as-is): sections F1-F10+F12 and AC-1..24+26 now live in the committed S-01 spec;
  AC-2/AC-25/AC-27 recorded there as deferred.
- No canonical doc moved: `docs/features/` and the TDD plan remain the AC and plan
  sources of record (release-plan decision D3).

## 2026-07-11 - Audit-repair pass on the plan and design

Applied the design-stage audit's plan-code and documentation fixes. Source of findings:
`_local/audit/2026-07-10_fable-audit/AUDIT_REPORT.md` (local-only, intentionally NOT
committed). No product code exists yet; these are edits to the TDD plan's embedded Rust
and to `DESIGN.md`, converting prose-mandated safety behaviors into step code plus red
tests and aligning the design of record.

### Impact on existing documents

- **`docs/superpowers/plans/2026-07-10-claude-project-mover.md`** - EDITED. Backup
  snapshot now wholesale-copies pre-rename transcripts with a red test (B-01);
  `claude_history` rewrites every stored path variant (LEAD-03); the plugin state dir is
  named from the destination basename and `plugin_state::audit` is implemented, backed by
  a new `ProjectIndex.cwds` field (LEAD-04); Task 1.3 gains a fixture sanitize/minimize
  step and `test/fixtures/README.md` (LEAD-09); `apply_verified` rolls back on a
  mid-apply error and names the backup dir on every failure path (LEAD-01); `rollback`
  sha256-checks each backup (A-01); `verify` takes an optional manifest for the line-count
  postcondition (LEAD-08); the write path hard-fails on invalid UTF-8 (LEAD-02); the two
  `audit()` methods route existence through the injected FS (B-02); compile hygiene fixed
  (LEAD-06); `Scope` tiers implemented and wired to `--scope` (B-05); CI gains no-network
  and `cargo audit` gates (E-03, A-07); the archive engine covers unresolved dirs,
  session-artifacts, and a real manifest/INDEX (LEAD-05); `cpm list` wires real
  PATH-keyed counts for AC-31 (LEAD-10). A Self-Review "Audit-repair pass" entry lists
  every finding and the new cross-task mechanical changes.
- **`docs/DESIGN.md`** - EDITED. Store trait signatures aligned to the plan as built with
  `apply` documented as centralized and `probe` returning `()` (B-06); v1 platform scope
  stated as Windows-only, macOS deferred (C-03/E-01); a note that `_local/` is
  intentionally uncommitted and Section 2 is the authoritative extraction (F-02); exit
  code `1` (unexpected I/O error) documented alongside 0/2/3/4 (B-04).

### Not changed

The audit's repo-state and forward-looking items (README rebuild, LICENSE, `.gitignore`,
signing/release engineering, macOS bring-up, phase-12 GUI security baseline) are out of
scope for this doc-repair pass and remain on the roadmap in the audit report.

## 2026-07-10 - Add three features (F13-F15) and two learning docs

### New documents

- **`docs/reference/claude-data-model.md`** (learning) - defines the full surface
  of Claude Code project state: folder = project = an absolute path string; the
  corrected encoding; the PATH/SESSION/GLOBAL keying taxonomy; every data type;
  the retention rules and the `cleanupPeriodDays: 0` footgun; how to enumerate
  everything for one project. This is now the authoritative store enumeration that
  `DESIGN.md` Section 4 and the feature spec reference.
- **`docs/reference/existing-solutions.md`** (learning) - prior-art survey across
  inventory / retention / relocation / export. Confirms CPM's wedge (Windows-native
  mover; no tool offers inventory + retention + move together) and lists tools to
  cite and to reuse rather than rebuild.
- **`docs/features/v1.1-inventory-retention-reassociate.md`** (spec) - defines
  F13 `cpm list`, F14 `cpm archive`, F15 `cpm associate`, with AC-28 through AC-44.

### Impact on existing documents

- **`docs/DESIGN.md`** - EDITED.
  - Section 7 (CLI): added the `list`, `archive`, `associate` commands.
  - Section 9 (phase plan): added phases 13-15 (v1.1); clarified v1.0 = phases 1-9,
    deferred = 10-12. Phases renumbered in intent only (13-15 are new; 10-12
    unchanged).
  - New Section 10 (v1.1 features) summarizing F13-F15 and the retention hazard.
  - Old Section 10 (Non-goals) renumbered to Section 11 and extended: F14 archival
    is explicitly NOT general cross-machine sync; do not rebuild a transcript viewer.
- **`docs/superpowers/plans/2026-07-10-claude-project-mover.md`** - EDITED.
  - Added Phases 13-15 (Tasks 13.1-15.2) with full TDD steps and Rust code, inserted
    before the Deferred section.
  - Self-review addendum covering F13-F15 coverage and two new cross-task mechanical
    changes: `FileSystem` gains `mtime_secs`; `PlanOpts` gains `move_folder: bool`.
- **`_local/initial-discovery/02-hidden-files-and-external-state.md`** - SUPERSEDED
  in part (gitignored, left as-is). Corrections now live in `claude-data-model.md`:
  the `usage.db` size was 15.5 MB not "15.5 GB"; `githubRepoPaths` and plugin state
  dirs were missing from the surface map; the SESSION-keyed stores are now formally
  enumerated with the sessionId join.
- **`_local/initial-discovery/03/04/05`** - SUPERSEDED in part (gitignored). The
  tech-stack recommendation (TypeScript) is reversed to Rust in `DESIGN.md`; the
  encoding rule is corrected; the feature list is extended from F1-F12 to F1-F15.

### New empirical findings this session (evidence base for the above)

- **Transcripts are auto-deleting at 30 days on this machine right now.** Of 2,647
  transcripts, oldest 30 days, median 28; nothing older survives. No
  `cleanupPeriodDays` is set, so the 30-day default is active. This is the reason
  F14 (retention) is time-sensitive.
- **`cleanupPeriodDays: 0` is unsafe to rely on** - docs say it disables cleanup;
  community issue #23710 says it disables transcript writing; #62272 says cleanup is
  mtime-based. F14 uses a large finite value and content-hash dedup instead.
- **`history.jsonl` never expires** (prompts persist), while transcripts do - opposite
  lifetimes. Documented in `claude-data-model.md` Section 5.
- **A real gone-folder project exists** (`relational-connection/fixed`), used as the
  F15 fixture: its logs remain although the folder is deleted.
- **Session-keyed stores link cleanly by sessionId**: a project's transcript
  basenames match entries in `todos/`, `file-history/`, `session-env/`, `tasks/`.
  This join backs F13, F14, and F15.
- **Prior art is Unix-first**: ~5 movers (clamp, claudepath, skydiver) all need WSL
  or Git Bash. Windows-native is CPM's differentiator.

## 2026-07-10 - Initial validated design and plan (mover, v1.0)

- **`docs/DESIGN.md`** - CREATED. Validated design for the mover: Rust `cpm-core`
  (no Tauri/clap), reverse index, six store adapters + report-only sweep,
  parse-validate-never-serialize with anchored count-checked rewrites,
  backup/apply/verify/auto-rollback, doctor milestone. Corrected three empirical
  errors in the discovery docs and reversed the tech-stack call to Rust.
- **`docs/superpowers/plans/2026-07-10-claude-project-mover.md`** - CREATED.
  9-phase, 23-task TDD plan; golden test reproduces the 2026-07-09 reference-move
  counts exactly.
- Supersedes the tech-stack/encoding claims in `_local/initial-discovery/03-05`.
