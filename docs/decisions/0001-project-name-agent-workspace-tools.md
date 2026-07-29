---
status: accepted
date: 2026-07-17
decision-makers: jprisant
consulted: claude-code (opus 4.8)
informed: future contributors
---

# Rename the project to agent-workspace-tools

## Context and Problem Statement

The project began as `claude-project-mover` (binary `cpm`), scoped to one job:
move a project folder and repair the stale path references Claude Code leaves
behind in `~/.claude`. The scope has since grown. The roadmap (`docs/ROADMAP.md`)
now commits the v1.0 CLI to `doctor`, `scan`, the mover (`plan`/`apply`/`verify`/
`rollback`), plus `list` (F13), `archive` (F14), and `associate` (F15) - a
maintenance suite for the state a coding agent accumulates about your projects,
of which "move" is one capability. The roadmap also parks Codex and Gemini
adapters (P11, AC-27) as v1.x candidates, and the five-adapter store boundary that
makes that possible already shipped in Phase 3.

Two questions followed: does the name still describe the tool, and if not, when is
the cheapest moment to change it.

## Decision Drivers

- The name should describe the whole suite, not its first feature ("mover").
- The name is the single component that is expensive to change after the first
  release tag (repo URL, links, installed binary, muscle memory); the code behind
  the adapter boundary is designed to grow incrementally and is cheap to extend.
  So the name is precisely the thing worth future-proofing, and the pre-v0.1,
  unpublished state is the cheapest this change will ever be.
- A name must not promise what the tool does not do.
- The multi-agent direction is already written into the roadmap, not speculative.

## Considered Options

- `claude-project-mover` (status quo)
- `claude-project-manager`
- `claude-session-tools`
- `claude-workspace-tools`
- `agent-workspace-tools` (chosen)
- suffix wording: `-tools` vs `-utilities`

## Decision Outcome

Chosen: **`agent-workspace-tools`**, because it names the whole suite, is
agent-agnostic so the planned Codex/Gemini adapters do not orphan the name, and
future-proofs the one component that is costly to change later.

The suffix is **`tools`**, not `utilities`: "utilities" connotes a grab-bag of
unrelated helpers, while this is a cohesive suite over one shared model of agent
state. "Tools" is also the shorter, more conventional choice for a CLI suite.

### Consequences

- Good: the name survives the multi-agent expansion the roadmap already plans.
- Good: "workspace" accurately frames the subject as the agent's view of all your
  projects and their state.
- Bad: dropping `claude` also invalidates the `cpm` acronym (claude project mover),
  so the binary and the `cpm-core` / `cpm-cli` crate names no longer fit. They were
  renamed to `awt` / `awt-core` / `awt-cli` in a deferred code pass, which landed
  2026-07-24 (see More Information). The temporary inconsistency this consequence
  predicted (repo `agent-workspace-tools`, binary `cpm`) existed from 2026-07-17 to
  2026-07-24 and is now closed.
- Neutral: past session logs, `docs/CHANGELOG.md` entries, git history, and the
  archival regions under `~/.claude` (file-history, backups) are historical records
  and are preserved, not rewritten - the same "a match is not staleness" rule the
  tool itself enforces.

### Confirmation

The rename is confirmed complete when: the GitHub repo is `agent-workspace-tools`,
the git remote points at it, live in-repo references are updated, the local folder
is renamed, and the Claude Code transcript directory plus project memory are
repointed so no live reference to the old path survives outside historical records.

**Confirmed 2026-07-24** for the repo, remote, in-repo references, and local folder
(commit `c1d8969`, CI green). One residue remains outside the repo, in the
maintainer's own `~/.claude`: `E:\Projects\prisant-labs\claude-project-mover` still
appears as a `projects` key, in two `githubRepoPaths` entries, and across 12
`history.jsonl` lines. Repairing it is the job of `awt associate`, which cannot
currently do it - see AR-02 in
`docs/internal/release-plans/plan_v1.0.0/acceptance-run-2026-07-28.md`. The project's
own rename is therefore a live test case for its own tooling, and the residue stays
until AR-02 is fixed.

## Pros and Cons of the Options

### `claude-project-mover` (status quo)

- Bad: names one feature, not the suite; `claude` blocks the planned multi-agent
  expansion.

### `claude-project-manager`

- Good: keeps the `cpm` acronym (claude project manager), minimal churn.
- Bad: "project manager" collides with the Jira/Asana meaning and reads as a much
  broader effort; still `claude`-locked.

### `claude-session-tools`

- Bad: in Claude Code's own vocabulary a "session" is a transcript, which is the one
  thing this tool deliberately never rewrites (transcripts are the source of truth).
  The name points at the wrong noun and over-promises. Still `claude`-locked.

### `claude-workspace-tools`

- Good: "workspace tools" accurately frames the subject.
- Bad: `claude` still blocks the multi-agent direction.

### `agent-workspace-tools` (chosen)

- Good: describes the suite, agent-agnostic, future-proofs the expensive-to-change
  component.
- Cost: invalidates `cpm`; binary and crate renames become follow-up work.

## More Information

Executed 2026-07-24 on branch `rename-cpm-to-awt`.

The rename covered: the crate directories (`crates/cpm-core` -> `crates/awt-core`,
`crates/cpm-cli` -> `crates/awt-cli`), Cargo package names, the `[[bin]]` name,
every `cpm_core`/`cpm_cli` Rust identifier, every `cpm <subcommand>` invocation in
tests and docs, the CI dependency-hygiene gates, and the "CPM" / "Claude Project Mover"
branding. Historical records (session logs, existing `CHANGELOG.md` entries, the
dated plan in `docs/superpowers/plans/`) were preserved.

Original rationale (preserved for context):

- Binary rename `cpm` -> **`awt`**, accepted 2026-07-17. Chosen over a memorable
  wordmark for predictability: `awt` is the repo's initials, so there is one name to
  remember, and it reads fine as a command prefix (`awt doctor`, `awt scan`). Repo
  name and binary name need not match; here, matching them was the deciding virtue.
- Crate renames follow: `cpm-core` -> `awt-core`, `cpm-cli` -> `awt-cli`.

Related: the sweep-scope decision (`docs/superpowers/plans/2026-07-10-claude-project-mover.md`),
which applies the same preserve-vs-rewrite distinction to `doctor` output.
