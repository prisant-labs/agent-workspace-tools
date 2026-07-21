# Documentation Changelog

How the planning documents have changed, and how each change affects the others.
Newest first. This is a doc-impact log, not a code changelog.

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
