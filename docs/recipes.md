# Recipes

Task-oriented walkthroughs: "I have situation X, what do I run?" For a linear first run see
[quickstart.md](quickstart.md); for full flag detail see
[reference/commands.md](reference/commands.md).

Every command below accepts `--home <dir>` to target a Claude home other than your own. While
you are learning the tool, pass it and point at a scratch copy.

---

## Try everything safely first

```powershell
.\scripts\new-scratch-home.ps1 -Destination "E:\_temp\awt-practice"
awt list --home "E:\_temp\awt-practice"
```

The script copies both halves of your Claude home (`.claude\` and the sibling `.claude.json`),
refuses to copy into itself, and prints the `--home` value to use. Everything you do against
that copy is disposable. Delete it with `Remove-Item -Recurse -Force "E:\_temp\awt-practice"`.

Note that `apply` still performs a **real folder move** of `--src` to `--dst`; `--home` only
redirects the Claude state. When practicing, use a throwaway folder as `--src`.

---

## Move or rename a project

Close any Claude Code session working in the project first, or the lock guard will refuse.

```powershell
awt plan  --src "E:\Projects\old-name" --dst "E:\Projects\new-name"
awt apply --src "E:\Projects\old-name" --dst "E:\Projects\new-name" --backup-root "E:\awt-backups"
awt verify --src "E:\Projects\old-name" --dst "E:\Projects\new-name"
```

Read the `plan` output before applying: it lists the folder move, the `projects/` directory
rename, each transcript rewrite with its edit count, the `~/.claude.json` key change, and the
`history.jsonl` edits. `apply` runs `verify` itself and auto-rolls-back on failure; running
`verify` again afterwards is an independent second opinion.

Note the backup path `apply` prints. You need its `manifest.json` to undo.

Both paths must be on the same drive. Cross-volume moves are refused in v1.0.

---

## Undo a move

```powershell
awt rollback --report "E:\awt-backups\awt-<run-id>\manifest.json"
```

`--report` is a named flag, not a positional argument. Rollback restores every file to its
pre-migration bytes, moves the folder back, and proves the revert by re-hashing each restored
file against the snapshot. Exit 3 means at least one file did not come back byte-identical;
capture the `rollback-report.json` and stop.

To confirm the undo took, re-run `verify` for the same src/dst. It should now **fail** - that
failure is the confirmation.

---

## I already moved the folder by hand

This is the common case: you renamed a repo, moved a directory in Explorer, or cloned to a new
location, and Claude Code lost the thread.

```powershell
awt scan --src "E:\Projects\old-name"                                    # confirm state exists
awt associate --from "E:\Projects\old-name" --to "E:\Projects\new-name"
```

`associate` works even when the old folder no longer exists on disk. By default it does two
things: re-associates the history to the new path, and exports a portable copy of those sessions
into `<new-path>\.claude-sessions`. Split them with `--no-export` (re-associate only) or
`--no-reassociate` (export only), and change the export location with `--export-subdir`.

`associate` works even when the project's transcripts have already expired. Transcripts are
auto-deleted after 30 days while `history.jsonl` never expires, so a long-dead project typically
has `claude.json` and history state and nothing else - which is the normal case here, not an edge
case. When there are no transcripts left to copy, the export step is simply a no-op and the
re-association still runs.

---

## Protect transcripts before the 30-day cliff

One-off:

```powershell
awt archive --archive-dir "E:\claude-archive"
```

Incremental and content-hash deduplicated, so re-running only copies what changed. A second run
over an unchanged home reports everything skipped in a few seconds.

Automatic, on every session close:

```powershell
awt archive --archive-dir "E:\claude-archive" --install-hook
```

This registers a `SessionEnd` hook in `~/.claude/settings.json` that archives each session's
transcript as it closes. Remove it with `awt archive --uninstall-hook`.

Avoid pointing `--archive-dir` inside OneDrive or Dropbox; the tool warns when it detects a known
sync root. Sync clients rewrite and lock files underneath you, which is the opposite of what an
archive wants.

### Give yourself more runway

```powershell
awt archive --set-retention 3650
```

Sets `cleanupPeriodDays` in `~/.claude/settings.json`. Do **not** set it to `0` - see the
[FAQ](faq.md#should-i-just-set-cleanupperioddays-to-0). Setting 0 requires `--force-zero`
precisely so it cannot happen by accident.

---

## Recover history entries with a corrupted drive letter

If `awt doctor` reports stale `history.jsonl` values that look like `::\Projects\something` -
a colon where the drive letter belongs - that history is not merely stale, it is **damaged**.
Claude Code cannot match those lines to any project, so those prompts are unreachable.

See what could be recovered. This writes nothing:

```powershell
awt repair --drive-letter
```

It proposes a repair only where exactly one existing drive makes the corrected path resolve.
Anything that resolves nowhere, or on more than one drive, is listed as declined with the reason
rather than guessed at. Read that list before proceeding.

When it looks right:

```powershell
awt repair --drive-letter --apply --backup-root "E:\awt-backups"
```

Snapshots the file first, count-checks every replacement, and is undoable with
`awt rollback --report <manifest>` like any other write. Only `history.jsonl` is touched, and
running it again finds nothing to do.

> **Stale is not damaged.** A stale reference points at a folder that is genuinely gone, and the
> right response is to leave it alone - `repair` will not touch it. This command exists for the
> narrower case where the reference itself is corrupted and the correction is unambiguous.

---

## Audit your whole install

```powershell
awt doctor
```

Reports every path-keyed reference pointing at a folder that no longer exists, grouped by store,
plus report-only findings in archival regions and any unresolvable project dirs.

A first run over an uncached home takes a while (around 98 seconds on a 3.3 GB home); a warm run
is a few seconds.

For a browsable inventory instead:

```powershell
awt list --html "E:\claude-inventory.html"
```

The HTML page is written **in addition to** the terminal table, and is self-contained.

---

## Find what is consuming disk, and what is about to expire

```powershell
awt list --json | ConvertFrom-Json |
  Sort-Object bytes -Descending |
  Select-Object -First 15 cwd, sessions, bytes, oldest_days, health |
  Format-Table -AutoSize
```

`oldest_days` is the number to watch against the 30-day cliff. `health` is `ok`, `stale`
(contains stale references), or `unresolved` (transcripts with no resolvable `cwd`).

---

## Hand a project to another machine

```powershell
awt associate --from "E:\Projects\thing" --to "E:\Projects\thing" --no-reassociate
```

Same path for both, with re-association disabled, exports the project's sessions to
`E:\Projects\thing\.claude-sessions` without changing any state. Copy the folder to the other
machine.

---

## Script it

Every command takes `--json`, and the exit codes are stable and distinct: `0` success, `1` I/O
error, `2` guard refused and nothing was written, `3` verification failed, `4` unrecognized store
format. A script can tell a refusal from a bad write from a shape the tool would not touch.

Re-running `apply` on an already-migrated project is idempotent **by refusal**: it exits 2 with a
destination-exists message. Treat that specific combination as "already done", not as an error.

```powershell
awt apply --src $src --dst $dst --json
switch ($LASTEXITCODE) {
    0       { "migrated" }
    2       { "refused, nothing written - check the message" }
    3       { "verify failed, auto-rollback ran" }
    4       { "unrecognized store format - report it, do not force" }
    default { "I/O error" }
}
```
