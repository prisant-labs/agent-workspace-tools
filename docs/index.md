# awt Documentation Index

This index lists every document in the repository, who it is for, and when to read it. Every new document added to the project must get a row in this table and an entry in `docs/CHANGELOG.md`.

| Doc | Audience | Read when |
|---|---|---|
| [README.md](../README.md) | Non-engineers, everyone | Orientation - start here |
| [docs/quickstart.md](quickstart.md) | Everyone | First run, safest path first: install through plan/apply/verify/rollback |
| [docs/faq.md](faq.md) | Everyone | Before trusting the tool with real data: safety, what it changes, retention, recovery, scope |
| [docs/recipes.md](recipes.md) | Everyone | "I have situation X, what do I run?" - move, undo, already-moved-by-hand, archive, audit, script it |
| [docs/glossary.md](glossary.md) | Everyone, AI agents | Decoding the vocabulary: store, reverse index, encoded dir, anchored rewrite, guard, honesty checkpoint |
| [CHANGELOG.md](../CHANGELOG.md) | Everyone | User-facing release changelog (keepachangelog) |
| [SECURITY.md](../SECURITY.md) | Everyone | Reporting a vulnerability; what is in scope; the no-network and distribution posture |
| [docs/index.md](index.md) (this file) | Everyone | Finding a doc |
| [docs/DESIGN.md](DESIGN.md) | Engineers, AI agents | Understanding the architecture, store model, safety rules, CLI surface, and phase plan |
| [docs/ROADMAP.md](ROADMAP.md) | Maintainers | Understanding the full program plan for v1 and v2 |
| [docs/superpowers/plans/2026-07-10-claude-project-mover.md](superpowers/plans/2026-07-10-claude-project-mover.md) | Engineers, AI agents | Executing the TDD implementation plan task by task |
| [docs/features/v1.1-inventory-retention-reassociate.md](features/v1.1-inventory-retention-reassociate.md) | Engineers | F13-F15 spec: list, archive, associate |
| [docs/reference/commands.md](reference/commands.md) | Everyone | Per-subcommand reference: all 9 commands, their flags, and the exit-code contract |
| [docs/reference/claude-data-model.md](reference/claude-data-model.md) | Everyone technical, AI agents | How Claude Code stores project state - read before any store adapter work |
| [docs/reference/existing-solutions.md](reference/existing-solutions.md) | Evaluators, engineers | Prior art survey |
| [docs/internal/maintainer-todo.md](internal/maintainer-todo.md) | Maintainer | **The single human to-do list.** What is left before the tag, and which items only a human can clear |
| [docs/internal/release-plans/](internal/release-plans/) | Maintainers | Release scaffolding and acceptance criteria aggregation |
| [docs/internal/release-plans/plan_v1.0.0/acceptance-run-2026-07-30.md](internal/release-plans/plan_v1.0.0/acceptance-run-2026-07-30.md) | Maintainers | The passing happy-path acceptance run (gate f); its no-gate-remains verdict was later retracted, see the update note inside |
| [docs/internal/release-plans/plan_v1.0.0/S-04_safety-closeout/spec.md](internal/release-plans/plan_v1.0.0/S-04_safety-closeout/spec.md) | Maintainers | **The v1 safety closeout (gate g)**: verified data-loss and false-success findings as acceptance criteria, each blocking the tag |
| [docs/internal/release-plans/plan_v1.0.0/S-01_mover-cli/review-guide.md](internal/release-plans/plan_v1.0.0/S-01_mover-cli/review-guide.md) | Maintainer | The S-01 sign-off reading aid: every acceptance criterion in plain language, its evidence, and what currently contests it |
| [docs/internal/release-plans/plan_v1.0.0/acceptance-run-2026-07-28.md](internal/release-plans/plan_v1.0.0/acceptance-run-2026-07-28.md) | Maintainers | The first run: FAIL, with the AR-01..AR-04 analysis and their resolutions |
| [scripts/README.md](../scripts/README.md) | Everyone | What each helper script does, its parameters, and why it exists |
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

A row ending in `/` covers everything inside that directory, so individual ADRs under
`docs/decisions/` and the per-effort specs and plans under `docs/internal/release-plans/` do not
need their own rows. Give a file its own row when a reader would plausibly go looking for that
file specifically rather than browsing the directory.
