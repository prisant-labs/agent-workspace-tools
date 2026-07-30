# Troubleshooting

What each exit code means, what the tool is telling you, and what to do next. Every `awt`
command returns a script-friendly exit code. The guards refuse loudly and write nothing
rather than guess.

## Exit codes

| Code | Meaning | Typical cause |
|------|---------|---------------|
| 0 | Success | The command completed; for `apply`, changes were made and verified |
| 1 | I/O error | A file could not be read or written (permissions, a missing path) |
| 2 | Guard refusal - nothing was written | A safety guard fired before any change |
| 3 | Verification failed | A post-apply or post-rollback check did not hold |
| 4 | Unrecognized format | A store did not match a known shape; the tool refused to touch it |

Codes 2, 3, and 4 are distinct on purpose: a script can tell a refusal (2) from a bad
write (3) from a shape the tool would not touch at all (4). Code 1 is the catch-all I/O
error and never collides with a refusal.

## Guard refusals (exit 2)

Each of these stops the run before any change is made and tells you what to do.

- **Destination exists.** The destination folder (or a `~/.claude.json` key for it)
  already exists. Choose a destination that does not exist, or resolve the collision.
- **Worktree source.** The source's `.git` is a file, not a directory, so it is a git
  worktree; moving it would break its linkage. Move the real repository, or pass `--force`
  if you understand the consequences.
- **Cross-volume move.** "cross-volume move refused: source ... is on a different volume
  from destination ... Cross-volume moves are not supported in v1.0 (spec AC-2, deferred).
  Move within the same volume instead." v1.0 moves are same-volume renames; cross-volume
  copy-verify-delete is a v1.x feature.
- **Live lock.** "live IDE lock detected - a running CLI instance may be editing this
  project. Close the running CLI first, or pass --force to proceed anyway." Editing state
  a running CLI also holds can conflict. Close the other instance, or pass `--force` (which
  proceeds with a warning).
- **Ambiguous history.** "the project dir ... could belong to more than one live path
  (...). The tool will not guess which path is correct - resolve this manually. The
  --attribute resolver is planned for v1.x." Two or more still-existing paths are recorded
  for the same transcript history (a rename plus a surviving clone). The tool refuses
  rather than attribute the history to the wrong project. Resolving this automatically via
  an `--attribute fork|base|both` flag is planned for v1.x.

## Re-running `apply` is idempotent (by refusal)

Running `apply` a second time on an already-migrated project makes no changes and refuses
with the destination-exists guard (exit 2). That refusal is the idempotency signal: the
destination already holds the migrated state. A script that loops `apply` should treat exit
2 with a destination-exists message as "already done", not as a failure. (This is the S-01
AC-19 contract.)

## Verification failed (exit 3)

`verify`, and `apply`'s built-in post-apply verification, return 3 when any postcondition
does not hold (the dir is not at the new path, a `cwd` was not rewritten, an old reference
remains where none should, JSON did not parse, or a line count changed). Read the per-check
`FAIL` lines to see which postcondition failed, then inspect that store. `apply` with
auto-rollback on (the default) will have already restored the pre-migration state.

## Unrecognized format (exit 4)

The tool found a store it does not recognize - for example a `~/.claude.json` that does not
parse, or whose `projects` value is not an object. It refuses to touch it rather than guess.
This usually means Claude Code changed its on-disk format. Do not force the run; report the
format so an adapter can be updated.

## The report artifacts

Every `apply` and `rollback` leaves a machine-readable record, always, even without
`--json`. The record sits beside the sha256 backup snapshot, so "what happened" and "the
bytes to undo it" live in the same place.

- **`apply`** writes `<backup-dir>/report.json`: the run id, every action taken and its
  count, the backup location, and the verification result. Pass `--json` to also print it
  to stdout for scripting.
- **`rollback`** writes `<manifest-dir>/rollback-report.json`: the run id and a per-file
  proof that each restored file is byte-identical to its pre-migration original (a
  verifiable revert). If any file does not match, the rollback fails with exit 3. Pass
  `--json` to print it to stdout.

## `awt repair` reports things it will not repair

That is the command working correctly, not failing. `repair --drive-letter` fixes a damaged
`history.jsonl` entry only when exactly one existing drive makes the corrected path resolve. Two
other outcomes are reported rather than acted on:

- **Not repairable** - no existing drive makes the path resolve, so there is nothing to repair
  *to*. The folder is gone as well as the reference being damaged.
- **Refused as ambiguous** - more than one drive would resolve, so choosing one would be guessing.
  The competing candidates are named.

Both are listed with their line counts so you can see exactly what was skipped and why. The exit
code is still 0: declining to repair is a correct outcome, not an error.

## Where the backup lives

`apply` takes a full sha256 snapshot of every file it will modify before the first write,
under `<backup-root>/awt-<run-id>/` (the backup root defaults to the system temp dir; set it
with `--backup-root`). To undo an apply, run
`awt rollback --report <backup-root>/awt-<run-id>/manifest.json`.
