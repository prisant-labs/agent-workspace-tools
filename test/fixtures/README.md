# Test Fixtures

This directory holds golden test fixtures for agent-workspace-tools (awt).

## Structure

```
test/fixtures/
  reference-move/
    before/   - transcripts from the backup captured 2026-07-09, OLD path
    after/    - transcripts from live ~/.claude after migration, NEW path
    move.json - move descriptor with src/dst paths and expected pattern counts
  claude-json-variants/
    claude.json - synthetic minimized file with key-variant and githubRepoPaths groups
  plugin-state/
    markdown-for-humans-e854827f52137cd9/
      state.json - synthetic minimized plugin state file
```

## Provenance

### reference-move/before/

Source: backup at `E:\tmp\claude-move-backup-20260709-090053\transcripts_markdown-for-humans\`
captured on 2026-07-09 before the project move.

Contains two session transcripts for project `E:\Projects\Github Repos\markdown-for-humans`:
- `22b2362e-e4ef-4042-9b01-e3cba5719590.jsonl` (329 lines, ~716 KB)
- `28fd093e-f5ef-4dc7-af16-ea415c1840f7.jsonl` (2285 lines, ~8.3 MB)

### reference-move/after/

Source: live `C:\Users\jpris\.claude\projects\E--Projects-prisant-labs-vs-code-markdown-max\`
captured on 2026-07-11 after the project move.

Same two sessions, now reflecting the NEW path `E:\Projects\prisant-labs\vs-code-markdown-max`:
- `22b2362e-e4ef-4042-9b01-e3cba5719590.jsonl` (329 lines, ~716 KB)
- `28fd093e-f5ef-4dc7-af16-ea415c1840f7.jsonl` (2285 lines, ~8.3 MB)

### reference-move/after/claude.json

Synthetic minimized file. Represents post-move state: the NEW project key present, the OLD
key absent. NOT a copy of the live `~/.claude.json`.

### claude-json-variants/claude.json

Synthetic minimized file. Contains only the 3 real key-variant groups (values as empty objects)
and the 6 stale `githubRepoPaths` entries. No `mcpServers`, no other keys. Reflects the
key-casing and mixed-path diversity found in the real `~/.claude.json` without reproducing it.

### plugin-state/markdown-for-humans-e854827f52137cd9/state.json

Synthetic minimized file. Reduced to the minimal valid-JSON shape the adapter reads. NOT a
copy of the live plugin state.

## Sanitization performed (2026-07-11)

The four transcript files (before/ and after/) are real session logs. Before committing, they
were scanned for credentials using the following patterns (case-insensitive where relevant):

- `api[_-]?key` - 0 hits
- `token` - many lines; no credential-value pattern `token=<value>` found; all occurrences are
  discussion of token concepts (Claude API docs, code comments, tool names)
- `secret` - 13 lines across both files; all non-credential (MCP tool name
  `run_secret_scanning`, documentation text, CI/CD workflow echo commands)
- `password` - 0 hits
- `authorization` - 0 hits
- `bearer` - 0 hits
- `BEGIN [A-Z]+ PRIVATE KEY` - 0 hits
- `ghp_` - 0 hits
- `github_pat_` - 0 hits
- `sk-` - hits but all 3-character fragments (`sk-p`, `sk-summary-block`) - CSS classes, not
  API keys; `sk-ant-` and `sk-proj-` both 0 hits
- `eyJ` (JWT-like base64 strings) - 5 occurrences in 28fd; all are embedded in binary data
  streams (base64-encoded blobs), not standalone JWT tokens
- `.env` references - 1 line; mentions of `.env` files in gitignore context, no credential values

No credentials were found. No redaction was performed.

## Expected pattern counts (reference-move/before/)

These counts bind the before/ transcript set and are recorded in move.json:

| Pattern | Count | Method |
|---------|-------|--------|
| `"cwd":"E:\\Projects\\Github Repos\\markdown-for-humans"` | 1467 | lines (Grep count) |
| `E:\\Projects\\Github Repos\\markdown-for-humans\\` (backslash prefix) | 588 | occurrences (-o) |
| `E:/Projects/Github Repos/markdown-for-humans/` (forward prefix) | 27 | occurrences (-o) |
| `markdown-for-humans@` (npm version references, preserved) | 10 | occurrences (-o) |
| `markdown-for-humans_dev-` (branch/id references, preserved) | 55 | occurrences (-o) |

Note: The original plan brief listed `preserved_package_at: 8` and `preserved_branch_dev: 49`.
These were per-file counts for the larger session file (28fd) only. The correct totals across
both files are 10 and 55 respectively. move.json uses the correct totals.

## Standing rules

1. NEVER refresh these fixtures from live `~/.claude` files without re-running the full
   sanitization scan above and re-verifying the count table.
2. The synthetic files (`claude.json`, `state.json`) must NEVER be replaced with copies of
   live files. They exist specifically to avoid committing personal configuration.
3. After any redaction, re-run all 5 count assertions and confirm they match move.json.
