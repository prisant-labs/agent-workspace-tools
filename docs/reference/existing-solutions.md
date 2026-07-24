# Existing Solutions and Prior Art (Reference)

Are there tools that already do what Agent Workspace Tools (awt) sets out to do?
Yes, in every category - but with a clear, defensible gap. This document surveys
the field so we cite prior art honestly and build only the part that is missing.

Survey date: 2026-07-10 (web research). Star counts and dates are as of then and
will drift. Organized by awt's four capability areas.

## Bottom line

- **Inventory** and **export/viewing** are crowded; do not build viewers from
  scratch - cite and optionally reuse them.
- **Retention/backup** is thin but real, and hampered by open Anthropic bugs no
  tool fully addresses.
- **Relocation/migration** (awt's core) has about five direct competitors, but
  **every capable one is Unix-first** (requires WSL or Git Bash) and none is
  Windows-native. That, plus correctness on the edge cases they get wrong, is the
  wedge.

## A. Report / inventory

Well covered, but almost entirely through a **usage/cost** lens, not a
"what projects and files exist" lens.

| Tool | URL | Notes |
|------|-----|-------|
| ccusage | github.com/ccusage/ccusage | ~17k stars, very active. Reads local JSONL from Claude Code and 16+ other CLIs; daily/weekly/session cost and token reports, per-project grouping, JSON export. The dominant tool. Usage lens only. |
| Claude-Code-Usage-Monitor | github.com/Maciek-roboblog/Claude-Code-Usage-Monitor | ~8.4k stars, Python. Real-time TUI, burn-rate, optional local warehouse to keep history past 30 days. |
| ClaudeHistoryMCP | github.com/jhammant/ClaudeHistoryMCP | ~65 stars, TS. MCP server indexing projects + history.jsonl with BM25/TF-IDF search. |

**Gap awt fills:** no tool presents a plain "here are all your projects, N sessions
each, X MB on disk, plus todos/file-history/shell-snapshots" inventory. That framing
(feature 1) is open.

## B. Retention / backup / archival

Real but immature, and the platform itself fights you here. Open Anthropic issues
confirm the pain:

- **#23710** - `cleanupPeriodDays: 0` reportedly disables transcript **writing**,
  not cleanup (see `claude-data-model.md` Section 5).
- **#62272** - cleanup keys off file **mtime**, so a high value still loses old
  untouched sessions.
- **#62476 / #59248** - silent deletion reports.
- **#64721** - feature request for a native export/backup (does not exist).

| Tool | URL | Notes |
|------|-----|-------|
| cross-code-organizer | github.com/mcpware/cross-code-organizer | ~354 stars, JS, active. Dashboard + backup of `~/.claude` to `~/.claude-backups/` with git-tracked diffs. Covers A+B+D. |
| claude-code-sync | github.com/perfectra1n/claude-code-sync | ~78 stars, **Rust**, active. Pushes `~/.claude/projects/` to a private git repo on a schedule; pull to restore across machines. B+C. |
| claude-conversation-extractor | github.com/ZeroSumQuant/claude-conversation-extractor | ~638 stars, Python. Extract JSONL to Markdown/text before expiry. B+D. |
| claude-session-tracker | github.com/ej31/claude-session-tracker | ~44 stars, JS. The only one that **exfiltrates** sessions out of `~/.claude` (to GitHub Issues via a hook), surviving cleanup by design. |
| claude-code-backup-guide | github.com/jtklinger/claude-code-backup-guide | Shell. Backs up the full `~/.claude` to a private git repo. |
| claude-file-recovery | github.com/hjtenklooster/claude-file-recovery | ~106 stars, Python TUI. Reconstructs files from Write/Edit tool calls in transcripts. |

**Gap awt fills:** no tool prevents deletion at the source or archives keyed on
content hash to dodge the mtime bug; all copy on a schedule and inherit #62272. awt's
feature 2 (archive-out + SessionEnd hook + safe retention setting, content-hash
dedup) addresses the bug directly. Note `claude-code-sync` (Rust, git-repo target) is
the closest to awt's approach and worth studying.

## C. Relocation / migration - awt's core space

Not greenfield. About five direct competitors, all Unix-first. The top three were
fetched and verified.

| Tool | URL | Notes |
|------|-----|-------|
| clamp (claude-move-project) | github.com/wsagency/claude-move-project | 37 stars, Shell, v1.4.1 (Feb 2026). Most complete: rename encoded dir, rewrite history.jsonl, dry-run, pack/unpack backup, `--verify`/`--fix`, `--list`. Windows only via WSL/Git Bash. |
| claude-code-project-mover | github.com/skydiver/claude-code-project-mover | 20 stars, Shell. Renames the dir AND rewrites path references inside `.jsonl`. fzf picker, compressed backup. macOS/Linux only. |
| claudepath | github.com/Mahiler1909/claudepath | 17 stars, Python, v1.1.2 (Jul 2026). Rewrites `cwd` and file paths inside `.jsonl` line-by-line, recurses into subagent JSONLs. pip/pipx/Homebrew; no Windows testing documented. |
| claude-repath | github.com/xPeiPeix/claude-repath | Python, ~2 stars. Claims to patch sessions, memory, todos, worktrees; ships as a plugin. |
| project-move | github.com/JaviOFC/project-move | Python, ~2 stars. Moves sessions + memories + permissions + file history. |
| gwpl gist / maleta `claude-rename` | (gists) | The canonical write-ups of the encoding scheme. Both flag the `.`-to-`-` rule others miss. |

**Gaps awt fills:**

1. **Windows-native.** Every capable tool needs WSL or Git Bash, or is Unix-path
   focused. None handles Windows `\`, drive letters (`E:\`), and case variants
   natively. awt lives on `E:\` under PowerShell - this is the differentiator.
2. **Pre-move inventory (A+C together).** None lists projects/session counts before
   you choose one to move.
3. **Correctness on edge cases.** Dots in paths, subagent JSONLs, `githubRepoPaths`,
   plugin state dirs, and non-path occurrences of the name are handled
   inconsistently or not at all across these tools. awt's golden-fixture tests
   against a real reference move are a concrete correctness advantage.

## D. Export / viewing - very crowded, do not rebuild

15+ tools. Reuse or cite; awt's optional HTML report (feature 1) should borrow from
these rather than reinvent rendering.

| Tool | URL | Notes |
|------|-----|-------|
| claude-code-transcripts | github.com/simonw/claude-code-transcripts | ~1.6k stars, Python. JSONL to paginated HTML, `--gist` upload. |
| claude-code-log | github.com/daaain/claude-code-log | ~1.1k stars, Python, active. HTML/Markdown, TUI browser, date filtering, token stats, timeline. Most feature-rich. |
| claude-code-history-viewer | github.com/jhlee0409/claude-code-history-viewer | ~1.8k stars, **Tauri (Rust+TS)**, most-starred viewer. Browse/search/analytics. Directly relevant to awt's eventual GUI. |
| claude-history | github.com/raine/claude-history | ~386 stars, Rust TUI, fuzzy search, resume/fork. |

Others: kiliman/claude-transcript, annenpolka/cclog (Go), withLinda/claude-JSONL-browser,
d-kimuson/claude-code-viewer, plus VS Code extensions.

## What this means for awt

- **Build:** the Windows-native mover with correctness guarantees (core v1), the
  project inventory (feature 1), the archive-out + hook retention (feature 2), and
  the re-associate/export (feature 3). This exact combination exists in no single
  tool.
- **Cite as prior art:** clamp, claudepath, skydiver (movers); claude-code-sync
  (Rust git-repo archival, closest to feature 2); the gwpl gist (encoding); Anthropic
  issues #23710, #62272, #64721 (retention behavior and the missing-native-backup
  case).
- **Reuse / do not rebuild:** transcript rendering (simonw/claude-code-transcripts,
  daaain/claude-code-log) for feature 1's HTML; claude-code-history-viewer as a
  reference for the eventual Tauri GUI.

## Sources

Web research, 2026-07-10. GitHub repositories and Anthropic issue tracker as cited
inline. Star counts and activity as of the survey date; verify before publishing any
public comparison claim.
