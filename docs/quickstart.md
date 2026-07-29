# Quickstart

A first run of the tool, safest path first. The CLI is `awt`. Everything here is deterministic
and offline - no network, no LLM.

## Install

From a clone of the repository:

```
cargo install --path crates/awt-cli
```

or build without installing:

```
cargo build --release
```

## Try it safely first

The tool edits state under `~/.claude`. Before running `apply` against your live Claude state,
practice against a COPY. Every command accepts `--home <dir>` to point at a different Claude home:

```
# make a copy of your Claude home, then point the tool at the copy
awt list --home "C:\path\to\claude-home-copy"
```

Do the first real move against a copy. Only run against your live `~/.claude` once you have seen
a clean `plan` and a successful `apply` + `verify` + `rollback` on the copy.

## 1. See what Claude knows about your projects

```
awt list
```

Lists every project Claude has state for, with session counts, sizes, and transcript ages, so the
30-day auto-delete cliff is visible. Add `--json` for machine-readable output.

## 2. Find stale references

```
awt doctor
```

Scans the whole install for path references that no longer resolve (projects that were moved or
renamed without their Claude state being updated).

## 3. Move a project (the core flow)

Always dry-run first. `plan` writes nothing:

```
awt plan --src "E:\Projects\old-name" --dst "E:\Projects\new-name"
```

Read the plan. It lists the folder move, the `projects/` directory rename, each transcript rewrite
with its edit count, the `~/.claude.json` key change, and the `history.jsonl` edits. If a guard
fires (destination exists, worktree source, cross-volume move, a live lock, or an ambiguous
history), the plan refuses and tells you what to do - see `docs/troubleshooting.md`.

When the plan looks right, apply it:

```
awt apply --src "E:\Projects\old-name" --dst "E:\Projects\new-name" --backup-root "E:\awt-backups"
```

`apply` snapshots every file it will touch (with sha256) into `--backup-root` first, then makes the
changes, then verifies the result from disk and auto-rolls-back if anything fails. It writes a
`report.json` into the backup directory (add `--json` to also print it to stdout).

Confirm independently:

```
awt verify --src "E:\Projects\old-name" --dst "E:\Projects\new-name"
```

Same-volume moves only in v1.0. Both paths must be on the same drive.

## 4. Undo a move

Point `rollback` at the manifest the apply wrote:

```
awt rollback --report "E:\awt-backups\awt-<run-id>\manifest.json"
```

`rollback` restores every file to its pre-migration bytes, moves the folder back, then proves the
revert: it re-hashes each restored file against the snapshot and writes a `rollback-report.json`.
It exits non-zero (3) if any file did not come back byte-identical.

## 5. Protect transcripts from the 30-day cliff

```
awt archive --archive-dir "E:\claude-archive"
```

Copies transcripts and session artifacts to a durable folder before Claude Code auto-deletes them.
Incremental and deduplicated by content hash, so re-running only copies what changed.

## Next

- Full per-command reference (all flags, all exit codes): `docs/reference/commands.md`.
- What an exit code means and how to recover: `docs/troubleshooting.md`.
