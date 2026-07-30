---
id: S-03
title: "awt repair --drive-letter - recover history entries with a corrupted drive prefix"
type: spec
status: committed
created: 2026-07-30
updated: 2026-07-30
target-release: v1.0.0
linked-release: ../plan_v1.0.0.md
linked-plan: ./implementation-plan.md
ac-count: 10
ac-range: AC-45..AC-53a
requires-human-review: false
origin: "Acceptance run 2026-07-28 (observation: history.jsonl drive-letter corruption); decision D6, maintainer-approved 2026-07-30"
---

# S-03: `awt repair --drive-letter`

## Problem

`~/.claude/history.jsonl` on the maintainer's machine contains 46 distinct `project` values,
spanning **3,121 lines**, whose drive letter has been replaced by a colon:

```
::\Projects\adobe-cclib-liberator          should be  E:\Projects\adobe-cclib-liberator
::\Backup - Data\:choesGPT\backup\...      should be  E:\Backup - Data\EchoesGPT\backup\...
```

Every capital `E` became `:`. The affected lines are unreachable: Claude Code cannot match them to
any project, so that prompt history is effectively lost while still occupying the file.

`awt doctor` already reports these as stale and refuses to repair them, which is v1.0 behaving
correctly. This spec closes the gap between "will not guess" and "cannot help".

## Why this is not guessing

Measured on the live file, 2026-07-30:

| Outcome when the leading `::` is replaced by each present drive letter | Count |
|---|---|
| Resolves on **exactly one** drive | **34** |
| Resolves on **no** drive | 12 |
| Resolves on **two or more** drives | **0** |

Two of those 46 differ only in case (`::\projects\...` and `::\Projects\...` for the same
project). They are counted, and repaired, separately: the rewrite is a case-sensitive byte splice,
so each is a distinct literal requiring its own rule. A case-insensitive count would report 45 and
33 and would be wrong about the work to be done.

Every repairable value has exactly one answer, and the ambiguous case does not occur. A rule of
*repair only when exactly one candidate resolves* is therefore deterministic on real data, and
fails closed by construction if that ever stops being true. 2,303 of the 3,121 damaged lines are
recoverable under this rule.

The 12 that resolve nowhere include the one value with a second corrupted segment
(`:choesGPT`), which a drive-prefix rule cannot fix. Those are reported, never touched.

## Scope

**In scope.** The `claude.history` store only. The corruption was measured to be confined to
`history.jsonl`: zero `::` values in `.claude.json` `projects` keys, zero in `githubRepoPaths`,
zero malformed project directories.

**Out of scope.** Any transformation other than the leading drive prefix. Repairing
`:choesGPT`-style damage inside a path would require inferring content, not a drive, and is
explicitly not attempted (see D8).

## Definitions

**Malformed drive prefix.** A `project` value beginning with the two characters `::`. Narrow by
design: this is exactly the observed corruption, and the narrowness is what makes the guard
provable. A value that merely fails to resolve is *stale*, not damaged, and is not in scope.

**Candidate.** A drive letter `X` such that replacing the leading `::` with `X:` yields a path
that exists as a directory on disk.

**Repairable.** Exactly one candidate. Zero or more than one is not repairable.

## Acceptance Criteria

| AC | Criterion |
|----|-----------|
| AC-45 | `awt repair --drive-letter` identifies every distinct `history.jsonl` `project` value with a malformed drive prefix, and reports each with its line count |
| AC-46 | A value is proposed for repair **only** when exactly one drive letter yields an existing directory. Zero candidates is reported as unrepairable and left untouched; two or more is refused as ambiguous and left untouched |
| AC-47 | The command is a **dry run by default**: without `--apply` it writes nothing, and the file is byte-identical afterwards. `--apply` performs the repair |
| AC-48 | `--apply` takes a sha256 backup of `history.jsonl` before the first write, and every replacement is boundary-anchored and count-checked; a live count differing from the plan refuses the write |
| AC-49 | After `--apply`, verification re-reads from disk and confirms zero malformed values remain for the repaired paths, and that the line count of the file is unchanged |
| AC-50 | A failed verification triggers automatic rollback, and the run is recoverable by `awt rollback --report <manifest>` like any other write |
| AC-51 | Repair is idempotent: re-running `--apply` on a repaired file proposes no changes and exits 0 |
| AC-52 | Only `history.jsonl` is written. `claude.json`, transcripts, and plugin state are untouched, and unrelated lines in `history.jsonl` are byte-identical |
| AC-53 | `--json` emits the proposed and applied repairs as machine-readable data, including the unrepairable and ambiguous sets, so a caller can see what was declined and why |
| AC-53a | A `history.jsonl` that is not valid UTF-8 is refused (exit 4) before any plan is produced, and the file is untouched. Invalid UTF-8 is a different corruption class than a drive-letter substitution; planning against a lossy decode would compute counts for a file that does not exist and violate the never-lossy-rewrite invariant. Added 2026-07-30 with the safety closeout |

## Exit codes

Consistent with the existing contract: `0` success (including a dry run that found nothing),
`1` I/O error, `3` verification failed after write, and `4` unrecognized format - specifically a
`history.jsonl` that is not valid UTF-8 (AC-53a). A value that cannot be repaired is reported,
not an error, because declining to repair is correct behavior rather than a failure.

## Non-goals

- Inferring a drive when zero or several candidates exist.
- Repairing corruption anywhere other than the leading drive prefix.
- Repairing stores other than `history.jsonl`.
- Diagnosing the cause of the corruption (tracked as D7).
