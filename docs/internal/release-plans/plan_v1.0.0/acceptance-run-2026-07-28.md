---
title: "Manual acceptance run - 2026-07-28"
type: acceptance-run
release: v1.0.0
commit: 86841ff (plus uncommitted CLI help-text fixes)
result: FAIL
blockers: [AR-01, AR-04]
findings: [AR-01, AR-02, AR-03, AR-04]
all-findings-fixed: true
---

# Manual acceptance run - 2026-07-28

Run per [`docs/acceptance-run.md`](../../../acceptance-run.md) against a scratch copy of the
maintainer's live Claude home. **Result: FAIL.** Three defects found:

| ID | Severity | Status | Summary |
|---|---|---|---|
| AR-01 | **Release blocker** | **FIXED 2026-07-28** | `claude.json` `githubRepoPaths` rewrite fails on JSON escaping; `apply` and `associate` cannot complete for any project with such an entry |
| AR-02 | High | **FIXED 2026-07-28** | `associate` refuses a project whose transcripts have expired, which is the case it exists for |
| AR-03 | Medium for v1, blocking for v2 | **FIXED 2026-07-28** | `--json` is accepted and silently ignored by `plan` and `verify`; the v2 parity gate depends on `awt plan --json` |
| AR-04 | **Release blocker** | **FIXED 2026-07-28** | Two `githubRepoPaths` slugs holding the same path value each plan `expected: 1`, but each counts the whole file and sees 2. Found only after AR-01 was fixed - the two were stacked |

This document is the record of the run as it happened, with a resolution note under each finding.
**A full re-run of the sequence is still required before the tag**: only the steps that failed
were re-verified after each fix, not the whole sequence.

A note on how AR-04 was found, because it generalises. Fixing AR-01 changed the failure from
`expected 1, live 0` to `expected 1, live 2` - the anchor now matched, which revealed a second,
independent defect underneath it that the first had been masking. Two defects in the same code
path can present as one symptom, so "the error message changed" is progress, not success, and a
re-run against real data after each fix is what distinguishes them.

The safety machinery behaved correctly throughout: every failure failed closed, auto-rollback
fired, and every restored file was proven byte-identical. No data was lost or corrupted at any
point, including in the failing cases.

## Environment

| Item | Value |
|---|---|
| Scratch home | `E:\Projects\_temp\awt-acceptance-2026-07-28` (55,413 files, 3.30 GB) |
| Created by | `scripts/new-scratch-home.ps1` |
| Copy fidelity | `history.jsonl` SHA-256 identical between live and copy |
| Binary | `target/release/awt.exe`, `awt 1.0.0` |
| Live home touched | Never. Every command carried `--home <scratch>`. |

Real project folders were never moved. The write-path tests used `E:\tmp\chain-smoke`, a
disposable scratch folder, and a destination under `E:\tmp`.

## Results

| # | Step | Result | Notes |
|---|---|---|---|
| 1 | `doctor` | PASS | exit 0. Cold 98s, warm 5.9s. 75 stale, 1 report-only, 15 unresolved |
| 2 | `scan` | PASS | exit 0, correct hits per project including gone folders |
| 3 | `list` | PASS | exit 0, 1.6s, 58 rows |
| 4 | `plan` | PASS | exit 0, correct change list, both separator conventions planned |
| 5 | `apply` (transcripts only) | PASS | exit 0, 4 changes, folder moved, `report.json` written |
| 6 | `verify` | PASS | exit 0, all 6 checks `[ok]` |
| 7 | `rollback` | PASS | exit 0, 3/3 then 4/4 byte-identical proofs, folder restored |
| 8 | `verify` after rollback | PASS | exit 3 with FAIL lines, the expected confirmation |
| 9 | `apply` (with `githubRepoPaths`) | **FAIL** | exit 3, `VerifyFailed(".claude.json: expected 1, live 0")`. See AR-01 |
| 10 | `associate` (gone folder) | **FAIL** | exit 4 then exit 3. See AR-01 and AR-02 |
| 11 | `archive` | PASS | exit 0, 9,661 files / 0.98 GB in 83s |
| 12 | `archive` re-run | PASS | exit 0, 0 copied / 9,661 skipped in 5s - content-hash dedup works |

---

## AR-01: `claude.json` `githubRepoPaths` rewrite fails on JSON escaping

**Severity: release blocker.** Affects `apply` and `associate` for any project that has a
`githubRepoPaths` entry, which is the normal case for any repo cloned through `gh` or `git` on
Windows.

### Symptom

```
awt apply --home <scratch> --src E:\tmp\chain-smoke --dst E:\tmp\chain-smoke-moved
error: verification failed: apply failed
  (VerifyFailed("...\.claude.json: expected 1, live 0")); backup at ...\awt-1785296779
exit 3
```

### Root cause

`~/.claude.json` stores the same logical path in two separator conventions, and the
`githubRepoPaths` convention is JSON-escaped on disk:

- `projects` keys use forward slashes: `"E:/Projects/prisant-labs/nightscout-mcp"` (71 of 79 keys)
- `githubRepoPaths` values are arrays of **backslash** paths, so the raw file bytes contain the
  escaped form: `["E:\\Projects\\prisant-labs\\nightscout-mcp"]`

The planner derives its anchored replacement from the **parsed** JSON value, which is unescaped
(`E:\Projects\prisant-labs\nightscout-mcp`). The write is a literal boundary-anchored byte splice
against the **raw** file, where that literal never occurs. The count check therefore finds 0 where
it planned 1, and the run fails closed.

Measured directly against the real file:

```
count of 'E:\Projects\prisant-labs\nightscout-mcp'   in raw text: 0
count of 'E:\\Projects\\prisant-labs\\nightscout-mcp' in raw text: 1
```

This is the parse-to-validate-never-to-write invariant working exactly as designed on the write
side, paired with a planner that forgot the escaping on the read side. The two halves disagree
about which byte string they are talking about.

### Why the test suite missed it

The `plan` output looks correct, because `plan` prints the parsed (unescaped) value. Only `apply`
exercises the byte splice. Three gaps line up to hide it completely:

1. **The golden end-to-end fixture has no `githubRepoPaths` coverage.**
   `test/fixtures/reference-move/before/` has no `claude.json` at all, and
   `test/fixtures/reference-move/after/claude.json` carries `"githubRepoPaths": {}`. The one test
   that exercises the full write path therefore never rewrites a `githubRepoPaths` value.

2. **The fixture that *would* have caught it is orphaned.**
   `test/fixtures/claude-json-variants/claude.json` contains exactly the triggering shape, with
   properly escaped Windows paths inside `githubRepoPaths` arrays:

   ```json
   "githubRepoPaths": {
     "owner/markdown-for-humans": ["E:\\Projects\\Github Repos\\markdown-for-humans"],
     "owner/pp": ["D:\\Cloud-Work-PP", "d:/cloud-work-pp"]
   }
   ```

   Grepping the whole workspace for `claude-json-variants` returns **no matches in any `.rs`
   file**. The fixture was built and never wired to a test.

3. **`plan`-level assertions cannot catch it.** Any test asserting on the planned change list sees
   the unescaped value and passes. The defect only exists at the byte-splice layer.

The regression test must assert on the **raw bytes** of the rewritten file, not on the parsed
value or the plan. Wiring `claude-json-variants/claude.json` into an apply-level test is the
natural fix and would have caught this before it ever reached an acceptance run.

### Suggested fix

Escape the path when constructing the anchor for any value spliced into a JSON file
(`\` -> `\\`), and add the round-trip assertion above as a red test first. The `projects` key
path is unaffected because it uses forward slashes, which need no escaping - which is precisely
why the simple case passed and masked this.

### Reproduction

1. Create a scratch home with `scripts/new-scratch-home.ps1`.
2. Pick any project whose `.claude.json` has a `githubRepoPaths` array entry with a backslash path.
3. `awt plan` - shows a `json array` edit and exits 0.
4. `awt apply` - exits 3 with `expected 1, live 0`, and auto-rollback restores everything.

---

### Resolution (FIXED 2026-07-28)

Fixed in `crates/awt-core/src/stores/claude_json.rs` by introducing `json_string_literal()`, which
renders a path as a JSON string literal with a real JSON writer (`serde_json::Value::String`)
rather than `format!("\"{value}\"")`. Both the `projects` key anchor and the `githubRepoPaths`
value anchor now match the bytes actually on disk. Forward-slash paths pass through unchanged, so
the majority case is unaffected.

Regression coverage added at `crates/awt-core/tests/claude_json_escaping.rs`, four tests, all
asserting on **raw file bytes**:

- `apply_rewrites_escaped_paths_in_claude_json` - the headline case
- `rewritten_claude_json_still_parses_with_correct_values` - catches a byte splice that produces
  the right bytes but breaks escaping
- `unrelated_entries_are_byte_identical_after_rewrite` - catches over-broad rewriting
- `forward_slash_paths_still_rewrite` - guards the majority case against regression

These wire up `test/fixtures/claude-json-variants/`, closing the orphaned-fixture gap. The seed
helper asserts the fixture still contains the escaped shape, so the coverage cannot silently
lapse if someone reformats the fixture.

One existing golden, `plan::tests::render_plan_locks_format`, had captured the pre-fix rendering
as its expected value and was updated. It is worth noting what that test was doing: it locked in
the *display* of the anchor, which looked natural precisely because it was single-escaped, while
the bytes it stood for could never match. A golden captured from actual output locks in whatever
the code did, correct or not. `render_plan` now shows the true double-escaped anchor, which keeps
the dry run a faithful preview of the splice.

Verified on real data (the same scratch home that failed):

```
awt apply  --home <scratch> --src E:\tmp\chain-smoke --dst E:\tmp\chain-smoke-moved
  applied 6 changes; exit 0            (previously: VerifyFailed "expected 1, live 0", exit 3)
awt verify ...                          all 6 checks [ok], exit 0
githubRepoPaths[acceptance/chain-smoke]  E:\tmp\chain-smoke -> E:\tmp\chain-smoke-moved
awt scan --src E:\tmp\chain-smoke        0 hits
awt rollback --report ...                revert verified: 5/5 byte-identical, exit 0
```

Workspace suite: 114 tests pass (110 before), `fmt` and `clippy` clean.

---

## AR-02: `associate` refuses a project that has no transcripts

**Severity: high, but not a tag blocker on its own.** Distinct from AR-01 and worth its own fix.

```
awt associate --from E:\Projects\prisant-labs\claude-project-mover --to <dst>
error: unrecognized store format: no Claude state found for project '...'; run 'awt list' to see known projects
exit 4
```

`awt scan` reports 4 hits for that same path (a `projects` key, 2 `githubRepoPaths` entries, and
12 `history.jsonl` lines), so state plainly exists. `archive_project` in
`crates/awt-core/src/archive.rs:194` resolves the project through `ProjectIndex.by_cwd`, which is
built only from transcript `cwd` values. A project whose transcripts have expired resolves to
nothing and the whole run aborts, including the re-association half, which does not depend on
transcripts at all.

This inverts the feature's purpose. Per `docs/reference/claude-data-model.md`, `history.jsonl`
never expires while transcripts are auto-deleted after 30 days, so the longer a project has been
dead, the more certain `associate` is to refuse it - and re-associating a long-dead project is the
main reason the command exists.

### Resolution (FIXED 2026-07-28)

Implemented as suggested. `associate` now resolves its target through `doctor::scan`, which
queries every store adapter, instead of the transcript-keyed reverse index. The export step
checks for transcripts first and degrades to a no-op when there are none, rather than aborting a
run whose re-association half is viable. A project with genuinely no state anywhere is still
refused, and there is a test asserting that the widened check did not simply turn the guard off.

A second layer surfaced during the fix: `verify` unconditionally asserted "new projects dir
exists", which can never hold for a project that has no transcripts, so the run then failed at
exit 3 instead of exit 4. The check now distinguishes "nothing to move" from "the move did not
happen" by also looking at the source directory - if neither side exists there were never
transcripts and their absence is the correct postcondition, but if the source survives while the
destination is missing, the move genuinely failed and the check still catches it.

Verified on real data, re-associating this repo's own pre-rename residue:

```
awt associate --from E:\Projects\prisant-labs\claude-project-mover --to <dst>
  associate complete: 3 changes applied; exit 0     (was exit 4, then exit 3)
awt scan --src <old>   0 hits
awt scan --src <new>   4 hits: projects key, 2 githubRepoPaths entries, 12 history lines
```

Tests: `crates/awt-core/tests/associate_and_duplicates.rs`.

---

## AR-03: `--json` is silently ignored by `plan` and `verify`

**Severity: medium for v1, blocking for v2.**

`--json` is a global flag accepted by every subcommand, and
`docs/reference/commands.md` documents it as "Emit machine-readable JSON to stdout instead of
human text". Measured behavior:

| Command | `--json` honored |
|---|---|
| `doctor` | yes |
| `list` | yes |
| `scan` | yes |
| `plan` | **no - prints human text** |
| `verify` | **no - prints human text** |

The flag is accepted, exits 0, and does nothing. A script that pipes `awt plan --json` into a
parser gets human text and fails at the parse, with no signal from the tool that it asked for
something unsupported.

**This blocks v2 specifically.** `docs/ROADMAP.md` Section 1 sets the v2 gate as a parity test
of the form `GUI plan model == awt plan --json`. That contract cannot be written against the
current binary, because the right-hand side does not exist. Implementing it is a prerequisite for
the GUI, not a nicety, and it is entirely independent of the v1.0 tag - it can be built while the
tag is blocked.

Either implement `--json` for `plan` and `verify`, or reject the flag with exit 2 where it is
unsupported. Silently accepting a flag and ignoring it is the one option that is not defensible,
and it is the current behavior.

### Resolution (FIXED 2026-07-28)

Implemented rather than rejected, because the parity contract needs it to exist.

`Plan::to_json()` is new in `crates/awt-core/src/plan.rs`. Every change carries a `kind`
discriminant (`rename_dir`, `merge_dir`, `move_tree`, `rewrite_file`, `rename_json_key`,
`rewrite_json_array_value`) so a consumer can switch on it without parsing prose, and
`rewrite_file` exposes its literal find/replace `rules` so a GUI can offer a byte-level
drill-down. A `totals` object carries both `changes` (plan entries, what a progress bar steps
through) and `edits` (byte replacements, the number a human reasons about) because they are
different numbers and both are wanted.

`verify --json` emits the check list as data plus `failed` and `ok`. The exit-code contract is
independent of the output format: a failed verification is still exit 3 when reporting JSON, and
there is a test asserting exactly that.

Verified on real data:

```
awt plan --json ...    parses; 6 changes; kinds: rename_dir, rewrite_file, rewrite_file,
                       rename_json_key, rewrite_json_array_value, move_tree; totals.edits 101
awt verify --json ...  parses; ok=false, failed=2, 6 checks; exit 3
awt verify --json ...  parses; ok=true,  failed=0;            exit 0
```

**The v2 parity gate is now writable.** `GUI plan model == awt plan --json` can be expressed
against the shipped binary, which was the blocking dependency for starting GUI work.

---

## AR-04: duplicate path values across `githubRepoPaths` slugs

**Severity: release blocker.** Found while re-verifying the AR-01 fix, not during the run itself.

### Symptom

```
awt associate --from E:\Projects\prisant-labs\claude-project-mover --to <dst>
error: verification failed: apply failed
  (VerifyFailed("...\.claude.json: expected 1, live 2")); exit 3
```

### Root cause

`.claude.json` can hold the same path under more than one `githubRepoPaths` slug. On real data
this repo's own pre-rename residue did exactly that:

```
prisant-labs/claude-project-mover   -> ["E:\\Projects\\prisant-labs\\claude-project-mover"]
prisant-labs/agent-workspace-tools  -> ["E:\\Projects\\...\\agent-workspace-tools",
                                        "E:\\Projects\\prisant-labs\\claude-project-mover"]
```

`detect` correctly emits one hit per occurrence, and `plan` turns each hit into its own change
with `expected: 1`. But a change is applied by counting occurrences of its literal across the
**whole file**, so two changes sharing a literal each observe two live occurrences, and the first
one refuses. Every additional slug pointing at the same folder makes the refusal more certain.

### Why it was invisible until AR-01 was fixed

Before the escaping fix the anchor matched nothing at all, so this code path always failed at
`expected 1, live 0` and never got far enough to reveal a count of 2. The two defects were
stacked in the same expression, and fixing the outer one is what exposed the inner one.

### Resolution (FIXED 2026-07-28)

`coalesce_text_splices()` in `crates/awt-core/src/plan.rs` merges changes that share
`(path, from, to)`, summing their expected counts so the count check agrees with the file. Changes
with different literals never merge, so a genuine mismatch - the literal also appearing somewhere
no hit claimed - still refuses, which is the intended fail-closed behavior. The merged change
keeps the position of the first occurrence, because apply ordering is load-bearing elsewhere.

Tests: `crates/awt-core/tests/associate_and_duplicates.rs`, asserting both that the plan
coalesces to a single `expected: 2` change and that apply then rewrites both slugs.

## Observations (not defects)

**Pre-existing corruption in the live `history.jsonl`.** 46 distinct `project` values have had
the drive letter replaced by a colon: `::\Projects\...` instead of `E:\Projects\...`, and
`::\Backup - Data\:choesGPT\...` instead of `E:\Backup - Data\EchoesGPT\...`. Confirmed present in
the live file (not introduced by the copy: SHA-256 matches). `awt doctor` correctly reports these
as stale and correctly refuses to guess at repairing them. Worth a maintainer decision on whether
a future `awt repair --drive-letter` is wanted; it is not v1.0 scope.

**The `doctor` honesty-checkpoint baseline in `AGENTS.md` is stale.** It cites 6 stale
`githubRepoPaths`, 11 stale `history.jsonl` values, and one orphaned plugin dir. Today's run
reports 20 `claude.json`, 48 `claude.history`, 5 `claude.projects`, and 2 `plugin.state`. The
growth is explained by two weeks of project churn plus the repo's own rename, so it is drift, not
a contradiction - but a baseline quoted as a fixed number will keep going stale. It should be
described as a procedure ("re-derive by hand, then compare") rather than a fixed count.

**Cold-cache `doctor` is 98s, not the ~8s quoted in `ROADMAP.md`.** Warm is 5.9s, which matches.
The quoted figure should say "warm; a first run over an uncached tree is materially slower".

**`githubRepoPaths` values of the wrong JSON type are ignored silently.** A string where an array
is expected is skipped rather than raising exit 4. Defensible for a report-only field, and it is
what let a malformed synthetic test entry pass unnoticed during this run. Worth a deliberate
decision rather than leaving it accidental.

## Cleanup

The scratch home, backups, and archive were retained pending the AR-01 fix so the run can be
repeated against the same data. Delete with:

```
Remove-Item -Recurse -Force "E:\Projects\_temp\awt-acceptance-2026-07-28",
  "E:\Projects\_temp\awt-acceptance-2026-07-28-backups",
  "E:\Projects\_temp\awt-acceptance-2026-07-28-archive"
```
