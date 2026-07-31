# Changelog

All notable, user-facing changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project aims to follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

This is the user-facing changelog. For how the planning and design documents changed over time,
see `docs/CHANGELOG.md` (a doc-impact log, not a code changelog).

## [1.0.0] - unreleased

The complete Claude-state-aware project mover plus retention and repair tooling. Deterministic
and offline: zero LLM or network calls in the migration path, enforced by a dependency-guard
test.

The repair capability below was briefly drafted as a separate v1.1.0; the maintainer folded it
into v1.0.0 on 2026-07-30 (decision D9 in the release plan) because v1 is defined as everything
the CLI does and no v1.0.0 tag existed yet to freeze against.

The dating of this section is deferred to the tag. The tag is gated on the v1 safety closeout
(S-04), the maintainer spec sign-off (S-01), and a clean adversarial acceptance run - see
`docs/release-runbook.md` and the release plan.

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
- **Repair tooling** - `repair --drive-letter` recovers `history.jsonl` entries whose drive
  letter was replaced by a colon (`::\Projects\X` where `E:\Projects\X` belongs). Dry run by
  default; `--apply` writes. A value is repaired **only** when exactly one existing drive makes
  the corrected path resolve; zero candidates is reported as unrepairable, two or more is
  refused as ambiguous, and both declined sets are printed with their reason. Snapshots before
  writing, count-checks every replacement, undoable with `awt rollback`, writes only
  `history.jsonl`, idempotent. A `history.jsonl` that is not valid UTF-8 is refused outright
  (exit 4): that is a different corruption class than a drive-letter substitution, and repairing
  through a lossy decode would violate the never-lossy-rewrite invariant. There is deliberately
  no "repair everything" mode: each transformation is separately named and separately guarded.
- **`doctor` warnings channel** - shapes an adapter recognized and deliberately declined to act
  on. Distinct from stale references and from report-only findings. Included in `--json` output.
- **Fail-closed safety guards**, each with a plain-language situation/why/next-step message:
  destination-exists, git-worktree source, cross-volume move, live-lock, and ambiguous-history
  detection.
- **Machine-readable output everywhere** - `apply` writes a `report.json` on every run;
  `rollback` performs a verifiable revert, re-hashing every restored file against the
  pre-migration snapshot and writing a `rollback-report.json`; `plan --json` emits the full plan
  model (every change carries a `kind` discriminant, `rewrite_file` exposes its literal
  find/replace rules, and `totals` gives both plan entries and byte edits - the object the v2
  GUI is required to render under the AC-25 parity rule); `verify --json` emits the check list
  as data plus `failed` and `ok`.
- **Script-friendly exit codes** - 0 success, 1 io, 2 guard refusal (nothing written), 3
  verification failed, 4 unrecognized store format. Exit codes are unaffected by `--json`.

### Removed

- **`--on-collision keep-dest`/`keep-src`, `--recursive`, and `--scope minimal`/`full`**
  (AC-58, maintainer decision 2026-07-30). None was implemented behind its help text, and two
  silently weakened guards: the collision modes bypassed the destination-key check while
  changing nothing else, and `--recursive` suppressed the nested-project warning while moving
  nothing. `minimal` could never pass verification; `full` rewrote files verification did not
  cover. There is now exactly one rewrite behavior, collisions always refuse, and **a move
  whose source contains nested projects refuses outright** (exit 2, naming the children) -
  move the nested projects first. Each option may return later behind a real spec.

### Changed

- Renamed the binary and crates from `cpm` / `cpm-core` / `cpm-cli` to `awt` / `awt-core` /
  `awt-cli` (ADR-0001, branch `rename-cpm-to-awt`).
- A `githubRepoPaths` value that is not an array is no longer skipped **silently**. `doctor` and
  `plan` both name it and say it will not be examined or rewritten. Behavior is otherwise
  unchanged: it is still never rewritten, and it still does not fail the run. Silence was the
  previous behavior only by accident, and it is indistinguishable from a tool that failed to
  look.

### Fixed

- `awt list --html` help text said the HTML inventory was written *instead of* the table. It is
  written *in addition to* the table; the help text was wrong, not the behavior.
- `--src`, `--dst`, and `--report` had no help text in `--help` output. They are now described.
- The documented `rollback` invocation was wrong in the quickstart and troubleshooting guides:
  `rollback` takes `--report <manifest>`, not a positional path. Any command copied from those
  docs would have failed.
- **`apply` and `associate` could not complete for a project with a `githubRepoPaths` entry** in
  `~/.claude.json`. The rewrite was planned against the parsed (unescaped) path while the file
  stores it JSON-escaped, so the count check refused with `expected 1, live 0`. The failure was
  always safe - nothing was written and auto-rollback restored byte-identical state - but the
  operation could not succeed. Found by the first manual acceptance run (AR-01).
- **`apply` and `associate` could not complete when two `githubRepoPaths` slugs held the same
  path value.** Each occurrence planned its own edit expecting one match, while each edit counts
  across the whole file and saw two, so the count check refused with `expected 1, live 2`.
  Duplicate edits are now coalesced into one with the correct total (AR-04).
- **`associate` refused a project whose transcripts had expired**, even when `history.jsonl` and
  `claude.json` state remained, reporting "no Claude state found". Since transcripts expire after
  30 days and history never does, this refused exactly the long-dead projects the command exists
  to rescue. It now resolves the target across every store (AR-02).
- **`--json` was silently ignored by `plan` and `verify`.** Both accepted the flag, exited 0, and
  printed human text anyway. Both now emit JSON; exit codes are unchanged by the format, so a
  failed `verify --json` still exits 3 (AR-03).
- **`repair` read a damaged `history.jsonl` with a lossy UTF-8 decode**, violating the
  never-lossy-rewrite invariant; planning against lossily-decoded text computes counts for a
  file that does not exist on disk. It now refuses invalid UTF-8 with exit 4 (AC-53a).
- **Rollback could delete files it never backed up.** A directory rename snapshotted only
  top-level `*.jsonl`, then rollback recursively deleted the renamed directory - nested memory
  files, tool results, and any other sidecar were destroyed by the undo while it reported
  success. The snapshot is now recursive and rollback renames the whole directory back before
  restoring modified files, so the complete tree provably returns (AC-54).
- **A missing source folder was a silent success.** `apply` skipped the folder move, recorded
  it as applied, and exited 0. A folder move now requires the source to exist at plan time
  (exit 2), a source that vanishes before apply is a hard failure with auto-rollback, and
  verify asserts the destination is present and the source absent (AC-55).
- **A green verify could miss real damage.** Verification now takes the applied plan and
  asserts every planned `claude.json` edit actually landed (destination anchor present, source
  anchor gone, checked as raw bytes); a malformed `history.jsonl` line is a verification
  failure instead of being silently skipped; an unreadable transcripts directory is a failure
  instead of "zero stale"; and a verify that cannot run at all now rolls the apply back rather
  than leaving it applied-but-unproven (AC-57, AC-59).
- **A read failure could masquerade as emptiness.** Backup and merge walks are now strict - an
  unreadable subtree aborts the apply before any write instead of producing a snapshot that
  looks complete while missing files (AC-59).
- **Plugin state was missed when the path was spelled differently.** The state-dir hash is
  computed from the exact bytes Claude Code recorded, but detection hashed the caller's typed
  path - so `e:/projects/a` resolved every store except the plugin dir recorded under
  `E:\Projects\A`, and verify repeated the same blind spot and passed. Both now derive
  candidate hashes from every recorded spelling of the path (AC-60).
- **A malformed `settings.json` was replaced with a nearly-empty one.** Any read or parse
  failure loaded as an empty object that the next `--set-retention` or hook install/uninstall
  wrote over the user's file. Settings operations now refuse (exit 4) unless the file is
  genuinely absent, and write atomically via a temp file (AC-56).

### Known issues

- **A v1 safety closeout (S-04) is in progress and blocks the tag.** An external adversarial
  code audit (2026-07-30) found data-loss and false-success paths the happy-path acceptance run
  did not exercise. The three Critical findings (rollback tree loss, missing-source false
  success, settings fail-open) are **fixed** - see Fixed above. Still open: verification is not
  yet derived from the plan (a green verify does not check everything promised), several
  advertised flags are inert (`--on-collision keep-dest`/`keep-src`, `--recursive`, the
  `minimal`/`full` scopes), some I/O failures read as absence, plugin-state hashing uses the
  caller's path spelling, path confinement is lexical, and real transcripts remain published as
  fixtures. Until the closeout completes, prefer running against a copy of `~/.claude`.

### Notes

- Windows-first. Same-volume moves only in 1.0; cross-volume copy-verify-delete is a v1.x item.
- Re-running `apply` on an already-migrated project is idempotent by refusal (exit 2); see
  `docs/troubleshooting.md`.

## [0.1.0] - internal milestone (not tagged)

### Added

- Read-only `doctor` and `scan` (the Phase 4 read-only milestone). Kept available as a
  pre-release marker; never cut as a public tag.
