# Changelog

All notable, user-facing changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project aims to follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

This is the user-facing changelog. For how the planning and design documents changed over time,
see `docs/CHANGELOG.md` (a doc-impact log, not a code changelog).

## [1.0.0] - unreleased

### Changed

- Renamed the binary and crates from `cpm` / `cpm-core` / `cpm-cli` to `awt` / `awt-core` / `awt-cli` (ADR-0001, branch `rename-cpm-to-awt`).

The dating of this section is deferred to the tag; the v1.0.0 tag is gated on a maintainer
acceptance run and spec sign-off (see `docs/release-runbook.md`).

The full Claude-state-aware project mover plus retention tooling. Deterministic and offline:
zero LLM or network calls in the migration path, enforced by a dependency-guard test.

### Added
- **Mover CLI** - `plan`, `apply`, `verify`, `rollback`: relocate a project folder and migrate
  all Claude Code state keyed to its old absolute path (transcripts, `~/.claude.json`,
  `history.jsonl`, plugin state). Every run takes a sha256 backup snapshot before writing,
  count-checks each boundary-anchored rewrite, verifies each postcondition from disk after
  apply, and automatically rolls back on any failure.
- **Read-only tools** - `doctor` (scan the whole install for stale path references) and `scan`
  (show all Claude state for one project).
- **Retention tooling** - `list` (inventory with session counts, sizes, and 30-day-cliff ages),
  `archive` (content-hash incremental copy of transcripts before the auto-delete removes them),
  and `associate` (re-link a deprecated project's history to a replacement path, reversibly,
  even when the old folder is gone).
- **Fail-closed safety guards**, each with a plain-language situation/why/next-step message:
  destination-exists, git-worktree source, cross-volume move, live-lock, and ambiguous-history
  detection.
- **Machine-readable records** - `apply` writes a `report.json` on every run (and to stdout with
  `--json`); `rollback` performs a verifiable revert, re-hashing every restored file against the
  pre-migration snapshot and writing a `rollback-report.json`.
- **Script-friendly exit codes** - 0 success, 1 io, 2 guard refusal (nothing written), 3
  verification failed, 4 unrecognized store format.

### Notes
- Windows-first. Same-volume moves only in 1.0; cross-volume copy-verify-delete is a v1.x item.
- Re-running `apply` on an already-migrated project is idempotent by refusal (exit 2); see
  `docs/troubleshooting.md`.

## [0.1.0] - internal milestone (not tagged)

### Added
- Read-only `doctor` and `scan` (the Phase 4 read-only milestone). Kept available as a
  pre-release marker; never cut as a public tag.
