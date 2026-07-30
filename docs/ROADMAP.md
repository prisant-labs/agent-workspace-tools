# awt Roadmap - program plan across v1 and v2

Status: active. Created 2026-07-11. Owner: maintainer (jprisant).
Scope: this is the program-level plan spanning releases. Version-scoped execution
detail lives in `docs/internal/release-plans/plan_vX.Y.Z/` (jp-release-plan
convention); the validated design lives in `docs/DESIGN.md`; the executable TDD plan
lives in `docs/superpowers/plans/2026-07-10-claude-project-mover.md`.

## 1. Version map

| Version | Contents | Gate to ship |
|---|---|---|
| v0.1.0 (milestone tag) | Read-only CLI: `doctor` + `scan` (TDD plan phases 1-4) | The honesty checkpoint: `awt doctor` on the real machine reports exactly the residue verified by hand (6 stale githubRepoPaths, ~11 stale history values, the orphaned plugin dir). No write code ships before this passes. |
| v1.0.0 | The complete Windows-native CLI: mover (`plan`/`apply`/`verify`/`rollback`, phases 5-9) plus F13 `awt list`, F14 `awt archive`, F15 `awt associate` (phases 13-15) | Release plan hygiene gates + doc checklist at `docs/internal/release-plans/plan_v1.0.0/` |
| v1.1.0 | Repair: `awt repair --drive-letter` (S-03, AC-45..AC-53), plus the `doctor` warnings channel | Plan at `docs/internal/release-plans/plan_v1.1.0/`. Ships after v1.0.0; does not gate that tag |
| v1.x (parked candidates) | P10 cross-volume move (AC-2); P11 Codex/Gemini adapters (AC-27) | Promotion into a release plan when scheduled; both sit behind the existing adapter boundary and copy primitives, no re-architecture needed |
| v2.0.0 | Tauri 2 + React GUI over the identical `awt-core` for every v1 capability (old phase 12; AC-25 parity) | GUI security baseline and native-parity requirements written into DESIGN.md BEFORE build starts; parity test `GUI plan model == awt plan --json`; signing + updater pipeline |

Scope decision of record: the old numbering ("v1.0 mover" + "v1.1 features") folds
into a single v1.0.0, because v1 is defined as everything the CLI does. Old phases
10-11 are defined-in-CLI but deliberately deferred; they park as promotable v1.x
backlog rather than silently disappearing. The GUI is v2.0.0.

## 2. Workstream: code

Execution follows the TDD plan task-by-task (subagent-driven, red-green per step).
Order and dependencies:

```
Phase 1 scaffold + FileSystem + sanitized fixtures + CI gates
Phase 2 encoding + reverse index          Phase 3 adapter read paths
Phase 4 doctor/scan  ==> HONESTY CHECKPOINT ==> tag v0.1.0
Phase 5 rewrite engine   Phase 6 plan + guards
Phase 7 backup/apply     Phase 8 verify + auto-rollback   Phase 9 rollback + CLI
Phase 13 list (F13)      Phase 14 archive (F14)           Phase 15 associate (F15)
==> v1.0.0 gates ==> tag v1.0.0
```

F13 and F14 depend only on phases 1-4 plus phase-7 copy primitives, so they may be
built immediately after the doctor milestone if retention priorities demand it;
F15 requires the full mover write path.

Pre-execution gate (done 2026-07-11): the audit-repair pass. The 2026-07-11 audit
(report local-only at `_local/audit/2026-07-10_fable-audit/`; summary of applied
fixes in `docs/CHANGELOG.md`) found 46 verified findings, 4 High; all plan-text
repairs were applied before Phase 1 execution so that no audited defect gets
transcribed into code.

## 3. Workstream: CI

Progressive hardening; each stage lands with the phase that makes it meaningful.

| Stage | Lands with | Jobs |
|---|---|---|
| CI-0 | Phase 1 Task 1.1 | windows-latest: `cargo fmt --check`, `clippy -D warnings`, `cargo test --workspace`, dependency-hygiene gate (awt-core must not depend on tauri/clap), no-network gate (no reqwest/ureq/hyper/curl in tree), `cargo audit` |
| CI-1 | Phase 1 Task 1.3 | Golden-fixture tests run in CI (sanitized fixtures committed; counts locked) |
| CI-2 | macOS scope opens (post-v1.0.0 unless promoted) | macos-latest matrix entry + POSIX-path encoding/normalization test cases; same_volume device-id fix; iCloud-path cloud-sync detection |
| CI-3 | First binary distribution (may follow the v1.0.0 tag) | Release workflow: matrix build, minisign checksums, winget manifest, code signing or a documented SmartScreen/Gatekeeper posture; release runbook executed, not improvised |
| CI-4 | v2.0.0 | tauri-specta stale-bindings gate, Tauri bundle matrix (.msi/NSIS, .dmg/.app), updater with signature verification, notarization for macOS |

Note: v1.0.0 itself may ship source-first (`cargo install` / clone-and-build), which
requires no signing; CI-3 gates the binary channel, not the tag.

## 4. Workstream: documentation

Three audiences, one map. Every doc gets a row in `docs/index.md` and a
doc-impact entry in `docs/CHANGELOG.md` when it changes.

| Artifact | Audience | Exists? | Lands |
|---|---|---|---|
| README.md (orientation, plain language) | Non-engineers + everyone | Rebuilt 2026-07-11 | now |
| docs/index.md (doc map) | Everyone | Created 2026-07-11 | now |
| AGENTS.md (operating manual: invariants, conventions, environment) | Agents | Created 2026-07-11 | now |
| CLAUDE.md (thin Claude overlay) | Agents | Created 2026-07-11 | now |
| LICENSE (MIT) + .gitignore | Everyone | Created 2026-07-11 | now |
| docs/DESIGN.md (validated design) | Engineers | Yes (audit-repaired) | exists |
| TDD plan (executable, per-task) | Engineers + agents | Yes (audit-repaired) | exists |
| docs/features/v1.1-...md (F13-F15 spec, AC-28..44) | Engineers | Yes | exists |
| Release plan + committed specs (S-01 carve of AC-1..24,26; S-02 pointer) | Maintainers | Created 2026-07-11 | now |
| docs/ROADMAP.md (this file) | Maintainers | Created 2026-07-11 | now |
| docs/quickstart.md (build + first run against a COPY of ~/.claude) | Engineers, then everyone | No | Phase 4 (doctor milestone) |
| docs/troubleshooting.md (real error messages, exit codes 1/2/3/4) | Everyone | No | Phase 4, grows through v1 |
| docs/reference/commands.md (per-subcommand reference, flags, exit codes) | Engineers | No (DESIGN Section 7 serves) | v1.0.0 tag prep |
| CONTRIBUTING.md (test/format/clippy commands, commit trailers) | Engineers | No | first external contribution or v1.0.0, whichever first |
| docs/release-runbook.md (tag ceremony, checklist, CI-3 steps) | Maintainers | No | before first tag (v0.1.0 in lightweight form) |
| Root CHANGELOG.md (keepachangelog, user-facing releases) | Everyone | No (docs/CHANGELOG.md is doc-impact only) | v0.1.0 |
| docs/security-model.md (standalone; capability rationale per window) | Engineers | No (DESIGN Section 6 serves for CLI) | v2.0.0 pre-work |
| GUI quickstarts per OS (screenshots, installer paths) | Non-engineers | No | v2.0.0 |

## 5. v2.0.0 outline (GUI)

Prerequisites to write into DESIGN.md BEFORE v2 build starts (both were audit
findings): a security baseline (Tauri capability scoping per window, CSP, typed IPC
allowlist via tauri-specta, minisign-pinned updater, stale-bindings CI gate) and a
native-parity baseline (Cmd/Ctrl shortcut policy, window chrome, system theme,
high-DPI, native dialogs). The GUI calls the identical `awt-core`; the CLI parity
test (plan --json equivalence) is the correctness spine. Model repo: repo-sync-tool
(Rust + Tauri 2 + React 19 + tauri-specta).

Create `docs/internal/release-plans/plan_v2.0.0/` via `/jp-release-plan --create
v2.0.0` when v1.0.0 gates pass; carve the GUI spec (AC-25 plus the two baselines)
as its first effort.

## 6. Governance

- Specs own acceptance criteria; release plans only aggregate (jp-release-plan
  convention). The v1 spec set: S-01 (mover, AC-1, AC-3..24, AC-26) and S-02
  (pointer to the committed F13-F15 spec, AC-28..44). Deferred AC: AC-2 (P10),
  AC-25 (v2), AC-27 (P11).
- The doctor honesty checkpoint is a hard gate: no phase-5+ write code merges
  before it passes on the real machine.
- Fixtures are sanitized once (plan Task 1.3 sanitization step) and never refreshed
  from live files without repeating that step.
- Every doc change lands with a `docs/CHANGELOG.md` entry; session logs go to
  `_local/_session-logs/` (gitignored, local-only - not part of the repo).

## 7. Current status (2026-07-28)

**v1.0 is feature-complete but NOT releasable:** the first manual acceptance run found a
release-blocking defect (AR-01, below). All engine phases (1-9) plus the retention
features (13-15) are implemented, committed, and pushed; `main` is level with origin. 110
tests pass, clippy and fmt clean.

- Phases 1-4: scaffold + FileSystem trait + reverse index + six adapter read paths +
  `doctor`/`scan` and the `awt` CLI. This is the v0.1.0 read-only milestone; the doctor
  honesty checkpoint passed on the real machine (reports the hand-verified residue, leaves
  live state alone).
- Phase 5: anchored rewrite engine (count-checked, byte-preserving). The golden test
  reproduces the reference-move counts (1467 + 588 + 27 = 2082) while proving package and
  branch mentions come through byte-identical.
- Phases 6-9: plan + guards, backup + transactional apply, verify + auto-rollback + lock
  detection + hard-fail, and the full CLI (`plan`/`apply`/`verify`/`rollback`).
- F13-F15: `awt list` inventory (health flags + 30-day-cliff ages), `awt archive`
  (content-hash incremental + cumulative manifest + SessionEnd retention hook), `awt
  associate` (re-associate and/or from-scoped export, working on a gone source folder).

**Sweep scope resolved.** The sweep skips vendored and archival regions (`plugins/`,
`file-history/`, `backups/`) where an old path is correct by design, cutting `doctor` from
~345s to ~8s warm; sweep results live in `DoctorReport.report_only`, structurally out of the
rewrite path. Re-measured 2026-07-28 on a 3.30 GB / 55,413-file home: 5.9s warm, but 98s on a
first run over an uncached tree. Quote the warm figure only with that caveat attached.

**Adversarial review pass.** Per-feature Codex adversarial reviews were run; they surfaced
and drove fixes for fail-silent read errors in `list`/`ProjectIndex` (now fail loud on a real
I/O error while a missing projects dir stays a valid empty result) and for the F14 archive
manifest (now cumulative + atomic) and SessionEnd hook (now reads the hook JSON from stdin,
which is the real Claude Code hook contract).

**Since 2026-07-20 (merged to `main`, CI green on windows-latest, PRs #1 and #2):** an
AC-to-test traceability review closed five acceptance criteria the build detected but did not
enforce - cross-volume guard, lock detection, ambiguous-history fail-close, machine-readable
report, and a zero-network guard test - added a verifiable revert (post-rollback byte-identity
proof), and amended AC-7 (fail-closed, 7a) and AC-19 (idempotent-by-refusal, 19b). A
release-hygiene pass then added `docs/quickstart.md`, `docs/troubleshooting.md`, and the root
`CHANGELOG.md`, bumped `Cargo.toml` to 1.0.0, and gave `AwtError` a `Display` impl so guard
errors surface their plain-language message. Evidence:
`docs/internal/release-plans/plan_v1.0.0/S-01_mover-cli/ac-traceability.md`.

**The acceptance run happened on 2026-07-28 and FAILED.** Full report:
`docs/internal/release-plans/plan_v1.0.0/acceptance-run-2026-07-28.md`. Eleven of thirteen steps
passed, including the complete plan/apply/verify/rollback cycle with byte-identical revert proof,
and `archive` over the full live corpus. Two defects block the tag:

- **AR-01 (release blocker) - FIXED 2026-07-28. `claude.json` `githubRepoPaths` rewrite failed on JSON escaping.**
  The planner anchors on the parsed, unescaped path (`E:\a\b`) while the file stores it escaped
  (`E:\\a\\b`), so the count check finds 0 where it planned 1. `apply` and `associate` both fail
  closed with exit 3 for any project that has a `githubRepoPaths` entry - the normal case for a
  cloned repo on Windows. Three coverage gaps hid it: the golden end-to-end fixture has an empty
  `githubRepoPaths`, the `claude-json-variants` fixture that contains the triggering shape is
  referenced by no test at all, and `plan`-level assertions see the unescaped value and pass.
- **AR-02 - FIXED 2026-07-28. `associate` refused a project whose transcripts had expired.** It resolves targets
  through the transcript-keyed reverse index, so a project with `history.jsonl` and `claude.json`
  state but no surviving transcripts is reported as having no state. Since transcripts expire at
  30 days and `history.jsonl` never does, this refuses precisely the cases the command exists for.
- **AR-03 - FIXED 2026-07-28. `--json` was silently ignored by `plan` and `verify`.** Implemented for `doctor`,
  `list`, and `scan`; accepted and ignored elsewhere. **This one blocks v2, not v1:** the GUI
  parity gate in Section 1 is `GUI plan model == awt plan --json`, and that contract cannot be
  written against the current binary. Implementing it is unblocked work that can proceed in
  parallel with the v1.0 fix.

The safety design was vindicated by both failures: every refusal was fail-closed, auto-rollback
fired, and every restored file was proven byte-identical. Nothing was lost.

A fourth defect, **AR-04**, surfaced only after AR-01 was fixed: two `githubRepoPaths` slugs can
hold the same path value, and each occurrence planned its own edit expecting one match while each
edit counts across the whole file and saw two. The two were stacked in one expression, which is
why the acceptance run saw a single symptom. All four are now fixed and regression-tested.

**Remaining before the v1.0 tag:** (1) re-run the acceptance run end to end and get a clean pass;
(2) the maintainer S-01 spec sign-off, evidence-backed by the traceability doc.

**v1.1.0 is open and implemented (2026-07-30).** The three decisions left open after the
acceptance run were all resolved as recommended. `awt repair --drive-letter` recovers history
entries whose drive letter was corrupted, repairing only where exactly one drive resolves and
naming everything it declines (2,303 of 3,121 damaged lines on the machine where this was found).
`doctor` gained a warnings channel for shapes an adapter recognizes and deliberately skips, the
first being a wrong-typed `githubRepoPaths` value, which was previously silent. Dependabot was
added, monthly. Plan: `docs/internal/release-plans/plan_v1.1.0/`. None of this gates the v1.0.0
tag. The `cpm` ->
`awt` rename landed 2026-07-24 (ADR-0001). The signing / SmartScreen posture is CI-3 (the binary
channel), not a tag blocker - v1.0 may ship source-first.

**Next major work: v2.0.0** - the Tauri + React GUI ("Taura") over the identical `awt-core`;
conceptual mockups exist. Open maintainer decisions remain tracked in the release plan's
Decisions section (tag ceremony; one-time manual archive copy destination; P10/P11 timing).
