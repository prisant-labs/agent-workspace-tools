<a id="readme-top"></a>

# agent-workspace-tools

**A deterministic, offline, Windows-native CLI for the state a coding agent leaves scattered across your machine.**

Move a project folder and `awt` repairs every stale path reference Claude Code leaves behind in
`~/.claude` - transcripts, history, per-project settings, plugin state - so `--resume` and your
per-project config keep working. Plus tooling to inventory, archive, and re-associate that state.
No LLM, no network, backup-before-write, verify-after, single-command rollback.

[**What it is**](#-what-it-is) &nbsp;·&nbsp; [**Quick start**](#-quick-start) &nbsp;·&nbsp; [**Commands**](#-what-awt-does) &nbsp;·&nbsp; [**Safety**](#-safety-model) &nbsp;·&nbsp; [**Status**](#-status-and-roadmap) &nbsp;·&nbsp; [**Docs**](#-documentation)

[![CI](https://github.com/prisant-labs/agent-workspace-tools/actions/workflows/ci.yml/badge.svg)](https://github.com/prisant-labs/agent-workspace-tools/actions/workflows/ci.yml)
![Status](https://img.shields.io/badge/status-pre--release-orange?style=flat-square)
![Platform](https://img.shields.io/badge/platform-Windows-0078D6?style=flat-square)
![Rust](https://img.shields.io/badge/Rust-stable-b7410e?style=flat-square)
![Network](https://img.shields.io/badge/network-zero-brightgreen?style=flat-square)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](LICENSE)

> ⚠️ **Pre-release, in active development.** v1.0 is feature-complete but **not yet tagged** - it has
> not finished its final acceptance run against real data, and commands may still change. Because
> `awt` edits Claude Code state under `~/.claude`, read the [quickstart](docs/quickstart.md) and
> **always run it against a copy of `~/.claude` first**.

---

## 🧭 What it is

**A cohesive suite over one shared model of the state a coding agent accumulates about your projects - of which "move a project" is the first capability.**

Moving a project folder is easy. The hard part is everything Claude Code stores *about* the
project outside the folder, all keyed to the project's old absolute path. After a move, Claude
Code no longer recognizes the project: session resume fails, per-project history is gone, and
saved settings orphan. That state is spread across several files in `~/.claude` and is not easy
to find or fix by hand. On top of that, Claude Code silently auto-deletes transcripts after 30
days - no warning, no recovery.

`awt` handles both: it relocates a project and provably migrates all of its Claude state, and it
gives you the tools to see and protect that state before the 30-day cliff takes it.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## ⚡ Quick start

Install from source (Rust stable + cargo required):

```bash
cargo install --path crates/awt-cli
# or, without installing:
cargo build --release   # -> target/release/awt
```

Then, safest path first - practice against a **copy** of your Claude home:

```bash
awt list  --home "C:\Temp\claude-copy"                       # see every project Claude has state for
awt doctor --home "C:\Temp\claude-copy"                      # find stale path references
awt plan  --home "C:\Temp\claude-copy" --src "E:\old" --dst "E:\new"   # dry run, writes nothing
awt apply --home "C:\Temp\claude-copy" --src "E:\old" --dst "E:\new" --backup-root "C:\Temp\bk"
awt verify --home "C:\Temp\claude-copy" --src "E:\old" --dst "E:\new"
```

> 📖 Full first-run walkthrough: [`docs/quickstart.md`](docs/quickstart.md). Exit codes and recovery: [`docs/troubleshooting.md`](docs/troubleshooting.md).

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## 🧰 What awt does

| Command | What it does |
|---|---|
| `awt doctor` | Scan your whole Claude install for stale path references across all projects |
| `awt scan` | Show all Claude state that exists for one specific project |
| `awt plan` | Show exactly what a move would change - a dry run that writes nothing |
| `awt apply` | Move a project and migrate all state: backup first, count-check every edit, verify, auto-rollback on failure |
| `awt verify` | Independently confirm a completed migration is correct |
| `awt rollback` | Restore from a backup snapshot, then prove each file is byte-identical to pre-migration |
| `awt list` | Inventory every project Claude has state for - session counts, sizes, and transcript ages (so the 30-day cliff is visible) |
| `awt archive` | Copy transcripts and session artifacts to a durable folder before auto-delete removes them; incremental, content-hash deduplicated |
| `awt associate` | Re-link a deprecated project's history to a replacement path, even when the old folder is gone |

Every `apply` and `rollback` also writes an always-on machine-readable record (`report.json` /
`rollback-report.json`) beside the backup. Add `--json` to any command for machine-readable output.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## 🛡️ Safety model

**`awt` refuses to guess, and it never writes without a way back.**

- **Probe before write.** Every store is checked for a recognized format first; an unknown shape
  is a hard abort before anything changes (exit 4).
- **Backup before write.** A sha256 snapshot of every file the run will touch is taken first.
- **Count-checked edits.** Every text replacement is boundary-anchored; if the live count differs
  from the planned count, the write is refused.
- **Verify after write.** An independent pass re-reads the result from disk; any failed
  postcondition triggers automatic rollback from the snapshot.
- **Verifiable revert.** `rollback` re-hashes every restored file against the snapshot and proves
  byte-identity (exit 3 on any mismatch).
- **Fail closed.** Destination exists, git-worktree source, cross-volume move, a live lock, or an
  ambiguous history each stop the run with a plain-language message and a documented exit code -
  never a silent surprise.
- **Deterministic and offline.** Zero LLM and zero network calls in the migration path, enforced
  by a dependency-guard test.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## 🗺️ Status and roadmap

| Release | Contents | Status |
|---|---|---|
| v0.1.0 | `awt doctor` and `awt scan` - read-only | Internal milestone (not tagged) |
| **v1.0.0** | Full CLI: mover (`plan`/`apply`/`verify`/`rollback`) plus `list`, `archive`, `associate` | **Feature-complete; all 24 mover acceptance criteria verified.** Tag pending the maintainer spec sign-off and the manual acceptance run. |
| v1.x (parked) | Cross-volume move; Codex and Gemini adapters | Behind the existing adapter boundary; promotable when scheduled |
| v2.0.0 | Tauri 2 + React GUI over the identical core | Deferred; security and native-parity baselines first |

Full program plan: [`docs/ROADMAP.md`](docs/ROADMAP.md). How the system works (architecture):
[`docs/DESIGN.md`](docs/DESIGN.md).

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## 📚 Documentation

| Doc | For whom | Read when |
|---|---|---|
| [`docs/quickstart.md`](docs/quickstart.md) | Everyone | Your first run, safest path first |
| [`docs/reference/commands.md`](docs/reference/commands.md) | Everyone | Per-command reference: flags, behavior, exit codes |
| [`docs/troubleshooting.md`](docs/troubleshooting.md) | Everyone | What an exit code means and how to recover |
| [`docs/DESIGN.md`](docs/DESIGN.md) | Engineers | Architecture: store model, safety rules, CLI surface |
| [`docs/reference/claude-data-model.md`](docs/reference/claude-data-model.md) | Engineers | How Claude Code stores project state |
| [`docs/acceptance-run.md`](docs/acceptance-run.md) | Maintainers | The manual acceptance run before a tag |
| [`docs/ROADMAP.md`](docs/ROADMAP.md) | Maintainers | Program plan for v1 and v2 |
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | Contributors | Build, test, conventions, PR flow |
| [`docs/index.md`](docs/index.md) | Everyone | The full documentation index |

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## 🤝 Contributing

Rust stable and cargo are required. Build and test with `cargo test --workspace`; see
[`CONTRIBUTING.md`](CONTRIBUTING.md) for the full lint gates, the non-negotiable conventions
(offline, deterministic, no em-dashes, fixtures sanitized once), and the PR flow. Read
[`AGENTS.md`](AGENTS.md) before any agentic work on this repo.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## 📄 License

MIT. See [`LICENSE`](LICENSE).

<p align="right">(<a href="#readme-top">back to top</a>)</p>
