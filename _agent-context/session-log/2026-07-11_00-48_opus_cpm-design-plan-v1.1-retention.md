---
type: session-log
date: 2026-07-11
time: "00:48"
llm: claude-opus-4-8
mode: deep
title: "CPM validated design, v1 TDD plan, v1.1 features, and live retention fix"
project: claude-project-mover
status: design-and-plan-complete-ready-to-build
branch: main
head: 655d6b5
related:
  - docs/DESIGN.md
  - docs/superpowers/plans/2026-07-10-claude-project-mover.md
  - docs/features/v1.1-inventory-retention-reassociate.md
  - docs/reference/claude-data-model.md
  - docs/reference/existing-solutions.md
  - docs/CHANGELOG.md
  - _local/initial-discovery/01-05 (gitignored, partially superseded)
---

# Session Log: CPM design, plan, v1.1 features, retention fix

## What this session was

Took the `claude-project-mover` (CPM) project from raw discovery notes to a
validated design, a full TDD implementation plan, three additional planned
features with two learning docs, and one live machine change (stopping Claude's
30-day transcript auto-delete). All planning artifacts are committed to `main`.
No product code was written yet - this was brainstorming -> writing-plans -> scope
expansion, plus a stopgap config fix.

The project: a deterministic, offline Windows-native CLI that relocates a project
folder and migrates all Claude Code state keyed to its old absolute path
(transcripts, `~/.claude.json`, `history.jsonl`, plugin state), with
dry-run/backup/verify/rollback, zero LLM/network use.

## Decisions made (with rationale)

1. **Tech stack: Rust core + Tauri deferred, reversing the discovery brief's
   TypeScript pick.** Evidence: `repo-sync-tool` (Rust+Tauri, a `reposync-core`
   crate forbidden from depending on Tauri via a one-line CI gate) is the model to
   copy; `adobe-cclib-liberator`'s own docs say it is Python only because a
   pre-existing engine existed - a constraint CPM does not have. The brief's
   anti-Rust argument ("byte-preserving rewrites are ceremony in a serializer")
   dissolves once the rule is "never serialize; parse only to validate, write by
   literal splice." Toolchain present: cargo 1.96.0.

2. **v1 scope: full move loop, with `doctor` (read-only) as milestone one.** User's
   synthesis, accepted. `doctor` exercises the risky read layer (reverse index,
   shape validation, staleness detection) against real data before any byte is
   written. `doctor` != dry-run: `doctor` takes no destination and inventories
   machine-wide staleness; `plan` takes `--src --dst` and produces a change list.

3. **Two policy decisions locked:** claude.json destination-key collision -> refuse
   by default, `--on-collision=keep-dest|keep-src` to override. Nested project keys
   under source -> detect + report, migrate only with `--recursive`.

4. **Three v1.1 features added** (F13-F15), Claude-only, same adapter boundary:
   - F13 `cpm list`: project inventory (terminal / --json / --html).
   - F14 `cpm archive`: retention/archival to a user-defined folder before the
     30-day delete; content-hash incremental dedup; `--install-hook` (SessionEnd);
     `--set-retention` (large finite value, refuses 0). Git-agnostic; user decides
     if the archive folder is a repo / gitignored / cloud-synced.
   - F15 `cpm associate --from --to`: re-associate AND/OR export a project's
     sessions to another folder; both modes independently toggleable; works when
     the source folder is already deleted.

5. **Live change: stopped the 30-day auto-delete.** Set `cleanupPeriodDays: 36500`
   in `~/.claude/settings.json` (backed up first). Deliberately NOT `0` (see
   findings). This is a stopgap, not a backup - F14 remains the real durability.

## Empirical findings (load-bearing; these drove the design)

Verified against the live machine (`C:\Users\jpris\.claude`), not from the docs:

1. **Encoding rule was wrong in the discovery docs.** It replaces every non-`[A-Za-z0-9]`
   char with `-`, INCLUDING dots. Proof: `...\.claude\...` -> `-claude-`,
   `...\v2.26.0` -> `v2-26-0`. A tool on the documented rule (`: \ / space` only)
   silently orphans transcripts.
2. **The encoding is lossy** (`a-b`, `a.b`, `a\b`, `a_b` all collapse) so discovery
   must read the stored `cwd` from inside transcripts, not compute `encode(src)`.
3. **Dir name derives from the launch `cwd` string**, so drive-letter case can differ
   from a normalized path (`d--Cloud-Work-PP` holds cwd `D:\Cloud-Work-PP`). Match
   case-insensitively.
4. **Two path-keyed stores the docs missed:** `~/.claude.json -> githubRepoPaths`
   (array values, 6 stale today) and plugin state dirs
   `plugins/data/*/state/<base>-<sha256(abs)[:16]>` (verified:
   `sha256("E:\Projects\Github Repos\markdown-for-humans")[:16] == e854827f52137cd9`).
5. **Transcripts are auto-deleting at 30 days RIGHT NOW.** Of 2,647 transcripts,
   oldest 30d, median 28d, nothing older. No cleanupPeriodDays was set -> 30-day
   default active. This made F14 time-sensitive and prompted the live fix.
6. **`cleanupPeriodDays: 0` is unsafe.** Docs say 0 disables cleanup; Anthropic issue
   #23710 says 0 disables transcript WRITING; #62272 says cleanup is mtime-based.
   Use a large finite value + content-hash dedup.
7. **`history.jsonl` never expires** (prompts persist), transcripts do - opposite
   lifetimes.
8. **A real gone-folder project exists** (`relational-connection/fixed`) - the F15
   fixture. Session-keyed stores link by sessionId (todos/file-history/session-env/
   tasks), the join backing F13/F14/F15.
9. **Prior art is Unix-first** (clamp, claudepath, skydiver all need WSL/Git Bash);
   Windows-native is CPM's wedge; no tool combines inventory + retention + move.

## Artifacts produced

Committed to `main` (HEAD 655d6b5):

- `docs/DESIGN.md` - validated design (mover + v1.1 features section, phase plan 1-15).
- `docs/superpowers/plans/2026-07-10-claude-project-mover.md` - TDD plan, Phases 1-9
  (mover) + 13-15 (v1.1), full Rust code per task, golden reference-move test.
- `docs/features/v1.1-inventory-retention-reassociate.md` - F13-F15 spec, AC-28..44.
- `docs/reference/claude-data-model.md` - learning doc: full store taxonomy,
  folder=project, retention rules.
- `docs/reference/existing-solutions.md` - learning doc: prior-art survey.
- `docs/CHANGELOG.md` - doc-impact log.

Live machine (not in repo):
- `~/.claude/settings.json` - added `cleanupPeriodDays: 36500`; backups at
  `settings.json.bak-20260710-193613` and a second identical timestamped `.bak`.
- Memory: `~/.claude/projects/E--Projects-prisant-labs-claude-project-mover/memory/`
  (`cleanup-retention-disabled.md` + `MEMORY.md`).

## Current state

- Branch `main`, HEAD `655d6b5`, working tree clean except untracked `.memsearch/`.
- No `crates/`, no code yet. `README.md` is a stub. `_local/initial-discovery/01-05`
  are gitignored and partially superseded (corrections live in `docs/`).
- The plan is ready to execute task-by-task (subagent-driven or inline).

## Open questions / risks (unresolved, non-blocking)

- **Cross-platform encoding** (macOS/Linux) unverified; v1 is Windows-only.
- **Codex/Gemini trust storage** location unconfirmed; those adapters deferred.
- **F14 SESSION-keyed copy loop and F15 from-scoped export filter** are specified
  in-plan as notes (Task 14.1, 15.1) and must be completed before those features
  ship - each has a required test.
- **Two mechanical cross-task changes** flagged in the plan self-review: add
  `mtime_secs` to `FileSystem` (Phase 13), add `move_folder: bool` to `PlanOpts`
  (Phase 15).
- **`cleanupPeriodDays: 0` behavior** not verified on the installed version (used
  36500 to sidestep).

## Suggested next actions (priority order)

1. **Start the build** at Phase 1 (workspace scaffold + FileSystem trait + fixtures
   + CI dep-gate). Subagent-driven per task is the recommended execution mode.
2. Or **one-time archive copy** of `~/.claude/projects` to a durable folder as a
   manual F14 stand-in (offered, not yet done - no destination chosen).
3. Confirm whether v1.1 features build interleaved with the mover or strictly after
   v1.0 (plan supports either; F13/F14 need only phases 1-4 + 7).

---

## Verbose continuation prompt (copy-paste ready)

```
Resume the claude-project-mover (CPM) project. Full context is in the repo at
E:\Projects\prisant-labs\claude-project-mover. Read these first, in order:
docs/DESIGN.md (the validated design), docs/superpowers/plans/2026-07-10-claude-project-mover.md
(the TDD implementation plan), docs/features/v1.1-inventory-retention-reassociate.md
(features F13-F15), and docs/reference/claude-data-model.md (how Claude stores project
state). The doc-impact history is in docs/CHANGELOG.md. Prior-art is in
docs/reference/existing-solutions.md.

State of play: all planning is DONE and committed to main (HEAD 655d6b5). No product
code exists yet. The project is a deterministic, offline, Windows-native Rust CLI
(`cpm`) that relocates a project folder and migrates all Claude Code state keyed to
its old absolute path, with dry-run/backup/verify/rollback and zero LLM/network use.
Architecture: a Tauri-free `cpm-core` crate + a `cpm-cli` crate; GUI deferred. The
design mirrors E:\Projects\product-on-purpose\repo-sync-tool (Rust + Tauri 2 +
tauri-specta, core crate forbidden from depending on Tauri via a one-line CI gate).

Hard rules the whole design depends on (do not violate):
- Path-dir encoding = replace every non-[A-Za-z0-9] char with '-' (INCLUDING dots).
  It is lossy and forward-only; find existing dirs via a reverse index that reads the
  stored `cwd` from inside transcripts, never by computing encode(src).
- Parse store files only to VALIDATE shape (serde_json::from_str), discard the result,
  and write by literal boundary-anchored, count-checked byte splice. NEVER re-serialize
  a store file (it would reformat and defeat verification).
- Match paths case-insensitively (drive-letter case can differ from a normalized path).
- Never rewrite another project's transcripts even if they mention the old path.
- The six v1 stores: claude.projects (dir + transcripts), claude.json (projects{} keys
  + githubRepoPaths{} array values), claude.history (project field), plugin.state
  (sha256(abs)[:16] dir suffix), plus a report-only sweep.unknown. history.jsonl and
  githubRepoPaths and plugin state dirs are easy to forget - they are covered.

Immediate next action: begin executing the plan at Phase 1 Task 1.1 (Rust workspace
scaffold + CI dependency-hygiene gate), using the superpowers:subagent-driven-development
skill (fresh subagent per task, review between tasks) unless I say otherwise. The plan's
tasks are bite-sized TDD (write failing test -> run -> implement -> run -> commit) with
complete Rust code in each step. The FIRST milestone is Phase 4: `cpm doctor` running
read-only against the real machine and reporting exactly the residue found by hand
(6 stale githubRepoPaths, ~11 stale history.jsonl values, the orphaned plugin dir
markdown-for-humans-e854827f52137cd9). That is the honesty checkpoint that proves the
read layer before any write code is trusted.

Two mechanical cross-task changes the plan self-review flagged (apply when you reach
them): add `mtime_secs` to the FileSystem trait (Phase 13), and add `move_folder: bool`
to PlanOpts (Phase 15). Two in-plan notes to finish before shipping F14/F15: the
SESSION-keyed copy loop (Task 14.1) and the from-scoped export filter (Task 15.1).

Golden test to preserve: the reference move is old=E:\Projects\Github Repos\markdown-for-humans,
new=E:\Projects\prisant-labs\vs-code-markdown-max; fixtures come from
E:\tmp\claude-move-backup-20260709-090053; the anchored rewrite must reproduce exactly
cwd 227+1240, backslash 54+534, forward 0+27, while leaving markdown-for-humans@ (8) and
markdown-for-humans_dev- (49) byte-identical.

Already done to the live machine (do NOT redo): set cleanupPeriodDays: 36500 in
~/.claude/settings.json to stop the active 30-day transcript auto-delete (backed up).
An optional one-time archive copy of ~/.claude/projects to a durable folder was offered
but not done (no destination chosen) - ask me if I still want it.

Environment notes: Windows 11, PowerShell primary; the Bash tool has been flaky this
session (commands intermittently drop to background / time out) - prefer PowerShell,
Glob, and Grep tools over Bash for file ops. cargo 1.96.0 and pnpm 10.33.4 are installed.

Start by confirming you have read DESIGN.md and the plan, then propose the first
subagent dispatch for Phase 1 Task 1.1 (or tell me if you'd execute inline instead).
```
