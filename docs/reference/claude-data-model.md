# How Claude Code Stores Project State (Reference)

A plain-language map of everything Claude Code keeps about "a project," where it
lives, how it is keyed, how long it survives, and the traps. Written to answer a
specific confusion: across this project we say "session logs," but we really mean
**all** the metadata Claude keeps that references a project. This document defines
that surface precisely.

Grounded in direct inspection of `C:\Users\jpris\.claude` on 2026-07-09/10 and in
the official docs (code.claude.com/docs). Where the two disagree, that is called
out explicitly.

## 1. The core model: folder = project = an absolute path string

Claude Code has no "project" object with an ID. **A project is just an absolute
directory path.** Every piece of state below is tied to a project by storing, or
encoding, that path string. There is no database row, no UUID for the project
itself; the path *is* the identity.

Consequences that follow from this one fact:

- **Move or rename the folder and the linkage breaks.** Nothing inside Claude
  updates automatically. The session history still exists but is now keyed to a
  path that no longer holds that project.
- **The same folder can have more than one identity.** Windows paths appear as
  `E:\Projects\A`, `E:/Projects/A`, and `e:\Projects\A`; Claude stores whichever
  string the shell handed it at launch. One project can hold several key variants
  that differ only in slash direction or drive-letter case.
- **Two folders that are clones of the same repo are two different projects.** The
  path is the identity, not the git remote.
- **One session history can straddle a rename.** If you rename a folder mid-session
  or fork it, the recorded working directory (`cwd`) may point at the old or the
  base folder even though the work continued elsewhere. This is why attributing a
  history to a folder must read the stored `cwd`, not assume the folder name.

## 2. The directory-name encoding (and the bug everyone hits)

Transcripts live under `~/.claude/projects/<encoded-path>/`. The encoding replaces
**every character that is not ASCII `[A-Za-z0-9]` with a hyphen `-`.**

```
E:\Projects\prisant-labs\vs-code-markdown-max
  -> E--Projects-prisant-labs-vs-code-markdown-max
```

Verified corner cases on this machine:

- `.` is replaced too: `...\release-plans\v2.26.0` -> `...-release-plans-v2-26-0`.
- `.claude` -> `-claude`; spaces -> `-`; `Chrome - Bookmark Autosort` -> `Chrome---Bookmark-Autosort`.

**The common bug** (noted in community gists and in several existing tools): people
assume only `:`, `\`, `/`, and space are replaced, and forget the dot. A tool that
gets this wrong computes the wrong directory name and silently orphans the history.

**The encoding is lossy and cannot be reversed.** `a-b`, `a.b`, `a\b`, and `a_b`
all encode to `a-b`. You can compute the directory name from a path, but you can
never recover the path from the directory name. To find which folder a directory
belongs to, you must open a transcript inside it and read the stored `cwd`.

## 3. The keying taxonomy

Every store falls into one of three classes. This is the single most useful lens.

| Class | Identified by | Survives a folder move? | What breaks |
|-------|---------------|-------------------------|-------------|
| **PATH-keyed** | the absolute path (in the name or the contents) | No | Resume, per-project config, prompt history |
| **SESSION-keyed** | a session/agent UUID | Yes (name is fine) | Nothing structural; contents may hold a stale path |
| **GLOBAL** | not per-project | N/A | Only if a global file hardcodes the path |

## 4. Every data type, enumerated

### PATH-keyed (these are what a "move" must fix)

| Store | Path | What it holds | Notes |
|-------|------|---------------|-------|
| Project transcripts (dir) | `~/.claude/projects/<enc>/` | The session `.jsonl` files | Dir name is the encoded path. Rename on move. |
| Transcript contents | `<enc>/*.jsonl` | Every message, tool call, tool result. Each line carries a `cwd` field and absolute file paths. | Rewrite the `cwd` and anchored path prefixes. Preserve package/branch/prose mentions of the same string. |
| Tool-result spillover | `<enc>/<sessionId>/tool-results/` | Large tool outputs spilled to files | Moves with the dir. May contain paths. |
| Subagent transcripts | `<enc>/<sessionId>/subagents/` | Subagent conversation logs | Moves with the dir. |
| Project memory | `<enc>/memory/*.md` | Persisted notes/memory for the project | Moves with the dir. May reference paths. |
| Per-project config | `~/.claude.json` -> `projects{"<abs path>"}` | allowedTools, trust flags, MCP servers, last-session metrics | Migrate the key. Both slash and case variants can coexist. |
| GitHub repo paths | `~/.claude.json` -> `githubRepoPaths{"<slug>": ["<abs path>", ...]}` | Maps a GitHub slug to local checkout paths | **Often missed.** Values are arrays and can hold case variants. |
| Prompt history | `~/.claude/history.jsonl` -> `project` field | Every prompt you have typed, with the project path | **Never expires** (see retention). Rewrite the `project` field on a move. |
| Plugin state dirs | `~/.claude/plugins/data/<plugin>/state/<base>-<sha256(abs)[:16]>/` | Third-party plugin per-project state | Dir suffix is `sha256` of the path (backslash form), first 16 hex chars. Recompute for the new path. **Open-ended: any plugin can add one.** |

### SESSION-keyed (survive a move; contents may still hold a stale path)

Joined to a project by **sessionId** - the `.jsonl` basename under `projects/<enc>/`
is the sessionId, and these stores are named by that same id. That join is how you
enumerate "everything belonging to this project."

| Store | Path | What it holds |
|-------|------|---------------|
| Todo lists | `~/.claude/todos/<sessionId>-agent-<agentId>.json` | Persisted task lists |
| File-history snapshots | `~/.claude/file-history/<sessionId>/` | Pre-edit file snapshots for checkpoint/undo |
| Session env | `~/.claude/session-env/<sessionId>/` | Per-session environment capture |
| Task-tool state | `~/.claude/tasks/<id>/` | Subagent/Task-tool state |
| Shell snapshots | `~/.claude/shell-snapshots/` | Captured shell environment (timestamp/pid keyed) |
| Plan-mode files | `~/.claude/plans/` | Plans produced in plan mode |
| Debug logs | `~/.claude/debug/` | Per-session debug output |

### GLOBAL (review once; usually irrelevant to a move)

`~/.claude/settings.json`, `settings.local.json`, `CLAUDE.md`, `rules/`, `hooks/`,
`skills/`, `plugins/` (code), `commands/`, `usage.db` (~15.5 MB telemetry, not the
"15.5 GB" a stale note once claimed), `stats-cache.json`, `remote-settings.json`,
`ide/` (lockfiles), and the transient caches (`cache/`, `paste-cache/`,
`image-cache/`, `uploads/`, `downloads/`, `backups/`). These are per-project only
if a hook script or rule hardcodes a path, which must be scanned for, not assumed.

## 5. Retention: what expires, what does not, and a live footgun

This is the part most people get wrong, and it is actively costing you history.

- **Transcripts expire.** `~/.claude/projects/<enc>/*.jsonl` and most of the
  SESSION-keyed stores (file-history, session-env, tasks, tool-results, plans,
  debug, paste/image caches, backups) are deleted when older than
  **`cleanupPeriodDays`** (default **30**). Cleanup runs **on startup**, so old
  logs vanish the next time you launch Claude.
- **Measured on this machine (2026-07-10):** across 2,647 transcripts the oldest
  was 30 days and the median 28. Essentially nothing survives past 30 days. The
  30-day default is deleting your history right now, silently.
- **Prompt history does NOT expire.** `~/.claude/history.jsonl` is explicitly not
  covered by `cleanupPeriodDays`. Every prompt you have ever typed persists there
  (with its project path) until you delete it. So your prompts survive but the full
  conversations - the valuable part - do not.

### The `cleanupPeriodDays: 0` footgun (unresolved discrepancy)

- The **official docs** say `0` disables cleanup entirely (indefinite retention).
- **Community bug reports** (Anthropic issue #23710) say `0` silently disables
  transcript **writing** instead - so you would stop losing old logs by never
  recording new ones. Issue #62272 reports cleanup keys off file **mtime**, so even
  a large value loses old *untouched* sessions while recently-updated ones survive.

Until this is verified on your installed version, **do not set `0`.** The safe
posture: set a large finite value (e.g. `3650` = 10 years) as a safety net, and
treat **copying transcripts out to a durable archive** as the real retention
mechanism. An archive keyed on content hash (not mtime) also sidesteps #62272.

## 6. How to enumerate "everything for one project"

1. Compute the encoded dir from the path, OR (safer) find it via the reverse index
   (scan `projects/*`, read each dir's stored `cwd`, match on the normalized path).
2. The transcripts in that dir give you the set of **sessionIds** (the `.jsonl`
   basenames).
3. For each sessionId, collect matching entries in `todos/`, `file-history/`,
   `session-env/`, `tasks/`. That union is the project's full session footprint.
4. Add the PATH-keyed config: the `~/.claude.json` `projects{}` key(s), any
   `githubRepoPaths` values, the `history.jsonl` lines, and any plugin state dir
   whose hash matches the path.

That procedure is the backbone of the inventory (feature 1), the archive (feature
2), and the re-associate/export (feature 3) features.

## 7. Local vs remote

Everything above is **local plaintext** on your disk. Claude Code session state is
not synced to claude.ai; the web/desktop "projects" surface is a separate,
server-side store. Two implications:

- Your only copy of a CLI transcript is the local file. When cleanup deletes it,
  it is gone - there is no cloud version to restore from. This is the whole reason
  feature 2 exists.
- Transcripts hold whatever passed through tools, including `.env` contents, command
  output with credentials, and pasted secrets, all in plaintext. OS file
  permissions are the only protection. An archive inherits that exposure and should
  live somewhere with the same or tighter permissions.

## 8. Traps, in one place

- Dot-to-dash in the encoding (Section 2). The classic miss.
- The encoding is lossy; never invert it (Section 2).
- Directory name derives from the launch-time `cwd` string, so drive-letter case
  can differ from a normalized path; match case-insensitively (Section 1).
- `githubRepoPaths` and plugin state dirs are PATH-keyed and easy to forget
  (Section 4).
- A path string also appears in non-path contexts (npm package names, git branch
  names, prose). A rewrite must be boundary-anchored or it corrupts those.
- `history.jsonl` never expires and grows forever; transcripts expire at 30 days.
  Opposite lifetimes (Section 5).
- `cleanupPeriodDays: 0` may not mean what the docs say (Section 5).
- Cleanup keys off mtime, so retention is per-file, not per-project (Section 5).

## Sources

- Direct filesystem inspection of `~/.claude` and `~/.claude.json`, 2026-07-09/10.
- code.claude.com/docs: `claude-directory.md` (Application data, retention),
  `settings.md` (`cleanupPeriodDays`), `sessions.md` (`/export`).
- Anthropic issues #23710 (cleanupPeriodDays: 0 behavior), #62272 (mtime-based
  cleanup), #62476/#59248 (silent deletion), #64721 (native backup request).
- Companion prior-art survey: `existing-solutions.md`.
