# CPM Documentation Index

This index lists every document in the repository, who it is for, and when to read it. Every new document added to the project must get a row in this table and an entry in `docs/CHANGELOG.md`.

| Doc | Audience | Read when |
|---|---|---|
| [README.md](../README.md) | Non-engineers, everyone | Orientation - start here |
| [docs/quickstart.md](quickstart.md) | Everyone | First run, safest path first: install through plan/apply/verify/rollback |
| [CHANGELOG.md](../CHANGELOG.md) | Everyone | User-facing release changelog (keepachangelog) |
| [docs/index.md](index.md) (this file) | Everyone | Finding a doc |
| [docs/DESIGN.md](DESIGN.md) | Engineers, AI agents | Understanding the architecture, store model, safety rules, CLI surface, and phase plan |
| [docs/ROADMAP.md](ROADMAP.md) | Maintainers | Understanding the full program plan for v1 and v2 |
| [docs/superpowers/plans/2026-07-10-claude-project-mover.md](superpowers/plans/2026-07-10-claude-project-mover.md) | Engineers, AI agents | Executing the TDD implementation plan task by task |
| [docs/features/v1.1-inventory-retention-reassociate.md](features/v1.1-inventory-retention-reassociate.md) | Engineers | F13-F15 spec: list, archive, associate |
| [docs/reference/commands.md](reference/commands.md) | Everyone | Per-subcommand reference: all 9 commands, their flags, and the exit-code contract |
| [docs/reference/claude-data-model.md](reference/claude-data-model.md) | Everyone technical, AI agents | How Claude Code stores project state - read before any store adapter work |
| [docs/reference/existing-solutions.md](reference/existing-solutions.md) | Evaluators, engineers | Prior art survey |
| [docs/internal/release-plans/](internal/release-plans/) | Maintainers | Release scaffolding and acceptance criteria aggregation |
| [docs/decisions/](decisions/) | Maintainers, AI agents | Architecture and naming decision records (MADR v4) |
| [docs/internal/v2-gui-design-brief.md](internal/v2-gui-design-brief.md) | Designers, design agents | Generating v2 GUI concepts - self-contained, no repo access needed |
| [AGENTS.md](../AGENTS.md) | AI agents | Operating manual: invariants, conventions, environment notes |
| [CONTRIBUTING.md](../CONTRIBUTING.md) | Engineers | How to build, test, and contribute; conventions and the PR flow |
| [docs/release-runbook.md](release-runbook.md) | Maintainers | v1.0 tag ceremony checklist: pre-tag gates, tag steps, signing posture |
| [docs/acceptance-run.md](acceptance-run.md) | Maintainers | Step-by-step manual acceptance run against a COPY of ~/.claude (the pre-tag honesty gate) |
| [docs/troubleshooting.md](troubleshooting.md) | Everyone | What an exit code means and what to do: the 0/1/2/3/4 contract, guard refusals, idempotency, and the report artifacts |
| [docs/CHANGELOG.md](CHANGELOG.md) | Everyone | Doc-impact history - updated with every doc change |

---

**Orphan rule:** every new document added to this repo gets a row in this table and an entry in `docs/CHANGELOG.md`.
