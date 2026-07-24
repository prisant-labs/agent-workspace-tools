# CLAUDE.md

Read AGENTS.md first. It is the operating manual for all agentic work on this repo.

## Claude-specific notes

- Use `superpowers:subagent-driven-development` for plan execution (task by task from `docs/superpowers/plans/2026-07-10-claude-project-mover.md`).
- Wrap sessions with `jp-wrap-session` but save logs to `_local/_session-logs/`, NOT the skill's default `_agent-context/session-log/`. Session logs are gitignored, local-only notes; never commit them.
- The audit report at `_local/audit/2026-07-10_fable-audit/AUDIT_REPORT.md` is local-only context. When citing findings in committed text, reference the `docs/` path the finding traces to, not the local audit file.
