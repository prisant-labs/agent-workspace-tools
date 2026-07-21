# Claude Project Mover (CPM)

A deterministic, offline, Windows-native Rust CLI that relocates a project folder and migrates all Claude Code state keyed to its old absolute path.

> **v1.0 feature-complete.** Install from source: `cargo install --path crates/cpm-cli`, or clone and `cargo build --release`. See [docs/reference/commands.md](docs/reference/commands.md) for the full command reference.

## The problem

Moving a project folder is straightforward. The hard part is everything Claude Code stores about a project outside the folder - transcripts, history, per-project settings, and plugin state are all keyed to the project's old absolute path. After a move, Claude Code no longer recognizes the project: session resume fails, per-project history is gone, and saved settings become orphaned. This state is spread across several files in `~/.claude` and is not easy to find or fix by hand.

On top of that, Claude Code silently auto-deletes transcripts after 30 days by default. There is no warning and no way to recover them. Every day without archival is a day of conversation history that may disappear.

## What CPM does

- **doctor** - scan your Claude installation for stale path references across all projects.
- **scan** - show all Claude state that exists for a specific project.
- **plan** - show exactly what would change if you moved a project (dry run, no writes).
- **apply** - move a project and update all state; backs up first, count-checks every edit, verifies independently, and auto-rolls back on failure.
- **verify** - independently confirm that a completed migration is correct.
- **rollback** - restore from a backup snapshot if anything went wrong.
- **list** - inventory every project Claude has state for, with session counts, sizes, and transcript ages (so the 30-day auto-delete cliff is visible).
- **archive** - copy transcripts and session artifacts to a durable folder before the 30-day auto-delete removes them; incremental, deduplicated by content hash.
- **associate** - re-link a deprecated project's history to a replacement path, even when the old folder is already gone.

## Safety model

CPM refuses to write until it has probed every store for a recognized format - an unknown format is a hard abort before anything changes (exit 4). Before any write, it snapshots all files to a backup directory with a manifest. Every text replacement is boundary-anchored and count-checked; if the actual count differs from the planned count, the write is refused. After apply, an independent verify step re-reads the result from disk; any failed postcondition triggers automatic rollback from the backup snapshot. The tool refuses silently surprising actions - destination path already exists, nested project keys, worktree sources - and requires an explicit flag to override.

## Status and roadmap

| Release | Contents | Status |
|---|---|---|
| v0.1.0 (Phase 4 milestone) | `cpm doctor` and `cpm scan` - read-only | Shipped |
| v1.0.0 | Full CLI: mover (`plan`/`apply`/`verify`/`rollback`) plus `list`, `archive`, `associate` | Feature-complete; v1.0 tag pending |
| v2 | Tauri 2 + React GUI over the identical core | Deferred |

Full program plan: [docs/ROADMAP.md](docs/ROADMAP.md).

## Documentation map

| Doc | For whom | What |
|---|---|---|
| README (this file) | Everyone | Orientation: what CPM is, what it does, how to get started |
| [docs/reference/commands.md](docs/reference/commands.md) | Everyone | Per-subcommand reference: flags, behavior, exit codes |
| [docs/release-runbook.md](docs/release-runbook.md) | Maintainers | v1.0 tag ceremony checklist |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Maintainers | Program plan for v1 and v2 |
| [docs/DESIGN.md](docs/DESIGN.md) | Engineers | Validated design: architecture, safety model, CLI surface, phase plan |
| [docs/superpowers/plans/2026-07-10-claude-project-mover.md](docs/superpowers/plans/2026-07-10-claude-project-mover.md) | Engineers, AI agents | Executable TDD plan, task by task |
| [docs/features/v1.1-inventory-retention-reassociate.md](docs/features/v1.1-inventory-retention-reassociate.md) | Engineers | F13-F15 spec: list, archive, associate |
| [docs/reference/claude-data-model.md](docs/reference/claude-data-model.md) | Everyone technical | How Claude Code stores project state - readable standalone |
| [docs/reference/existing-solutions.md](docs/reference/existing-solutions.md) | Evaluators | Prior art survey |
| [docs/internal/release-plans/](docs/internal/release-plans/) | Maintainers | Release scaffolding (jp-release-plan convention) |
| [docs/index.md](docs/index.md) | Everyone | Full documentation index |
| [AGENTS.md](AGENTS.md) | AI agents | Operating manual: invariants, conventions, environment notes |
| [docs/CHANGELOG.md](docs/CHANGELOG.md) | Everyone | Doc-impact history |

## For contributors

Rust stable and cargo are required. Once product code exists: `cargo test --workspace`. Read [AGENTS.md](AGENTS.md) before any agentic work on this repo.

## License

MIT. See [LICENSE](LICENSE).
