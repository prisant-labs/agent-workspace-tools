# awt Command Reference

Binary: `awt`. All subcommands share a set of global flags and a common exit-code contract.

---

## Global flags

These flags are accepted by every subcommand.

| Flag | Default | Description |
|---|---|---|
| `--home <PATH>` | `USERPROFILE` / `HOME` env var | Home directory that holds `.claude/` and `.claude.json` |
| `--json` | false | Emit machine-readable JSON to stdout instead of human text. Supported by every subcommand. For `plan` this is the plan model the v2 GUI renders (ROADMAP AC-25 parity); each change carries a `kind` discriminant and a `totals` object gives `changes` and `edits`. Exit codes are unaffected by the format: a failed `verify --json` still exits 3. |
| `--backup-root <PATH>` | System temp dir | Root directory where backup snapshots are written |
| `--force` | false | Proceed despite a git-worktree source or a live IDE lock. Both overrides record a warning that reaches the rendered plan, `plan --json`, and the run's report. **Collisions always refuse**: the destination-exists guard is unconditional since AC-58 removed the `keep-dest`/`keep-src` bypass modes, so `--force` will not overwrite a destination (see [Notes on flags](#notes-on-flags)) |
| `--no-auto-rollback` | false | Disable the automatic rollback triggered on apply failure |

---

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | I/O error (catch-all for file-system failures), including a failed write into the archive folder on a full `awt archive` sweep or a `--session` run |
| 2 | Guard tripped - nothing was written. Conditions that produce exit 2: destination path already exists, both project folders of a merge hold the same file, the destination's project folder belongs to a look-alike path, a plugin-state folder already occupies its rename destination, the `list --html` target already exists, the source folder is missing, nested projects live under the source, source is a worktree, an IDE lock file exists under `~/.claude/ide/`, an ambiguous reference was found, an archive folder contains the Claude data, an export-only `associate` has no readable transcript that resolves the project, or a flag combination leaves nothing to do |
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

- `Warnings` - shapes an adapter recognized and deliberately declined to act on, printed only when there are any. Distinct from stale (a dead reference) and from report-only (a region no adapter owns): these sit inside an owned region and were skipped on purpose. Currently: a `githubRepoPaths` value that is not an array, which will never be examined or rewritten.

**Output (`--json`):** `{ "stale": [...], "report_only": [...], "unresolved": [...], "warnings": [...] }` where each stale/report-only entry carries `store`, `reference`, and `location` fields.

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

Global flag that affects planning: `--force`.

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

Global flags that affect apply: `--force`, `--no-auto-rollback`, `--backup-root`.

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

**Exit codes used:** 0, 1 (I/O error reading or restoring files), 3 (a backup failed its hash, a folder exists at both its old and new name, or a post-rollback check failed), 4 (the file is not a manifest or a report). A backup failure or a folder at both names refuses before anything changes.

---

### `awt list`

Enumerate every project Claude has state for, with session counts, sizes, oldest and newest transcript ages, and a health status.

**Purpose:** inventory your Claude installation. The `oldest_days` column makes the 30-day auto-delete cliff visible at a glance.

**Per-command flags:**

| Flag | Required | Description |
|---|---|---|
| `--html <PATH>` | no | Write a self-contained HTML inventory page to this path in addition to (not instead of) the table. Refuses with exit 2 if the path already exists, so it never replaces a file |

**Output (text):** a table with columns: cwd, sessions, bytes, oldest age (days), newest age (days), history lines, json keys, github paths, plugin dirs, health.

**Output (`--json`):** array of objects with the same fields plus `encoded_dir`, `todos`, `file_history`.

Health values: `ok`, `stale` (contains stale references), `unresolved` (transcripts with no resolvable `cwd`).

**Exit codes used:** 0, 1, 2 (the `--html` path already exists).

---

### `awt archive`

Copy session history to a durable folder before Claude's 30-day auto-delete removes it. Archives are incremental and deduplicated by content hash.

**What it copies.** For each project folder under `~/.claude/projects/<enc>/`, it copies these files byte for byte:

- Each top-level `*.jsonl` transcript, to `<archive>/projects/<enc>/`.
- Every file below every subfolder of the project folder, except `memory/`, to `<archive>/projects/<enc>/<subfolder>/`. These session folders hold `subagents/`, `tool-results/` and `workflows/`. A session folder is copied even when no transcript sits beside it.
- The session-keyed stores, to `<archive>/session-artifacts/`. A full `awt archive` sweep copies `file-history/`, `session-env/`, `tasks/` and `todos/` whole. A file is archived even when its session has no transcript or session folder left.
- `--session`, the hook and `associate --export` copy the stores per session id instead: `file-history/<id>/`, `session-env/<id>/`, `tasks/<id>/`, and the `todos/` files whose name contains the id. The ids come from the transcript names and the session folder names.

**What it deliberately does not copy.**

- Project `memory/` folders, because they are project notes and not session history.
- Top-level files in a project folder that are not transcripts, for the same reason.
- Anything behind a junction or symlink. The sweep skips the link and protects the rest (AC-61, the junction skip).

**Care for the archive folder.** `session-env/` holds hook environment scripts that can contain environment values. Transcripts and tool results can hold pasted secrets. Give the archive folder the same protection as the Claude home.

**Archive layout.**

```
<archive-dir>/
  projects/<enc>/<sessionId>.jsonl
  projects/<enc>/<sessionId>/...        subagents, tool-results, workflows
  session-artifacts/{file-history,session-env,tasks}/<sessionId>/...
  session-artifacts/todos/<file>.json   a sweep copies every file; a per-session run copies files whose name contains the id
  manifest.json                         source path, destination and SHA-256 of every archived file
  index.json
```

`--render` and a human-readable `INDEX.md` are not built yet.

**Purpose:** preserve session history. Can be run manually, on a schedule, or wired into Claude Code as a `SessionEnd` hook so every session is archived automatically when it closes.

**Per-command flags:**

| Flag | Required | Description |
|---|---|---|
| `--archive-dir <PATH>` | yes (for archival and hook operations) | Destination directory for archived data. A relative path is made absolute, and `.` and `..` are removed, before use. The folder must not contain the Claude data (see below) |
| `--session <PATH>` | no | Archive a single transcript; path must be a `.jsonl` file. Requires `--archive-dir` |
| `--install-hook` | no | Register the `awt archive` `SessionEnd` hook in `~/.claude/settings.json`, with a 60-second timeout. Requires `--archive-dir`, and records its absolute path |
| `--uninstall-hook` | no | Remove the `awt archive` `SessionEnd` hook from `~/.claude/settings.json` |
| `--set-retention <DAYS>` | no | Set `cleanupPeriodDays` in `~/.claude/settings.json`. Minimum safe value is 1 (see issues #23710 and #62272 before setting to 0) |
| `--force-zero` | no | Required to set `cleanupPeriodDays` to 0. Read the known issues before using |
| `--hook-stdin` | no | Read `SessionEnd` hook JSON from stdin and archive the `transcript_path` field. Requires `--archive-dir`. Used by the installed hook; not normally called by hand |
| `--render` | no | Reserved for future HTML rendering of archived sessions. Has no effect in v1.0 |

**Default behavior (no session or hook flags):** archive every file listed under "What it copies" that is new or changed.

**Output:** `archived: N copied, N skipped, N failed`. With `--json`, one object with numeric `copied`, `skipped` and `failed` fields.

**Failed files.** Running sessions hold some of these files open, such as `tasks/<id>/.lock`, so a read can be refused. The sweep records each file it cannot read or write, prints `archive failed: <path>: <reason>` to stderr, and carries on. A failed file gets no new manifest entry. A file that an earlier run archived keeps its old entry and its old copy when it later fails. A failed write never leaves a partial copy behind. The next full sweep tries the file again.

A folder that cannot be listed is recorded as a failure too. A store folder that does not exist is not a failure.

**Leftover temp files.** Each copy is written to `<name>.<token>.awt-tmp` and then renamed. A run that is killed can leave that file in the archive folder. Nothing removes it, and it is safe to delete.

**Archive folder inside the tree.** A source path under `--archive-dir` is never copied. An archive folder placed deep inside the Claude home therefore does not copy itself. A path such as `~/.claude/backups/x` or a project's own subfolder is allowed.

**Archive folder that holds the Claude data.** The command refuses with exit 2 when `--archive-dir` equals or contains `~/.claude`, `projects`, `file-history`, `session-env`, `tasks` or `todos`. That includes the home folder and a drive root. Without this check the run would skip its own sources, copy nothing and exit 0. `--install-hook` makes the same check, so the hook never records a useless path.

**The hook.** `--install-hook` sets a 60-second `timeout` on the hook. Claude Code gives `SessionEnd` hooks 1.5 seconds by default, and 60 seconds is the most a hook may ask for. A hook installed by an older `awt` has no timeout. Run `awt archive --install-hook --archive-dir <path>` again to replace it.

A hook run archives only its own session. A full `awt archive` sweep is what retries every other failed file. A rerun reads and hashes both sides of each file, so on a large home it is not free.

**Cloud-sync warning:** if `--archive-dir` resolves to a path under a known sync root (`OneDrive`, `Dropbox`), a warning is printed to stderr. The archive proceeds; the warning is advisory.

**Exit codes used:** 0, 1, 2 (locked - missing required companion flag, or an archive folder that holds the Claude data), 4 (hook stdin contained invalid JSON or missing `transcript_path`).

A failure to read a file exits 0. The installed `SessionEnd` hook runs while other sessions are live, and a locked file must not turn a working hook into a failing one. The failures are printed and counted instead.

A failure to write into the archive folder exits 1 on a full sweep and on `--session`. The result prints first, and with `--json` stdout stays one object. Hook mode (`--hook-stdin`) exits 0 for both kinds of failure.

Exit 1 also means the sweep could not run. The archive folder or `manifest.json` could not be written, or the `--session` transcript does not exist.

`awt associate --export` is stricter, because its export is a safety copy. It exits 1 and re-associates nothing if any file failed.

---

### `awt repair`

Repair Claude state that is **damaged** rather than merely stale. Dry run by default.

**Purpose:** a stale reference points at a folder that is gone, and the correct response is to
leave it alone. A damaged reference is different: it is corrupted in a way that can be mechanically
undone. `repair` handles the second case, and only where the answer is unambiguous.

**Per-command flags:**

| Flag | Required | Description |
|---|---|---|
| `--drive-letter` | yes (one repair must be selected) | Repair `history.jsonl` `project` values whose drive letter was replaced by a colon, e.g. `::\Projects\X` where `E:\Projects\X` belongs |
| `--apply` | no | Perform the repair. Without it the command writes nothing |

Passing no repair flag is an error (exit 2 - nothing to do). Each repair is a separately named and
separately guarded transformation; there is deliberately no "repair everything you can find" mode,
because that is unbounded inference rather than a bounded fix.

**The guard.** A value is repaired **only** when exactly one existing drive makes the corrected
path resolve on disk. Zero candidates is reported as unrepairable and left untouched. Two or more
is refused as ambiguous and left untouched. The output names both declined sets with the reason,
so what was skipped is as visible as what was done.

**Output (text):**

```
Repairable: 34 value(s), 2303 history line(s)
  ::\Projects\alpha -> E:\Projects\alpha  (15 line(s), drive E is the only match)
  ...
Not repairable (12): no existing drive makes these resolve, so there is nothing to repair to.
Refused as ambiguous (0): more than one drive would resolve. Choosing would be guessing.

Dry run: nothing was written. Re-run with --apply to repair.
```

**Output (`--json`):** `{ "path", "repairs": [...], "unrepairable": [...], "ambiguous": [...],
"totals": { "repairable_values", "repairable_lines", "unrepairable_values", "ambiguous_values" } }`.
Each declined entry carries a `reason`.

**Safety:** `--apply` snapshots `history.jsonl` before the first write, count-checks every
replacement, and is recoverable with `awt rollback --report <manifest>` like any other write. Only
`history.jsonl` is written. Repair is idempotent: running it again finds nothing to do and exits 0.

**Exit codes used:** 0 (including a dry run that found nothing), 1 (I/O), 2 (no repair selected),
3 (verification failed after write), 4 (`history.jsonl` is not valid UTF-8 - a different
corruption class than the drive-letter substitution, refused rather than lossily decoded).

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

The export uses the same per-session copy as `awt archive --session`, so it includes session folders, `session-env/`, `tasks/` and `todos/`. If any file cannot be copied, the command exits 1 before re-associating anything.

With re-association on, the plan is built before the export. A plan refusal exits 2 and writes nothing into `--to`. If no readable transcript resolves the project, the export does nothing, and the re-association still runs.

With `--no-reassociate`, that same case refuses with exit 2 and says that nothing was exported. The command never prints an export path over an empty folder.

**A project folder with no readable transcript.** A folder under `~/.claude/projects/` can hold only `memory/`, only session folders, or transcripts with no `cwd`. `awt plan`, `apply` and `associate` then decide by the folder's encoded name, which is lossy, so a name alone is not enough:

- **The folder moves** if the path is known: the source folder exists, or `~/.claude.json`, `history.jsonl`, plugin state or the folder's own nested transcripts record the path. awt warns when the name was the only link.
- **The folder stays,** with a warning, if something ties it to another path. That means a transcript, or another recorded path with the same encoded name. The move itself still goes ahead.
- **The command exits 2** if two look-alike paths both claim the folder, or if the destination's folder belongs to a look-alike.
- **`associate` exits 2** if nothing records the path at all, so a typo cannot take another project's folder.

**One limit remains.** awt cannot see a look-alike project that nothing records. If `E:\foo_bar` exists and no file names it, moving `E:\foo-bar` can take a memory-only folder that was really `foo_bar`'s. Read the warning, and use `awt rollback` if the folder was not this project's.


**Output:** `associate complete: N changes applied`.

**Exit codes used:** 0, 1, 2 (guard - both no-ops requested, collision refused, or an export-only run with no resolving transcript).

---

## Notes on flags

- `--no-auto-rollback` is intended for debugging. In normal use, leave auto-rollback enabled.
- `--force` lifts the IDE lock guard, which only sees `*.lock` files under `~/.claude/ide/`. A
  session in a plain terminal writes none, so that guard never fires for one. Exit every session
  running in the source folder yourself before `apply` or `associate`.
- `--recursive`, `--on-collision`, and `--scope` existed in pre-release builds and were
  **removed** (AC-58, 2026-07-30). None was implemented behind its help text, and two of them
  silently weakened guards: `keep-dest`/`keep-src` bypassed the collision check while changing
  nothing else, and `--recursive` suppressed the nested-project warning while moving nothing.
  A move whose source contains nested projects now refuses outright - move the children first.
  There is exactly one rewrite behavior: the moved project's own transcripts plus every
  path-keyed store. Collisions always refuse.
