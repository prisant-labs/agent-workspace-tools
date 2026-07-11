# CLAUDE.md

Read AGENTS.md first. It is the operating manual for all agentic work on this repo.

## Claude-specific notes

- Use `superpowers:subagent-driven-development` for plan execution (task by task from `docs/superpowers/plans/2026-07-10-claude-project-mover.md`).
- Wrap sessions with `jp-wrap-session` and save logs to `_agent-context/session-log/`.
- The audit report at `_local/audit/2026-07-10_fable-audit/AUDIT_REPORT.md` is local-only context. When citing findings in committed text, reference the `docs/` path the finding traces to, not the local audit file.
