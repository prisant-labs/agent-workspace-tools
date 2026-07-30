---
id: S-04
title: "v1 safety closeout - close the data-loss and false-success paths before the tag"
type: spec
status: committed
created: 2026-07-30
updated: 2026-07-30
target-release: v1.0.0
linked-release: ../plan_v1.0.0.md
linked-plan: ./implementation-plan.md
ac-count: 12
ac-range: AC-54..AC-65
requires-human-review: false
origin: "External adversarial code audit, 2026-07-30 (report local-only per repo convention); every blocking finding independently verified against source before this spec was written"
---

# S-04: v1 safety closeout

## Why this effort exists

The 2026-07-30 acceptance run passed all 15 steps on real data, and that evidence is real - but
it exercised the happy path. An adversarial audit the same day found paths where the tool can
**lose data or report success falsely**, none of which the acceptance sequence probes. The
blocking findings were independently re-verified against the source before this spec was
written; the file-and-line evidence below is from that verification, not taken on faith.

This is the difference between "the exercised path works" and "the safety claim holds." The
product's one promise is that it never loses data and never lies about what it did. These
findings are therefore tag blockers, and S-01's sign-off should happen against the closed state,
not the current one.

Severity language: **Critical** = credible data loss, destructive overwrite, or false success in
a normal path. **High** = a hard invariant, security boundary, or advertised capability is
materially incorrect.

## Acceptance Criteria

### Critical

- **AC-54 (rollback restores the complete tree).** Given a project-state directory containing
  sidecars beyond top-level `*.jsonl` (nested `memory/*.md`, tool results, binary files), when an
  apply is rolled back (auto or manual), then every file that existed before the apply exists
  afterwards with byte-identical content, proven by a full pre/post tree hash comparison.
  *Verified defect:* `backup.rs` snapshots only immediate `*.jsonl` children for a
  `RenameDir`, while `rollback.rs` deletes the renamed directory recursively - unbacked sidecars
  are destroyed by the undo. *Fix direction:* roll the directory back by rename (the bytes are
  still on disk under the new name), restoring modified files after; do not reconstruct from a
  partial snapshot. Regression tests must include sidecars with and without old-path references,
  nested binary files, and injected apply and verify failures.

- **AC-55 (a missing source cannot succeed).** Given a `--src` that does not exist as a
  directory, when `plan` runs, then it refuses (exit 2) before any plan is produced; if the
  source vanishes between plan and apply, then apply hard-fails rather than skipping the move;
  and after any folder-moving apply, verify asserts both source-absent and destination-present.
  *Verified defect:* `build_plan` has no source-exists guard, and `apply` wraps `MoveTree` in
  `if fs.exists(from)` - a missing source is silently skipped and still recorded as applied, so
  Claude state is rewritten toward a destination no folder occupies, exit 0.

- **AC-56 (settings writes fail closed).** Given a `settings.json` that is unreadable, not valid
  UTF-8, not valid JSON, or whose root is not an object, when any settings-writing operation
  runs (`--set-retention`, `--install-hook`, `--uninstall-hook`), then it refuses with exit 4
  and the file is byte-identical afterwards. Only file-not-found may initialize a fresh
  settings object. Writes are atomic and preserve every unrelated key.
  *Verified defect:* `load_settings` converts every read or parse failure into an empty object,
  which the subsequent save writes over the user's file.

### High

- **AC-57 (verify derives from the plan).** Verification is constructed from the plan it checks:
  it runs at the applied scope (not hardcoded Standard), asserts destination-key presence in
  `claude.json` (not only old-key absence), asserts the folder postcondition for a folder move,
  treats a malformed `history.jsonl` line as a failure rather than skipping it, and treats a
  read error as a failure rather than absence. A verify that cannot read what it must check
  never reports green.

- **AC-58 (advertised-but-inert options are removed or implemented).** `--on-collision
  keep-dest`/`keep-src` (parsed, never consumed - selecting one silently bypasses the collision
  guard), `--recursive` (suppresses the nested-project warning without moving anything), and the
  `minimal`/`full` scope tiers (minimal cannot pass Standard verification; full rewrites files
  that verification and backup do not cover) each either acquire a full spec, implementation,
  and tests, or are removed from the v1 surface. **Default for this effort: remove them.** A
  safety-affecting flag that does nothing is worse than a smaller honest surface.

- **AC-59 (I/O failure is never absence).** In planning, backup, apply, and verify paths, a
  failed directory read or file read is an error, not an empty result. Missing *optional* roots
  (for example, no `plugins` directory) remain valid empty states. Proven with a
  failure-injecting `FileSystem` test double covering each mutation stage.

- **AC-60 (plugin hash uses the recorded spelling).** The plugin-state directory hash is derived
  from the `cwd` spelling recorded in transcripts (via the reverse index), never from the
  caller-typed `--src`. A move invoked with a case or separator variant of the recorded path
  still finds and renames the plugin state dir. *Verified defect:* the hash is computed from
  `mv.src_abs`, so a `d:/cloud-work-pp` invocation misses state recorded under
  `D:\Cloud-Work-PP`, and verify repeats the same wrong derivation and passes.

- **AC-61 (path confinement is semantic, not lexical).** The archive hook's transcript-path
  check canonicalizes before comparing (a `..` under the accepted prefix must not escape), and
  recursive walkers used by archive and full-scope operations have an explicit reparse-point
  policy - the v1 default is to refuse junctions and symlinks inside trees being archived or
  mutated. Windows junction tests included.

- **AC-62 (fixture publication response).** The four real session transcripts currently
  committed under `test/fixtures/` (18.1 MB, unredacted by policy, in a public repository) are
  replaced by synthetic or minimized fixtures that preserve only the shapes and counts the tests
  require, and the golden counts are re-locked against the replacements. **The response
  decision is the maintainer's** (see D10 in the release plan): whether history rewrite or
  removal-going-forward is sufficient, and whether anything in the exposed content needs
  rotation or notification. The engineering half (synthetic fixtures) proceeds regardless of
  that decision.

### Completed in this effort's opening commit

- **AC-53a (repair refuses invalid UTF-8).** Recorded in S-03's spec as an amendment; the fix
  and regression test landed with this spec. `build_repair_plan` now hard-fails on invalid
  UTF-8 (exit 4) instead of planning against a lossy decode.

### Scheduled but non-blocking (Medium)

Tracked here so they are not lost; they do not gate the tag:

- **AC-63 (transaction identity).** Run IDs are second-resolution timestamps, so two applies in
  one second share a backup directory; rollback errors after a failed apply are discarded;
  rollback panics on a malformed manifest. Unique run IDs, typed manifest parsing, and dual
  error reporting.
- **AC-64 (archive scope honesty).** Archive copies transcripts and file-history but not the
  other session-keyed artifacts inventory links (todos, session-env, tasks); a malformed
  existing archive manifest is silently replaced. Either widen the copy or state the exclusions
  in the output; hard-fail on a malformed manifest.
- **AC-65 (deterministic external output).** `list` and nested-project output iterate HashMaps,
  so ordering is not a stable contract; sort every externally visible collection. Fold into the
  v2 core-contract work if not done here.

## Out of scope

- The v2 typed core contract (schema versions, operation IDs, sensitive-field separation). That
  is v2 prework and follows this effort; it must not be squeezed into the tag path.
- Branch protection on `main` - recommended to the maintainer as repo configuration, not code.
- Cross-volume moves, other-CLI adapters, and everything already parked.

## Evidence discipline

Every AC lands with a test that fails before its fix and asserts on raw bytes or full tree
state, per the conventions in `AGENTS.md`. The final gate is a revised acceptance run whose
matrix includes adversarial cases (missing source, sidecar-bearing project, malformed settings,
junction in tree), not a larger happy-path copy.
