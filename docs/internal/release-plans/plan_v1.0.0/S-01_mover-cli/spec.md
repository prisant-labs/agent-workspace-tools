---
id: S-01
title: "CPM v1 mover CLI - deterministic Claude-state-aware project relocation"
type: spec
status: draft
created: 2026-07-11
updated: 2026-07-11
target-release: v1.0.0
linked-release: docs/internal/release-plans/plan_v1.0.0/plan_v1.0.0.md
linked-plan: ../../../../superpowers/plans/2026-07-10-claude-project-mover.md
ac-count: 24
requires-human-review: true
supersedes: "_local/initial-discovery/04-requirements.md (CPM-01, sections F1-F10+F12; local-only)"
---

# S-01: CPM v1 mover CLI - deterministic Claude-state-aware project relocation

> Carved from umbrella spec CPM-01 (`_local/initial-discovery/04-requirements.md`),
> which is gitignored by design. This committed spec covers the v1.0.0 mover CLI
> scope (F1-F10, F12, phases 1-9). Cross-volume move (AC-2), GUI (AC-25), and
> Codex/Gemini adapters (AC-27) are deferred; see Deferred Acceptance Criteria.
> Where the umbrella and the approved design (`docs/DESIGN.md`) disagree on the
> encoding rule or tech stack, DESIGN wins and this spec reflects DESIGN.

## Acceptance Criteria Fulfillment

- [ ] **AC-1** - Same-volume move executes as a rename
- [ ] **AC-3** - Move refuses on destination collision
- [ ] **AC-4** - Git worktree sources detected and refused/warned
- [ ] **AC-5** - Discovery enumerates all path-keyed Claude stores
- [ ] **AC-6** - Discovery reads stored cwd to map history to folder
- [ ] **AC-7** - Rename/shared-history straddle is detected and surfaced
- [ ] **AC-8** - `projects/<encoded>` dir renamed to new encoded path
- [ ] **AC-9** - Path encoding matches Claude Code's actual scheme
- [ ] **AC-10** - Transcript `cwd` rewritten exactly
- [ ] **AC-11** - Abs-path rewrites are boundary-anchored only
- [ ] **AC-12** - Non-path mentions (package/branch/prose) preserved
- [ ] **AC-13** - `~/.claude.json` keys migrated, all variants
- [ ] **AC-14** - `history.jsonl` `project` field rewritten
- [ ] **AC-15** - Dry-run plan shows every change before apply
- [ ] **AC-16** - Backup snapshot taken before any write
- [ ] **AC-17** - Rollback restores pre-migration state
- [ ] **AC-18** - Post-apply verification proves each postcondition
- [ ] **AC-19** - Re-running on migrated project is a no-op (idempotent)
- [ ] **AC-20** - Unrecognized store format hard-fails, no partial write
- [ ] **AC-21** - Live-process/lock detection warns before edit
- [ ] **AC-22** - Machine-readable migration report emitted
- [ ] **AC-23** - CLI exposes plan/apply/verify/rollback
- [ ] **AC-24** - CLI exit codes are script-friendly
- [ ] **AC-26** - Zero LLM/network calls in the migration path

### Currently In Progress

None.

## Purpose

Provide a deterministic, verifiable tool that relocates a project directory and
migrates all associated AI-CLI (Claude Code) session and log state keyed to the
project's old absolute path, so that `--resume` and per-project config continue to
work at the new location, with no LLM usage, no data loss, and a provable result.
The manual precedent and its gaps are recorded in the local discovery notes
(`_local/initial-discovery/01-move-log.md`) [S1]; the full surface it must cover is
mapped in `_local/initial-discovery/02-hidden-files-and-external-state.md` [S2].
Empirical corrections to those notes are in `docs/DESIGN.md` Section 2.

**Implementation stack:** Rust (`cpm-core` + `cpm-cli` crates), reversing the
strategy brief's TypeScript recommendation. Rationale and portfolio evidence in
`docs/DESIGN.md` Section 3.

## Scope

### In Scope (v1.0.0)

- **F1** - Folder move, same-volume only (atomic rename). Cross-volume copy+verify
  is deferred to phase 10 (AC-2 parked backlog).
- **F2** - Discovery / scan: enumerate path-keyed stores, read stored cwd, detect
  variants.
- **F3** - Mapping and disambiguation: map history to folder, detect
  rename/shared/clone-straddle.
- **F4** - Plan / dry-run: human diff + machine plan before any write.
- **F5** - Backup and rollback: snapshot before write, single-command restore.
- **F6** - Apply engine: dir renames, boundary-anchored text rewrites, byte-preserving.
- **F7** - Verification: postcondition assertions, count checks, JSON parse and
  line-count parity.
- **F8** - Store adapters: Claude Code stores only (`projects/`, `claude.json`,
  `history.jsonl`, `githubRepoPaths`, plugin state dirs). Codex/Gemini adapters are
  deferred to phase 11 (AC-27 parked backlog).
- **F9** - Safety and idempotency: live-process/lock detection, re-run no-op,
  hard-fail on unknown format.
- **F10** - CLI: `doctor`, `scan`, `plan`, `apply`, `verify`, `rollback`
  subcommands; exit codes; flags.
- **F12** - Reporting: machine-readable migration report artifact.

### Out of Scope (v1.0.0)

- **Cross-volume move (AC-2):** `copy + checksum-verify + delete` path. Deferred to
  phase 10, parked backlog.
- **GUI (AC-25, F11):** cross-platform frontend over the shared core. Deferred to v2
  (phases 12+, Tauri + React).
- **Codex/Gemini adapters (AC-27, F8 extension):** opt-in adapter layer. Deferred to
  phase 11, parked backlog.
- Rewriting opaque SQLite telemetry/usage DBs by default (`usage.db`, Codex
  `logs_2.sqlite`). Inspect read-only at most. [S2]
- Any LLM inference, cloud call, or network dependency in the migration path. [S3]
- Editing project-internal files that already move with the folder, unless they
  hardcode their own absolute path (then REVIEW, not auto-rewrite). [S2]
- Cross-machine sync, general backup/restore product, and multi-project batch mode.
  (F13-F15, v1.1 features, are separate scope in S-02.) [S3]

## Users / Actors

- **Solo developer (primary)** relocating/renaming their own projects on Windows
  first, other OSes later. Runs the CLI locally. [S3]
- **Automation/script** invoking the CLI non-interactively (batch reorg, CI). [S3]
- **Future open-source users** of the Claude Code CLI. [model-inference]

## Feature / Function Breakdown

| ID | Feature | Core functions | v1.0.0? |
|----|---------|----------------|---------|
| F1 | Move engine | same-volume rename; cross-volume copy+verify+delete; collision + worktree guards | partial (same-volume; cross-volume deferred) |
| F2 | Discovery / scan | enumerate path-keyed stores; read stored cwd; detect variants | yes |
| F3 | Mapping and disambiguation | map history->folder; detect renamed/shared/clone-straddling history | yes (detect+prompt) |
| F4 | Plan / dry-run | produce human diff + machine plan of every change | yes |
| F5 | Backup and rollback | snapshot before write; single-command restore | yes |
| F6 | Apply engine | dir renames; boundary-anchored text rewrites; byte-preserving | yes |
| F7 | Verification | assert postconditions; count checks; JSON parse + line-count parity | yes |
| F8 | Store adapters | Claude projects / claude.json / history.jsonl / githubRepoPaths / plugin state (v1.0.0); Codex; Gemini (opt-in, deferred) | Claude only |
| F9 | Safety and idempotency | live-process/lock detection; re-run no-op; hard-fail on unknown format | yes |
| F10 | CLI | `plan` / `apply` / `verify` / `rollback`; exit codes; flags | yes |
| F11 | GUI | cross-platform; renders same plan objects; per-item toggles | v2 |
| F12 | Reporting | machine-readable migration report artifact | yes |

## Requirements

**Move engine (F1).** The tool must relocate a directory. When source and
destination share a volume, it must use an atomic rename for speed; across volumes
it must copy, verify by checksum, then delete the source, never deleting before
verification [S1][S3]. It must refuse when the destination already exists [S1], and
detect git worktree sources (`.git` is a file, not a dir) and refuse or warn, since
moving a worktree breaks its linkage [S1]. Cross-volume move is deferred to phase 10.

**Discovery and mapping (F2, F3).** The tool must enumerate every path-keyed store
listed in the migration surface map [S2] and determine which session history belongs
to the moved folder by **reading the stored `cwd`/`project` values**, not by assuming
a 1:1 folder-name match [S1][S2]. It must detect the case where a history's stored
path differs from the current folder name (rename or shared clone) and surface it for
a decision rather than guessing [S1].

**Encoding and rewrite correctness (F6, F8).** The `projects/` directory rename must
use Claude Code's real encoding - every character not in `[A-Za-z0-9]` (every
non-alphanumeric character) replaced by `-`; verified: `E:\Projects\prisant-labs\audiobook-organizer`
encodes to `E--Projects-prisant-labs-audiobook-organizer`, and
`...\agent-skills-toolkit\.claude\worktrees\f2-phase-c-advisory` encodes to
`...-agent-skills-toolkit--claude-worktrees-f2-phase-c-advisory` (dots become `-`).
[S1][S4] The umbrella's narrower claim (`: \\ / space` only) is superseded by
`docs/DESIGN.md` Section 2, which is authoritative. This encoding is lossy and
cannot be inverted; discovery must build a reverse index from the `cwd` stored inside
transcripts, not compute `encode(src)` and look it up [S4]. Text rewrites must be
boundary-anchored (exact `cwd` field; abs-path prefixes followed by a separator or
terminating quote) and must never alter non-path occurrences such as npm package
names (`markdown-for-humans@0.2.1`), branch names (`markdown-for-humans_dev-*`), or
prose [S1]. Files must be written back byte-for-byte except at the intended edits
(line count unchanged, still valid JSON) [S1].

**Config and history stores (F8).** `~/.claude.json` project keys must be migrated
for every slash/case variant that occurs, matching only keys (quoted-path + colon),
never string values [S1]. The `githubRepoPaths` slug-to-paths map must also be
updated (six entries stale at empirical verification) [S4]. `~/.claude/history.jsonl`
`project` fields for the moved paths must be rewritten - the gap the manual move left
open [S1][S2]. Plugin state dirs under
`plugins/data/<plugin>/state/<base>-<sha256(absPath)[:16]>/` must be renamed with the
hash recomputed from the new path [S4].

**Safety model (F4, F5, F7, F9, F12).** Every run must offer a dry-run that shows
the complete plan before any write [S3]. A backup snapshot must be captured before
the first write, sufficient to fully restore [S1][S3]. After apply, a verification
pass must assert each postcondition and report pass/fail [S1][S3]. Re-running on an
already-migrated project must be a no-op [S3]. On encountering a store shape it does
not recognize, the tool must hard-fail before writing anything, not guess [S3]. It
must detect a running CLI/lock on the affected stores and warn [S2][S3]. It must
emit a machine-readable migration report [S3].

**Interfaces (F10).** A CLI must expose `doctor`, `scan`, `plan`, `apply`, `verify`,
`rollback` with script-friendly exit codes [S3]. Exit codes: 0 success; 2
guard/refusal; 3 verification failed; 4 unrecognized format. A cross-platform GUI
(F11) is deferred to v2.

**Determinism (cross-cutting).** No step in the migration path may call an LLM or
the network; identical inputs must produce identical outputs every run [S3].

**Multi-CLI (F8).** Codex and Gemini adapters are deferred to phase 11. In v1.0.0,
only Claude Code stores are touched; their less-settled formats cannot destabilize
the Claude path [S2][S3].

## Acceptance Criteria

Each AC is one observable, testable outcome. Fixtures are seeded from the real
backup `E:\tmp\claude-move-backup-20260709-090053` [S1].

- **AC-1** - Given src and dst on the same volume, When `apply` runs, Then the move
  completes via rename and the source path no longer exists. [S1]
- **AC-3** - Given dst already exists, When `plan` or `apply` runs, Then the tool
  refuses with a non-zero exit and makes no changes. [S1]
- **AC-4** - Given a source whose `.git` is a file (worktree), When `plan` runs,
  Then the tool flags it and does not proceed without an explicit override. [S1]
- **AC-5** - Given a project with Claude state, When `plan` runs, Then the plan
  lists the `projects/<encoded>` dir, the `~/.claude.json` key(s), and the
  `history.jsonl` entries that reference the old path. [S2]
- **AC-6** - Given a history dir whose encoded name does not match the current
  folder, When `plan` runs, Then mapping is decided by the `cwd` stored inside the
  transcripts, not the dir name. [S1][S2]
- **AC-7** - Given a history whose stored path differs from the folder (rename or
  shared clone), When `plan` runs, Then the tool marks it AMBIGUOUS and requires an
  attribution decision (interactive prompt or `--attribute` flag). [S1]
- **AC-8** - When `apply` runs, Then `~/.claude/projects/<old-encoded>` is renamed
  to `<new-encoded>` and the old dir no longer exists. [S1]
- **AC-9** - The encoded path the tool computes for a known input equals Claude
  Code's actual on-disk encoding for that input (verified against a real dir). [S1][S4]
- **AC-10** - After `apply`, every `cwd` field in the moved transcripts equals the
  new absolute path and zero equal the old path. [S1]
- **AC-11** - Abs-path rewrites occur only where the path is followed by a path
  separator or a terminating quote; no bare-substring replacement occurs. [S1]
- **AC-12** - After `apply`, occurrences of the project name as a package name,
  branch name, or prose are byte-identical to before (regression-checked against
  fixtures). [S1]
- **AC-13** - After `apply`, `~/.claude.json` contains the new project key(s) for
  every slash/case variant present pre-move, contains none of the old source keys,
  still parses, and retains the same total project-entry count. [S1]
- **AC-14** - After `apply`, `history.jsonl` entries that referenced the old path
  now reference the new path, and the file still parses line-by-line. [S1][S2]
- **AC-15** - `plan` (and `apply --dry-run`) prints a complete, human-readable diff
  of every file/dir/key it will change, and writes nothing. [S3]
- **AC-16** - Before the first write in `apply`, a backup snapshot exists that
  contains the original of every file the run will modify. [S1][S3]
- **AC-17** - Given a completed `apply`, When `rollback` runs against its report,
  Then all modified stores are restored to their pre-migration bytes and the folder
  is moved back. [S3][model-inference]
- **AC-18** - After `apply`, `verify` returns pass only if every postcondition holds
  (dir at new path, cwd rewritten, 0 old refs where required, JSON valid, line
  counts unchanged); otherwise it returns a listed failure. [S1][S3]
- **AC-19** - Running `apply` a second time on an already-migrated project makes no
  changes and exits success (no-op). [S3]
- **AC-20** - Given a store whose structure does not match a known adapter shape,
  When `plan` runs, Then the tool reports the unrecognized store and refuses to
  apply, leaving all state untouched. [S3]
- **AC-21** - Given a running CLI process or lockfile on an affected store, When
  `plan`/`apply` runs, Then the tool warns and requires confirmation (or `--force`)
  before editing. [S2][S3]
- **AC-22** - After `apply`, a machine-readable report (JSON) lists every action
  taken, counts changed, backup location, and verification result. [S3]
- **AC-23** - The CLI provides `plan`, `apply`, `verify`, and `rollback`
  subcommands. [S3]
- **AC-24** - The CLI returns 0 on success, a distinct non-zero code for
  refusal/guard trips, and another for verification failure. [S3][model-inference]
- **AC-26** - Across a full `plan`+`apply`+`verify`, zero LLM API calls and zero
  outbound network requests are made (assertable via a network-block test). [S3]

### Deferred Acceptance Criteria

These ACs are out of scope for v1.0.0 and are parked in the backlog.

- **AC-2** - Given src and dst on different volumes, When `apply` runs, Then the
  tool copies, verifies every file by checksum, and only then deletes the source;
  if any checksum mismatches, the source is left intact and the run fails.
  [S3][model-inference] Deferral: phase 10 backlog.
- **AC-25** - The GUI, given the same project, renders the same set of planned
  changes as the CLI `plan` output for that project (shared plan objects). [S3]
  Deferral: v2 (Tauri + React, phases 12+).
- **AC-27** - With Codex/Gemini adapters disabled (default), a Claude-only migration
  succeeds and touches no Codex/Gemini state; enabling them is an explicit flag.
  [S2][S3] Deferral: phase 11 backlog.

## Behavior / Examples

**Anchored rewrite (AC-10, AC-11, AC-12).** For the markdown move, the engine
replaced the exact literal `"cwd":"E:\\Projects\\Github Repos\\markdown-for-humans"`
(227 + 1240 times) and the prefix `E:\\Projects\\Github Repos\\markdown-for-humans\\`
(54 + 534 times) and the forward-slash prefix (27 times), while leaving
`markdown-for-humans@0.2.1` (8) and `markdown-for-humans_dev-*` (49) untouched;
line counts stayed 329 and 2285, all lines valid JSON. A conforming tool must
reproduce these exact counts against the fixture. [S1]

**Ambiguous attribution (AC-7).** The markdown history's stored cwd was the base
`...\markdown-for-humans` while the folder being moved was
`markdown-for-humans_jp-updates`, and a separate `markdown-for-humans` clone still
existed. The tool must surface this and take the operator's choice (fork / base /
both), exactly the decision made manually in the move log section 5. [S1]

**Dry-run first (AC-15).** `cpm plan --src "E:\Projects\Github Repos\markdown-for-humans_jp-updates" --dst "E:\Projects\prisant-labs\vs-code-markdown-max"` prints the
folder move, the `projects/` rename, the 2 transcript rewrites with counts, the
`claude.json` key change, and the `history.jsonl` edits, and writes nothing. [S3]

## Non-Functional Requirements

- **Determinism/offline:** no LLM, no network, reproducible output (AC-26). [S3]
- **Safety:** backup-before-write, verify-after-write, single-command rollback,
  hard-fail on unknown formats (AC-16, AC-17, AC-18, AC-20). [S3]
- **Performance:** same-volume moves are renames (sub-second for GBs); v1 reads whole
  files with a documented size bound and applies literal anchored byte splices
  (non-streaming); a single-project migration completes in seconds on the observed
  data sizes (transcripts ~9 MB, `history.jsonl` ~1 MB). [S1][S4]
- **Portability:** engine has no OS-specific logic outside a path-encoding module.
  GUI is deferred to v2. [S3]
- **Auditability:** every applied change appears in the report and matches the prior
  dry-run plan (AC-22). [S3]
- **Maintainability:** one adapter per fragile format, each with golden-file tests
  from real fixtures; format changes are localized (AC-20 backstops drift). [S2][S3]

## Sources and Evidence

### Primary committed sources

- **[D1]** `docs/DESIGN.md` - approved design (2026-07-10). Section 2 records the
  empirical encoding corrections that supersede the umbrella's `: \\ / space` claim;
  Section 3 documents the Rust stack decision; Section 4 defines the store adapter
  contract and rewrite algorithm; Section 5 lists correctness rules including the
  whole-buffer splice approach; Sections 6-9 cover the safety model, CLI surface,
  testing strategy, and phase plan.
- **[D2]** `docs/reference/claude-data-model.md` - data surface reference. The
  authoritative committed map of Claude Code's path-keyed and session-keyed stores.

### Discovery notes (local-only)

The `[S1]`-`[S4]` citations in the prose above reference the original discovery
documents at `_local/initial-discovery/`. These are gitignored by design (personal
notes, raw machine paths, backup references not appropriate for a public repo).
The factual findings they record are preserved and superseded by [D1]:

- **[S1]** `01-move-log.md` - executed manual migration with exact operations,
  counts, verification. Credibility A (primary, first-hand). Findings preserved in
  [D1] Sections 2 and 5.
- **[S2]** `02-hidden-files-and-external-state.md` - migration surface map across
  Claude/Codex/Gemini. Credibility A (direct inspection). Findings preserved in [D2].
- **[S3]** `03-strategy-brief.md` - architecture, scope tiers, plan/apply/verify
  safety model. Credibility B (reasoned analysis). Stack recommendation overridden by
  [D1] Section 3.
- **[S4]** Direct filesystem inspection of `~/.claude`, `~/.claude.json`, `~/.codex`,
  `~/.gemini` on this machine, 2026-07-09. Credibility A (observed), single-machine
  and single-point-in-time. Encoding rule and store discovery findings preserved in
  [D1] Section 2.

`[model-inference]` markers flag design choices (cross-volume checksum flow,
exit-code scheme, rollback semantics) that are reasoned, not yet sourced to a built
artifact; `requires-human-review: true` reflects their presence.

## Open Questions

Still open (require a decision before or during implementation):

1. **Format version stability** - how do Claude Code's on-disk formats change across
   releases, and should adapters pin observed versions or sniff shapes? Backstopped
   by AC-20, but the policy needs a decision. [S3]
2. **Attribution default** - in non-interactive mode with no `--attribute`, should
   an ambiguous history fail, or default to the fork? [S1]

Resolved by `docs/DESIGN.md` (no longer open):

- **Cross-platform encoding** - the encoding rule `[^A-Za-z0-9] -> -` is empirically
  confirmed on Windows against 45 dirs (DESIGN.md Section 2). The rule itself is
  settled; whether macOS/Linux encode identically is acknowledged but does not block
  v1 (Windows-first scope).
- **Language/runtime** - Rust chosen. Strategy brief's TypeScript recommendation is
  reversed by DESIGN.md Section 3 based on portfolio evidence (`repo-sync-tool`
  model, `adobe-cclib-liberator` cautionary evidence).
