# awt Command Reference

Binary: `awt`. All subcommands share a set of global flags and a common exit-code contract.

---

## Global flags

These flags are accepted by every subcommand.

| Flag | Default | Description |
|---|---|---|
| `--home <PATH>` | `USERPROFILE` / `HOME` env var | Home directory that holds `.claude/` and `.claude.json` |
| `--json` | false | Emit machine-readable JSON to stdout instead of human text |
| `--backup-root <PATH>` | System temp dir | Root directory where backup snapshots are written |
| `--force` | false | Allow overwriting a destination that already exists |
| `--recursive` | false | Also move nested projects found under `--src` |
| `--no-auto-rollback` | false | Disable the automatic rollback triggered on apply failure |
| `--on-collision <STRATEGY>` | `refuse` | How to handle a collision: `refuse` (default), `keep-dest`, `keep-src` |
| `--scope <SCOPE>` | `standard` | Which stores to rewrite: `minimal`, `standard` (default), `full` |

---

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | I/O error (catch-all for file-system failures) |
| 2 | Guard tripped - nothing was written. Conditions that produce exit 2: destination path already exists, source is a worktree, an ambiguous reference was found, or a flag combination leaves nothing to do |
| 3 | Verify failed - apply ran but at least one postcondition check failed; auto-rollback was attempted |
| 4 | Unrecognized format - a store contained a shape the tool does not know how to rewrite; no writes were made |

---

## Subcommands

### `awt doctor`

Report path-keyed state across all stores that references folders that no longer exist on disk.

**Purpose:** health check for your whole Claude installation. Run this first to see the scope of stale references before moving anything.

**Per-command flags:** none.

**Output (text):**
- `Stale references` - count and list of entries the tool can rewrite.
- `Report only (never rewritten)` - findings in regions no adapter owns (e.g., plugin vendor dirs, backups). Surfaced for visibility; never touched.
- `Unresolvable project dirs` - transcript directories whose sessions never recorded a `cwd`; nothing to resolve them against.

**Output (`--json`):** `{ "stale": [...], "report_only": [...], "unresolved": [...] }` where each entry carries `store`, `reference`, and `location` fields.

**Exit codes used:** 0 (clean or stale found), 1 (I/O error).

---

### `awt scan`

List every piece of Claude state that references a specific project path.

**Purpose:** scoped version of `awt doctor` for one project. Shows exactly what `apply` would need to rewrite.

**Per-command flags:**

| Flag | Required | Description |
|---|---|---|
| `--src <PATH>` | yes | Absolute path of the project to scan |

**Output (text):** `Hits for <src>: N` followed by one line per hit: `[<store>] <detail> -> <target>`.

**Output (`--json`):** `{ "src": "...", "hits": [{ "store", "detail", "target" }, ...] }`.

**Exit codes used:** 0, 1.

---

### `awt plan`

Dry-run: print every change that `apply` would make, without writing anything.

**Purpose:** review before committing. Shows which files would be modified, what path strings would be replaced, and how many occurrences.

**Per-command flags:**

| Flag | Required | Description |
|---|---|---|
| `--src <PATH>` | yes | Absolute path of the project in its current location |
| `--dst <PATH>` | yes | Absolute path of the intended new location |

Global flags that affect planning: `--recursive`, `--on-collision`, `--scope`, `--force`.

**Exit codes used:** 0, 1, 2 (guard), 4 (unrecognized format).

---

### `awt apply`

Move the project folder from `--src` to `--dst` and rewrite all Claude state to the new path.

**Sequence:** snapshot backup -> rewrite all state -> move the folder -> run verify -> (on failure) auto-rollback from snapshot.

**Per-command flags:**

| Flag | Required | Description |
|---|---|---|
| `--src <PATH>` | yes | Absolute path of the project in its current location |
| `--dst <PATH>` | yes | Absolute path of the intended new location |

Global flags that affect apply: `--recursive`, `--on-collision`, `--scope`, `--force`, `--no-auto-rollback`, `--backup-root`.

**Output:** `applied N changes; backup <path>` on success.

**Exit codes used:** 0, 1 (I/O), 2 (guard refused before any write), 3 (verify failed after write), 4 (unrecognized format before write).

---

### `awt verify`

Check that Claude state correctly references `--dst` after a completed move.

**Purpose:** independent post-hoc confirmation. `apply` runs this automatically; call it explicitly to re-check a move at any time.

**Per-command flags:**

| Flag | Required | Description |
|---|---|---|
| `--src <PATH>` | yes | Original (old) absolute path |
| `--dst <PATH>` | yes | New absolute path |

**Output:** one line per check: `[ok]` or `[FAIL]` with a detail string. Summary count of failures is printed to stderr and triggers exit 3.

**Exit codes used:** 0 (all checks pass), 1 (I/O error), 3 (one or more checks failed).

---

### `awt rollback`

Restore pre-move state from a backup manifest written by a previous `apply` run.

**Purpose:** escape hatch when auto-rollback did not fire or you need to undo a move after the fact. Takes the manifest path written by `apply`.

**Per-command flags:**

| Flag | Required | Description |
|---|---|---|
| `--report <PATH>` | yes | Path to the backup manifest file written by `apply` |

**Exit codes used:** 0, 1 (I/O error reading or restoring files), 2 (locked - manifest conflicts with current state).

---

### `awt list`

Enumerate every project Claude has state for, with session counts, sizes, oldest and newest transcript ages, and a health status.

**Purpose:** inventory your Claude installation. The `oldest_days` column makes the 30-day auto-delete cliff visible at a glance.

**Per-command flags:**

| Flag | Required | Description |
|---|---|---|
| `--html <PATH>` | no | Write a self-contained HTML inventory page to this path in addition to (not instead of) the table |

**Output (text):** a table with columns: cwd, sessions, bytes, oldest age (days), newest age (days), history lines, json keys, github paths, plugin dirs, health.

**Output (`--json`):** array of objects with the same fields plus `encoded_dir`, `todos`, `file_history`.

Health values: `ok`, `stale` (contains stale references), `unresolved` (transcripts with no resolvable `cwd`).

**Exit codes used:** 0, 1.

---

### `awt archive`

Copy transcripts and session artifacts to a durable folder before Claude's 30-day auto-delete removes them. Archives are incremental and deduplicated by content hash.

**Purpose:** preserve session history. Can be run manually, on a schedule, or wired into Claude Code as a `SessionEnd` hook so every session is archived automatically when it closes.

**Per-command flags:**

| Flag | Required | Description |
|---|---|---|
| `--archive-dir <PATH>` | yes (for archival and hook operations) | Destination directory for archived data |
| `--session <PATH>` | no | Archive a single transcript; path must be a `.jsonl` file. Requires `--archive-dir` |
| `--install-hook` | no | Register the `awt archive` `SessionEnd` hook in `~/.claude/settings.json`. Requires `--archive-dir` |
| `--uninstall-hook` | no | Remove the `awt archive` `SessionEnd` hook from `~/.claude/settings.json` |
| `--set-retention <DAYS>` | no | Set `cleanupPeriodDays` in `~/.claude/settings.json`. Minimum safe value is 1 (see issues #23710 and #62272 before setting to 0) |
| `--force-zero` | no | Required to set `cleanupPeriodDays` to 0. Read the known issues before using |
| `--hook-stdin` | no | Read `SessionEnd` hook JSON from stdin and archive the `transcript_path` field. Requires `--archive-dir`. Used by the installed hook; not normally called by hand |
| `--render` | no | Reserved for future HTML rendering of archived sessions. Has no effect in v1.0 |

**Default behavior (no session or hook flags):** archive all transcripts not yet archived.

**Output:** `archived: N copied, N skipped`.

**Cloud-sync warning:** if `--archive-dir` resolves to a path under a known sync root (`OneDrive`, `Dropbox`), a warning is printed to stderr. The archive proceeds; the warning is advisory.

**Exit codes used:** 0, 1, 2 (locked - missing required companion flag), 4 (hook stdin contained invalid JSON or missing `transcript_path`).

---

### `awt associate`

Re-link session history from one project path to another, and optionally export a portable copy of those sessions.

**Purpose:** recover history when a project has been moved or renamed outside of `awt apply`, or when the source folder no longer exists on disk.

**Per-command flags:**

| Flag | Required | Description |
|---|---|---|
| `--from <PATH>` | yes | Source project absolute path. May no longer exist on disk |
| `--to <PATH>` | yes | Destination project absolute path |
| `--export-subdir <DIR>` | no | Subdirectory name under `--to` for the exported archive. Default: `.claude-sessions` |
| `--no-reassociate` | no | Skip the re-association step; export only |
| `--no-export` | no | Skip the export step; re-associate only |

Passing both `--no-reassociate` and `--no-export` is an error (exit 2 - nothing to do).

Global flag that affects associate: `--on-collision`.

**Output:** `associate complete: N changes applied`.

**Exit codes used:** 0, 1, 2 (guard - both no-ops requested, or collision refused).

---

## Notes on flag interactions

- `--force` and `--on-collision` are independent. `--force` controls whether `apply` overwrites an existing destination folder on disk; `--on-collision` controls how the tool resolves collisions between Claude state entries when the same key appears for both `--src` and `--dst`.
- `--scope` values: `minimal` rewrites only the primary project key; `standard` (default) rewrites all path-keyed stores; `full` also rewrites content inside transcripts where the path appears in conversation text.
- `--no-auto-rollback` is intended for debugging. In normal use, leave auto-rollback enabled.
