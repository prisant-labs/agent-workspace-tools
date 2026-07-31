---
id: S-02
title: "awt v1.1 features - inventory, retention, re-associate (F13-F15)"
type: spec
status: committed
created: 2026-07-11
updated: 2026-07-11
target-release: v1.0.0
linked-release: ../plan_v1.0.0.md
canonical-spec: ../../../../features/v1.1-inventory-retention-reassociate.md
ac-count: 17
ac-range: AC-28..AC-44
---

# S-02: awt v1.1 features - inventory, retention, re-associate (F13-F15)

> This file is a pointer spec. The full feature specification and all acceptance
> criteria (AC-28 through AC-44) live at the canonical path listed in frontmatter:
> [`../../../../features/v1.1-inventory-retention-reassociate.md`](../../../../features/v1.1-inventory-retention-reassociate.md).
> Do not duplicate AC text here; this file exists to make the release folder
> self-describing and to record the release assignment.

## Features

- **F13 `awt list` (inventory)** - report every project Claude has state for, with
  session counts, sizes, transcript ages, SESSION-keyed store links, PATH-keyed
  declarations, and a health flag (OK / STALE / UNRESOLVED).
- **F14 `awt archive` (retention)** - copy transcripts and SESSION-keyed artifacts to
  a user-defined archive folder before the 30-day cleanup deletes them; incremental,
  content-hash deduped, with auto-hook install and safety-net retention setting.
- **F15 `awt associate --from A --to B` (re-associate/export)** - keep A's Claude
  session history with B when A is being deprecated; runs the mover's state migration
  minus the folder move, and/or exports a portable copy; works even when A's folder
  is already gone.

The canonical spec at
[`docs/features/v1.1-inventory-retention-reassociate.md`](../../../../features/v1.1-inventory-retention-reassociate.md)
was committed and approved 2026-07-10. All AC text (AC-28 through AC-44), behavior
descriptions, archive layout, the retention hazard design note, and the
dependency/sequencing summary live there.

## Acceptance Criteria Index

| AC | Summary |
|----|---------|
| AC-28 | `awt list` enumerates every project dir, resolves each to stored `cwd`, flags no-cwd dirs UNRESOLVED rather than omitting them |
| AC-29 | Report includes session count, total transcript bytes, and oldest/newest transcript age in days |
| AC-30 | Report links SESSION-keyed stores (todos, file-history, session-env, tasks) to each project by sessionId |
| AC-31 | Report lists PATH-keyed declarations per project: claude.json key variants, githubRepoPaths entries, history.jsonl line count, plugin state dirs |
| AC-32 | A project whose stored `cwd` folder no longer exists is flagged STALE |
| AC-33 | `--json` and `--html <path>` produce the same underlying data as the terminal table (one model, three renderers); `--html` writes a self-contained file and mutates nothing else |
| AC-34 | `awt archive --archive-dir D` copies every project's transcripts to `D/projects/<enc>/` byte-for-byte; a re-run copies nothing unchanged (content-hash incremental) |
| AC-35 | Archive includes each session's SESSION-keyed artifacts (file-history, session-env, tasks, todos) under `D/session-artifacts/` |
| AC-36 | `--install-hook` adds a `SessionEnd` hook to `~/.claude/settings.json` that archives the ending session; `--uninstall-hook` removes exactly that hook and leaves other settings byte-identical |
| AC-37 | `--set-retention <n>` writes `cleanupPeriodDays: n`; passing `0` is refused unless `--force-zero`, and any value prints the #23710/#62272 caveat |
| AC-38 | Writes are atomic; if `--archive-dir` resolves under a cloud-sync root, a warning is printed; no network calls occur |
| AC-39 | A `manifest.json` records, per archived session, its SHA-256, source path, and sizes, sufficient to detect drift and to verify a later restore |
| AC-40 | `awt associate --from A --to B` locates A's sessions via stored `cwd` even when A's folder no longer exists on disk |
| AC-41 | With `--reassociate`, after apply, A's sessions resume under B: encoded dir, transcript `cwd`, claude.json key, githubRepoPaths, history.jsonl, and plugin state dir all reference B; verify passes; backed up and rollback-able |
| AC-42 | With `--export`, A's transcripts and SESSION-keyed artifacts are copied into `B/<export-subdir>/` in the F14 archive format with a manifest |
| AC-43 | `--no-reassociate` performs export only (Claude's live records untouched); `--no-export` performs re-association only (no copy placed in B); at least one mode must be active or the command errors |
| AC-44 | If B already has a claude.json key (collision), the mover's collision policy applies - always refuse (amended 2026-07-30 under AC-58: the never-implemented `keep-dest`/`keep-src` modes were removed) |
