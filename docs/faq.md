# FAQ

Short answers to the questions people actually ask before trusting a tool that edits their
Claude Code state. For step-by-step instructions see [quickstart.md](quickstart.md); for
"I have situation X" see [recipes.md](recipes.md); for what a specific exit code means see
[troubleshooting.md](troubleshooting.md).

## Safety and trust

### Is this safe to run against my real Claude data?

The design assumes it is not, and works accordingly: it snapshots every file it will touch
(with sha256) before the first write, count-checks every replacement, re-reads the result from
disk afterwards, and automatically rolls back if any check fails. `rollback` then proves the
revert by re-hashing every restored file against the snapshot.

That said: **practice on a copy first.** Not because the tool is expected to fail, but because
that is how you find out what it does before it does it to something you care about. Use
`scripts/new-scratch-home.ps1` and pass `--home` at the copy.

### What is the worst case if something goes wrong?

The guards are built to fail closed, so the realistic worst case is "nothing happened and you
got exit 2, 3, or 4". During the 2026-07-28 acceptance run two genuine defects were hit, and in
both cases the tool refused, auto-rolled-back, and proved every restored file byte-identical.
Nothing was lost.

The one thing outside that safety net is the folder move itself: `apply` performs a real
`src` -> `dst` rename on disk. `rollback` moves it back.

### Does it send anything over the network, or call an LLM?

No, and this is enforced structurally rather than promised. `crates/awt-core/tests/no_network_deps.rs`
fails the build if any network-capable crate (`reqwest`, `ureq`, `hyper`, `curl`, ...) appears
anywhere in `Cargo.lock`, and CI re-checks the dependency tree per package. The tool is
deterministic: identical inputs produce identical outputs, with no wall-clock or randomness in
the migration path.

### Do I need to close Claude Code first?

Yes, close any session working in the project you are about to move. `awt` detects a live lock
and refuses with exit 2 rather than editing state another process is holding. You can override
with `--force`, but the refusal exists for a reason.

### Will it touch my other projects' transcripts?

No. Other projects' transcripts frequently mention the old path, and that is correct - they are
a historical record of what was said at the time. The tool's postcondition is "zero old-path
references in the moved project's own path-keyed state", not "zero old-path references
anywhere". A match is not staleness.

## What it actually changes

### What does moving a project actually touch?

Claude Code keys state to a project's absolute path across several places. `awt` handles six:

| Store | What changes |
|---|---|
| `claude.projects` | The `projects/<encoded-dir>/` directory is renamed and each transcript's recorded `cwd` is rewritten |
| `claude.json` | The `projects{}` key is renamed |
| `claude.json` `githubRepoPaths` | Array values pointing at the old path are rewritten |
| `claude.history` | `project` fields in `history.jsonl` are rewritten |
| `plugin.state` | The plugin state dir, whose name is `sha256(newAbsPath)[:16]`, is renamed |
| `sweep.unknown` | Reported only, never rewritten |

Run `awt scan --src <path>` to see exactly which of these hold state for one project, or
`awt plan` to see precisely what would change.

### Why does `doctor` report things it says it will never fix?

Those are the "report only" findings. They live in vendored and archival regions - `plugins/`,
`file-history/`, `backups/` - where an old path is *correct by design*, because those files are
records of a past state. Rewriting them would corrupt history rather than repair it. They are
surfaced so you can see them, and structurally excluded from the rewrite path.

### What is an "unresolvable project dir"?

A transcript directory whose sessions never recorded a `cwd`. Because the path encoding is lossy
and forward-only (`a-b`, `a.b`, `a\b`, and `a_b` all encode to the same directory name), the
directory name alone cannot be decoded back to a path. With no recorded `cwd` there is nothing to
resolve it against, so the tool reports it rather than guessing.

### Can I move a project to a different drive?

Not in v1.0. Cross-volume moves are refused with exit 2. A same-volume move is a rename, which
is atomic and trivially reversible; a cross-volume move is a copy-verify-delete with genuinely
different failure modes, and it is deferred to v1.x rather than half-implemented.

## Retention and the 30-day cliff

### What is the "30-day cliff"?

Claude Code auto-deletes transcripts after 30 days by default, with no warning and no recovery.
`awt list` shows each project's oldest transcript age so the cliff is visible, and `awt archive`
copies transcripts to a durable folder before it takes them. Archiving is incremental and
deduplicated by content hash, so re-running is cheap.

### Should I just set `cleanupPeriodDays` to 0?

No. The documentation implies 0 disables cleanup, but community reports say otherwise:
[#23710](https://github.com/anthropics/claude-code/issues/23710) reports that 0 disables
transcript *writing*, and [#62272](https://github.com/anthropics/claude-code/issues/62272)
reports the cleanup is mtime-based. `awt archive --set-retention` therefore takes a large finite
value, and setting 0 requires an explicit `--force-zero` after reading those issues.

### Does `history.jsonl` expire too?

No, and the asymmetry matters. Prompts in `history.jsonl` persist indefinitely while transcripts
are deleted at 30 days - opposite lifetimes. That is why a long-dead project often still has
history and JSON state but no transcripts.

## Recovery and undo

### How do I undo a move?

```
awt rollback --report <backup-root>\awt-<run-id>\manifest.json
```

The manifest path is printed by `apply`. Note `--report` is a named flag, not a positional
argument.

### I already moved the folder by hand. Can awt still fix it?

Yes, that is what `awt associate --from <old> --to <new>` is for, and it works even when the old
folder no longer exists on disk. See the [recipes](recipes.md).

### How do I remove awt and everything it set up?

Uninstall the binary with `cargo uninstall awt-cli`. If you installed the archive hook, remove it
first with `awt archive --uninstall-hook`, which takes it back out of `~/.claude/settings.json`.
Archived data is a plain directory tree you can delete or keep; nothing depends on the tool being
installed to read it.

## Platform and scope

### Does it work on macOS or Linux?

v1.0 is Windows-first and only tested there. The engine has no OS-specific logic outside the
path-encoding module, so a port is plausible rather than a rewrite, but it is not a v1.0 claim.

### Does it work with Codex, Gemini, or other agents?

Not yet. The store-adapter boundary was built so that additional agents slot in behind it, and
Codex and Gemini adapters are parked as v1.x candidates. The project was renamed from
`claude-project-mover` to `agent-workspace-tools` specifically so that expansion would not
orphan the name (see [ADR-0001](decisions/0001-project-name-agent-workspace-tools.md)).

### What happens if Claude Code changes its on-disk format?

The tool hard-fails with exit 4 and writes nothing. Every store is probed for a recognized shape
before any write; an unfamiliar shape is an abort, not a best-effort guess. If you see exit 4,
report the format so an adapter can be updated - do not try to force the run.

### Why Rust, and not a shell script?

Three properties a script cannot cheaply give: a count-checked byte-splice rewrite that refuses
when the live count differs from the plan, a `FileSystem` trait that lets the entire engine be
tested against an in-memory filesystem rather than your real one, and a structurally enforced
"no network crate in the dependency tree" guarantee. The prior art in this space is also
overwhelmingly Unix-first and needs WSL or Git Bash on Windows.
