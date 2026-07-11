# Documentation Changelog

How the planning documents have changed, and how each change affects the others.
Newest first. This is a doc-impact log, not a code changelog.

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
