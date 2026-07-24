# AGENTS.md - awt Operating Manual

Read these files first, in this order:

1. This file (AGENTS.md) - operating rules.
2. `docs/DESIGN.md` - validated design: encoding rules, store adapter contract, phase plan, CLI surface.
3. `docs/superpowers/plans/2026-07-10-claude-project-mover.md` - executable TDD plan, task by task.
4. `docs/reference/claude-data-model.md` - how Claude Code stores project state; essential context before touching any store adapter.

---

## HARD INVARIANTS

These rules are load-bearing. Violating any one invalidates correctness guarantees for the whole tool.

**Encoding.** `encode_project_dir(abs)` replaces every non-`[A-Za-z0-9]` character with `-`, including dots. The encoding is lossy and forward-only: `a-b`, `a.b`, `a\b`, and `a_b` all encode identically. Never look up an existing project dir by computing `encode(src)`. Instead, build a reverse index by reading the `cwd` stored inside transcripts and mapping `normalize(cwd)` to dirs.

**Parse to validate, never to write.** Call `serde_json::from_str` to confirm the shape of a store file, discard the result, and write by literal boundary-anchored, count-checked byte splice. NEVER re-serialize a store file. Re-serializing reformats whitespace and defeats verification.

**Case-insensitive path matching.** Drive-letter case can differ between a normalized path and the value stored on disk (e.g., `d--Cloud-Work-PP` holds `cwd: "D:\Cloud-Work-PP"`). All path comparisons must be case-insensitive.

**Never touch another project's transcripts.** Other projects' transcripts may mention the old path (confirmed: 26 such transcripts on this machine). The correct postcondition is zero old-path refs in the moved project's own path-keyed state, not zero old-path refs everywhere. Other projects' transcripts are read-only at every rewrite tier.

**Zero LLM/network calls in any product code path.** `awt-core` and `awt-cli` make no outbound requests at runtime. Enforcement is structural: the CI dependency gate fails if any network-capable crate (reqwest, ureq, hyper, curl) enters the dependency tree.

**Store files must be valid UTF-8.** Hard-fail on any file that is not valid UTF-8. Never lossy-rewrite.

**All six v1 stores are in scope.** `claude.projects` (dir rename + transcript rewrite), `claude.json` (`projects{}` key rename + `githubRepoPaths{}` array value rewrite), `claude.history` (`project` field rewrite), `plugin.state` (dir rename via `sha256(newAbsPath)[:16]` suffix), and `sweep.unknown` (report only). `history.jsonl`, `githubRepoPaths`, and plugin state dirs are easy to overlook - they are covered and must be tested.

---

## Conventions

- No em-dashes (U+2014) or en-dashes (U+2013) anywhere: code, comments, docs, or commit messages. Use ` - ` or restructure.
- Commit messages end with two trailers. Check `git log` to see the exact format in use:
  ```
  Co-Authored-By: Claude <model> <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_...
  ```
- `awt-core` must never depend on `tauri` or `clap`. CI enforces this: `cargo tree -p awt-core | grep -iE 'tauri|clap' && exit 1`.
- Tests run against `MemoryFileSystem`, never against live `~/.claude`. Golden fixtures live in `test/fixtures/` and are never refreshed from live files without the sanitization step in plan Task 1.3.
- Every doc change gets a `docs/CHANGELOG.md` entry.
- Session logs go in `_agent-context/session-log/` using the `jp-wrap-session` convention. They are tracked in git.

---

## Environment notes

Platform: Windows 11, PowerShell primary. Rust stable (cargo 1.96.0) is installed.

Shell commands can hang in some harnesses on this machine. For file operations, prefer `Glob`, `Grep`, and `Read` tools over shell commands. For `cargo` and `git` commands, use generous timeouts or run in background, then verify results by reading files or checking exit codes. Do not assume a shell command completed successfully without confirming its output.

---

## Where new code goes

- `crates/awt-core/` - the engine; pure functions over an injectable `FileSystem` trait; no `clap`, no `tauri`, no network.
- `crates/awt-cli/` - the `awt` binary; thin `clap` wrapper only; delegates everything to `awt-core`.
- `test/fixtures/` - golden data; never refresh from live files without the sanitization step in plan Task 1.3.
- `src-tauri/` and `src/` - deferred (v2 GUI); do not add code there during v1 work.

---

## Execution mode

Implement the TDD plan task by task using `superpowers:subagent-driven-development` (fresh subagent per task, parent reviews between tasks). Each step is red-green: write the failing test first, run it, implement, run again, commit.

Phase 4 is the honesty checkpoint. `awt doctor` on the real machine must report exactly the residue found by hand: 6 stale `githubRepoPaths`, 11 stale `history.jsonl` values, and the `markdown-for-humans-e854827f52137cd9` plugin dir. All write-phase work (phases 5-9) is gated on passing this checkpoint.

---

## Releases

Release scaffolding lives in `docs/internal/release-plans/plan_v1.0.0/` (jp-release-plan convention). Acceptance criteria live in spec files; the release plan aggregates them.
