# Contributing

Thanks for your interest. This is a Windows-first, deterministic, offline Rust CLI that edits a
user's Claude Code state, so correctness and safety come before features. Read
[AGENTS.md](AGENTS.md) (the operating manual) and [docs/DESIGN.md](docs/DESIGN.md) (the
architecture) before making a change of any size.

## Prerequisites

- Rust stable and cargo (see `rust-toolchain.toml`).
- Windows is the primary platform for v1.0; the engine has no OS-specific logic outside the
  path-encoding module.

## Build, test, lint

Run all of these before opening a pull request; CI runs the same on `windows-latest`:

```
cargo build --release
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

CI additionally enforces (see `.github/workflows/ci.yml`):

- **Dependency hygiene** - `awt-core` must not depend on `tauri`, `clap`, `reqwest`, `ureq`,
  `hyper`, or `curl`.
- **No network** - no network-capable crate may appear anywhere in the tree. The migration path
  makes zero network or LLM calls; `crates/awt-core/tests/no_network_deps.rs` guards this.
- **`cargo audit`** - no unresolved advisories.

## Non-negotiable conventions

- **No em-dashes (U+2014) or en-dashes (U+2013)** anywhere - code, comments, strings, docs, or
  commit messages. Use `-` or restructure. A local hook enforces this on writes.
- **Determinism** - no LLM, no network, no wall-clock or randomness in the migration path;
  identical inputs produce identical outputs.
- **Safety invariants** - back up before any write, verify each postcondition after, hard-fail
  (never guess) on an unrecognized store shape, and refuse rather than surprise the user. New
  behavior must preserve these.
- **Fixtures are sanitized once** and never refreshed from live files without repeating the
  sanitization step (see the TDD plan Task 1.3 and `docs/DESIGN.md` Section 8).
- **Test-driven** - write a failing test first, then the implementation. Every acceptance
  criterion should trace to a test (see the S-01 traceability doc for the pattern).

## Documentation

Every new document gets a row in [docs/index.md](docs/index.md) and a doc-impact entry in
`docs/CHANGELOG.md` (the orphan rule). User-facing release notes go in the root `CHANGELOG.md`
(keepachangelog); `docs/CHANGELOG.md` is a doc-impact log, not a code changelog.

## Commits and pull requests

- Branch from `main`; do not commit directly to `main`.
- Use clear, conventional commit subjects (`fix:`, `feat:`, `docs:`, `test:`, ...).
- Open a pull request and let CI go green before merging. The repo keeps a linear history
  (rebase merges).
- Agent-authored commits carry `Co-Authored-By:` and a session trailer; see the recent history
  for the format.

## Agentic work

If you drive changes with an AI agent, follow [AGENTS.md](AGENTS.md). The standing guardrail: an
implementer subagent never commits - a controlling session reviews the diff, verifies the gates
itself, and commits. See the session logs in `_agent-context/session-log/` for how past work was
executed and reviewed.
