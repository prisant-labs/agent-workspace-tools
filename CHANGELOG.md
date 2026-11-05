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

The dating of this section is deferred to the tag. The v1 safety closeout (S-04) completed
2026-07-31, including a passing adversarial acceptance run. That run was repeated on 2026-11-04,
because clap and the rollback engine changed after it. The AR-09 (archive scope defect) fix is
merged, and the archive steps passed on the merged build on 2026-11-05. AR-10 (a project folder
with no readable transcript could not be moved) is fixed, and its acceptance run passed. The tag
now waits on two maintainer steps: the spec sign-off (S-01, the mover CLI spec) and the
fixture-history decision (D10). See `docs/internal/maintainer-todo.md` section 1.

### Added

- **Mover CLI** - `plan`, `apply`, `verify`, `rollback`: relocate a project folder and migrate
  all Claude Code state keyed to its old absolute path (transcripts, `~/.claude.json`,
  `history.jsonl`, plugin state). Every run takes a sha256 backup snapshot before writing,
  count-checks each boundary-anchored rewrite, verifies each postcondition from disk after
  apply, and automatically rolls back on any failure.
- **Read-only tools** - `doctor` (scan the whole install for stale path references) and `scan`
  (show all Claude state for one project).
- **Retention tooling** - `list` (inventory with session counts, sizes, and 30-day-cliff ages),
  `archive` (content-hash incremental copy, before the auto-delete removes it, of transcripts,
  subagent transcripts, tool results, and each session's file history, session environment, tasks
  and todos; a file a running session holds open is reported as failed and the sweep goes on),
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
  destination-exists, git-worktree source, cross-volume move, IDE lock file, and
  ambiguous-history detection. The lock-file guard cannot see a session in a plain terminal;
  see Known issues.
- **Machine-readable output everywhere** - `apply` writes a `report.json` on every run;
  `rollback` performs a verifiable revert, re-hashing every restored file against the
  pre-migration snapshot and writing a `rollback-report.json`; `plan --json` emits the full plan
  model (every change carries a `kind` discriminant, `rewrite_file` exposes its literal
  find/replace rules, and `totals` gives both plan entries and byte edits - the object the v2
  GUI is required to render under the AC-25 parity rule); `verify --json` emits the check list
  as data plus `failed` and `ok`.
- **Script-friendly exit codes** - 0 success, 1 io, 2 guard refusal (nothing written), 3
  verification failed, 4 unrecognized store format. Exit codes are unaffected by `--json`.

### Security

- **Hook path confinement is now semantic, not lexical** (AC-61). The `SessionEnd` hook's
  `transcript_path` used to be checked by lowercase prefix comparison, which
  `<projects>/../../outside` defeats; it is now rejected for `..`/`.` components and
  canonicalized against the real filesystem before containment is required. A transcript
  that does not exist is refused rather than assumed safe.
- **Junctions and symlinks are refused in mutated trees and skipped by archive** (AC-61).
  A link inside a project-state directory could pull outside content into a snapshot or an
  archive, or let a rollback delete beyond the tree. Moves refuse at plan time (exit 2, with
  a TOCTOU re-check at apply); archive skips the link and protects the rest. Detected via
  real NTFS reparse attributes, tested with a real `mklink /J` junction.
- **The published real-transcript fixtures are gone from the working tree** (AC-62). The two
  real session transcripts (18.1 MB, before/after pairs) are replaced by deterministic
  synthetic fixtures generated by `scripts/generate-reference-fixtures.py`, preserving
  exactly the golden-locked counts (2,082 anchored rewrites, 10/55 preserved mentions,
  329/2,285 lines) and nothing else. Git history still contains the originals pending the
  maintainer's D10 decision.

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
- **Forcing past a git worktree source now warns.** `--force` lifts two guards; the live-lock
  one already recorded the override in the plan, the report, and `plan --json`, while the
  worktree one recorded nothing. That made the more destructive of the two the silent one:
  moving a linked worktree breaks the `.git`-file link to the parent repository and the
  parent's `worktrees/<name>/gitdir` link back, and `awt` repairs Claude Code state, not git's
  bookkeeping. The override now emits a warning naming the path and pointing at
  `git worktree repair` (AC-4). The refusal without `--force` is unchanged.

### Fixed

- **A project folder with no readable transcript now moves with its project.** A folder holding
  only `memory/`, only session folders, or a transcript with no `cwd` used to make `apply` and
  `associate` exit 3 and roll back. The folder now moves when its encoded name matches a path awt
  knows is real, and awt warns that the name was the only link. If something ties the folder to
  another path, it stays put and awt says so. If two look-alike paths both claim it, awt refuses
  with exit 2.
- **`apply` and `associate` now show the plan's warnings.** They appeared only in `plan` before. They
  go to stderr, and into a `warnings` field of the report when there are any.
- **A path's spelling no longer changes the result.** A trailing separator, a trailing `\.`, forward
  slashes or a `\\?\` prefix on `--src` or `--dst` could pass verification with state left behind.
  A typed path that differs from the stored one in case now rewrites the stored spelling too.
- **`plan` now refuses what `apply` could not do.** That covers a transcript with a broken line
  (exit 4), a file on both sides of a folder merge, a destination folder that belongs to a
  look-alike, and a nested project that has only a state folder (each exit 2).
- **A rename that keeps the encoded name now works,** such as `foo_bar` to `foo-bar`. It used to try
  to merge the folder into itself and exit 3.
- **Each run now has its own backup folder.** Two runs in the same second used to share one, and
  one could overwrite the backup the other's rollback needs. Backup folders are now named
  `awt-<seconds>-<process id>`.
- **A state folder that is itself a junction now refuses with exit 2.** It used to move files out
  of the junction's target.
- **The `SessionEnd` archive hook now asks for a 60-second timeout.** Claude Code gives that
  event 1.5 seconds by default, which a large session copy can exceed. If you installed the hook
  with an earlier build, run `awt archive --install-hook --archive-dir <path>` again. The new
  entry replaces the old one and carries the timeout.
- **A full `awt archive` sweep now copies `file-history/`, `session-env/`, `tasks/` and `todos/`
  whole.** Files whose session had no transcript or session folder left were dropped before.
  `--session`, the hook and `associate --export` still copy per session id.
- **`archive` refuses an archive folder that holds the Claude data (exit 2).** A folder equal to
  or above `~/.claude`, `projects`, `file-history`, `session-env`, `tasks` or `todos`, including
  a drive root, made the run skip its own sources, copy nothing and exit 0. `--install-hook`
  refuses it too, so the hook never records a useless path.
- **A failure to write into the archive folder now exits 1** on a full sweep and on `--session`,
  after the result prints. Read failures and hook mode still exit 0.
- **A relative or `..` spelling of `--archive-dir` no longer defeats the self-copy guard.** The
  path is made absolute before use, and the hook records the absolute path.
- **A transcript named `...jsonl` could copy the whole Claude home into the archive**, including
  `.credentials.json`. Session ids that are empty, `.` or `..` are no longer used as paths.
- **`associate --export` plans before it exports.** A plan refusal no longer leaves an export
  in `--to`. An export-only run that no readable transcript resolves now refuses with exit 2
  instead of printing an export path over an empty folder.
- **Rollback left a renamed plugin-state folder behind, and the same move then could not be
  retried.** Original files came back byte-identical, so the post-rollback proof passed, but
  rollback worked out each rename's destination from the encoded project name. A plugin-state
  folder never contains that name, and a transcript folder stored in another case does not
  match it, so rollback skipped both. A retry then passed `plan` and failed at `apply` with OS
  error 145. The manifest now records each rename's destination, and rollback uses it.
  Manifests written before this fix still restore. The post-rollback proof now fails if a
  renamed folder is still there (F-1).
- **`plan` passed when a plugin-state rename could not succeed.** A folder could already occupy
  the destination, or two folders holding state for two spellings of the source path could share
  one. `plan` now refuses with exit 2 and names the folder, and `apply` re-checks before its first
  change. A leftover from a rollback by an older build is the known way to reach this (F-1).
- **A failed `apply` hid a refused automatic rollback.** The message said only where the backup
  was, which read as if the failure had been cleaned up. It now says that the rollback did not
  complete, and why, so the half-applied move is visible (F-1).
- **Rollback could refuse halfway when a folder existed at both its old and new name.** The
  project folder had already moved back by then. The check now runs in the preflight, so the
  refusal changes nothing (F-1).
- **`awt list --html <path>` silently replaced an existing file**, from a command that otherwise
  only reads. It now refuses with exit 2 and leaves the file alone (F-2). To refresh an
  inventory, delete the old page first or choose a new name.
- **Rollback now validates the entire manifest and verifies every backup BEFORE touching the
  filesystem.** Previously the integrity checks ran inline during the restore loop, after the
  project folder and state directories had already been renamed back: a corrupted backup or
  malformed entry detected at position N stranded a half-restored filesystem, and a content
  entry missing its `sha256` restored unverified bytes silently. A fail-closed preflight now
  refuses (exit 4 for shape problems, exit 3 for a hash mismatch) with the filesystem
  untouched; unknown marker entries also refuse instead of being guessed at. Found by a Codex
  adversarial review on 2026-07-31.
- **Four successful mutating modes printed prose under `--json`:** `archive --install-hook`,
  `archive --uninstall-hook`, `archive --set-retention`, and `associate`. Each now emits one
  machine-readable JSON object on stdout (`{"hook_installed":...}`, `{"hook_removed":...}`,
  `{"cleanup_period_days":...}`, `{"applied":...,"backup_dir":...,"export":...}`); warnings
  stay on stderr. Same review.
- **`rollback --report` panicked (exit 101) when handed the `report.json` that `apply` itself
  prints.** The flag is named `--report` and apply's last output line points at `report.json`,
  so the natural invocation crashed with nothing restored. Rollback now accepts either file: a
  backup `manifest.json` passes through, an apply `report.json` dereferences to its sibling
  manifest, and any other JSON shape refuses with exit 4 instead of panicking. Found by the
  2026-07-31 adversarial acceptance run (AR-05).
- **`archive --json` printed the text summary line.** The `--json` sweep that fixed AR-03 on
  `plan`/`verify` missed `archive`; all three archive paths (full sweep, `--session`,
  `--hook-stdin`) now emit `{"copied": n, "skipped": n}` (AR-07).
- **`associate` and `archive` on a path with no recorded state exited 4 instead of 2.** Exit 4
  is reserved for store bytes the tool cannot parse; "nothing recorded for this path" is a
  guard refusal, exit 2. Message unchanged, class corrected (AR-08).
- `--force` help text still advertised "allow overwriting a destination that already exists",
  a bypass removed by the AC-58 surface reduction (collisions refuse unconditionally). It now
  names the two guards force actually lifts: worktree source and live IDE lock (AR-06).
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

- **The live-session guard does not see terminal sessions (FF-1).** The guard only finds IDE
  lock files under `~/.claude/ide/`, and a session in a plain terminal writes none. If such a
  session is running in the source folder, `apply` and `associate` do not refuse, and they
  report success. The session then keeps writing under the old path, so the migration goes
  stale within minutes. No data is lost. Exit every session running in the source folder first,
  and do not run `awt` from inside one. The guard fix is planned for v1.0.1.
- **`associate` cannot finish into a destination that is already live (FF-2).** It refuses with
  exit 2 and writes nothing, so a half-migrated project has no supported way to finish. The
  planned fix prints the manual steps, and exports same-session transcripts as an archive.

### Notes

- Windows-first. Same-volume moves only in 1.0; cross-volume copy-verify-delete is a v1.x item.
- Re-running `apply` on an already-migrated project is idempotent by refusal (exit 2); see
  `docs/troubleshooting.md`.

## [0.1.0] - internal milestone (not tagged)

### Added

- Read-only `doctor` and `scan` (the Phase 4 read-only milestone). Kept available as a
  pre-release marker; never cut as a public tag.
