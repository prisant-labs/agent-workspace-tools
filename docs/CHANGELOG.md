# Documentation Changelog

How the planning documents have changed, and how each change affects the others.
Newest first. This is a doc-impact log, not a code changelog.

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
