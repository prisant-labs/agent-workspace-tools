# Manual acceptance run

The one test that cannot be automated: driving the full CLI against a real Claude home on a
real machine. It is the honesty gate before the v1.0.0 tag. This guide is the detailed,
step-by-step form of the checklist in [release-runbook.md](release-runbook.md) Section 2 -
follow this, then tick the runbook boxes.

## The one rule

**Run against a COPY of `~/.claude`, never the live installation - certainly not on the first
run.** Every command below takes `--home <scratch>` so nothing touches your live state. The tool
is designed to be safe (backup before write, verify after, auto-rollback on failure), but the
point of an acceptance run is to prove that on your machine before you trust it with the real
thing.

## 0. Build

```
cargo build --release
```

The binary is `target/release/awt` (or install it with `cargo install --path crates/awt-cli`).
Confirm CI is green on the commit you intend to tag (release-runbook Section 1).

## 1. Make a scratch copy

Copy both the directory and the sibling JSON file to a scratch location on the SAME volume you
will test moves on (moves are same-volume in v1.0):

```
# PowerShell
Copy-Item -Recurse "$env:USERPROFILE\.claude"      "C:\Temp\claude-acceptance\.claude"
Copy-Item          "$env:USERPROFILE\.claude.json" "C:\Temp\claude-acceptance\.claude.json"
```

For every command below, `--home "C:\Temp\claude-acceptance"` points the tool at the copy. If you
ever see a command in this run without `--home`, stop - that is a mistake.

Pick a real project that has Claude state (something `awt list` shows with sessions). Call its
current path `<test-src>` and choose a non-existent same-volume destination `<test-dst>`.

## 2. Read-only checks (no writes yet)

```
awt doctor --home "C:\Temp\claude-acceptance"
```
Expect: exit 0, and a report of the stale-reference categories you know exist (stale
`githubRepoPaths`, stale `history.jsonl` values, an orphaned plugin dir). Compare the counts to
the last known-good baseline in the session log. A crash or an unexpected zero is a fail.

```
awt scan --home "C:\Temp\claude-acceptance" --src "<test-src>"
```
Expect: exit 0 and hits for that project across its stores.

```
awt list --home "C:\Temp\claude-acceptance"
```
Expect: exit 0 and a table with at least one row (session counts, sizes, ages). No panic, no I/O
error. Add `--json` to see the machine-readable form.

## 3. The move flow (the core of the test)

Dry run first - `plan` writes nothing:

```
awt plan --home "C:\Temp\claude-acceptance" --src "<test-src>" --dst "<test-dst>"
```
Expect: exit 0 and a change list - the folder move, the `projects/` directory rename, each
transcript rewrite with its edit count, the `~/.claude.json` key change, and the `history.jsonl`
edits. If a guard fires (destination exists, worktree source, cross-volume, a live lock, an
ambiguous history), the plan refuses with exit 2 and a plain-language message; resolve it per
[troubleshooting.md](troubleshooting.md) and re-run.

Apply it:

```
awt apply --home "C:\Temp\claude-acceptance" --src "<test-src>" --dst "<test-dst>" --backup-root "C:\Temp\claude-acceptance-backups"
```
Expect: exit 0, a line reporting the applied count and the backup path, and a `report.json`
written into the backup directory (`--json` also prints it to stdout). Note the backup directory -
you need its `manifest.json` for rollback.

Verify independently:

```
awt verify --home "C:\Temp\claude-acceptance" --src "<test-src>" --dst "<test-dst>"
```
Expect: exit 0 and every check `[ok]`. Any `FAIL` line is a real fail.

## 4. Prove rollback

```
awt rollback --home "C:\Temp\claude-acceptance" --report "C:\Temp\claude-acceptance-backups\awt-<run-id>\manifest.json"
```
Expect: exit 0, a per-file `[ok]` proof that each restored file is byte-identical to its
pre-migration original, a `revert verified: N/N checks passed` summary, and a `rollback-report.json`
beside the manifest. If any file did not come back byte-identical, rollback exits 3 - that is a
serious fail; capture the report and stop.

Confirm the copy is truly back to its pre-apply state by re-running verify - it should now FAIL
(the move has been undone), which is the expected confirmation that rollback worked:

```
awt verify --home "C:\Temp\claude-acceptance" --src "<test-src>" --dst "<test-dst>"
```
Expect: a non-zero exit with FAIL lines. That failure is success for this step.

## 5. Retention commands

```
awt archive --home "C:\Temp\claude-acceptance" --archive-dir "C:\Temp\claude-acceptance-archive"
```
Expect: exit 0 and an "archived: N copied, M skipped" line.

```
awt associate --home "C:\Temp\claude-acceptance" --from "<old-path>" --to "<new-path>"
```
Expect: exit 0. (Use a `<from>` that has history but whose folder may be gone - that is the case
`associate` exists for.)

## 6. Clean up

Delete the scratch copy, the scratch backups, and the scratch archive:

```
Remove-Item -Recurse -Force "C:\Temp\claude-acceptance", "C:\Temp\claude-acceptance-backups", "C:\Temp\claude-acceptance-archive"
```

## 7. Record the result

The acceptance run passes only if every step above met its expectation (including the
rollback-then-verify-fails confirmation). Record pass/fail and the commit SHA in the session log,
and tick release-runbook Section 2. If any step failed, do not tag; capture the failing command,
its exit code, and its `report.json` / `rollback-report.json`, and open an issue.

## Exit-code quick reference

`0` success, `1` I/O error, `2` guard refusal (nothing written), `3` verification failed, `4`
unrecognized store format. Full detail: [troubleshooting.md](troubleshooting.md).
