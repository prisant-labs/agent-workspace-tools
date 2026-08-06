---
title: "awt-core contract survey: what a freeze would actually have to cover"
type: survey
created: 2026-08-06
status: prework
feeds: docs/internal/release-plans/plan_v2.0.0/ (not yet created; ROADMAP section 5 gates its creation on v1.0.0)
---

# awt-core contract survey

The standing directive is to **freeze a versioned `awt-core` contract before any `src-tauri`
work**. This document is the input to that: a survey of what the machine-readable surface
looks like today, so the contract spec is written against evidence rather than intent.

It changes no code and moves no gate. The v2 release plan is created after v1.0.0 gates pass,
per [ROADMAP](../ROADMAP.md) section 5; this is the reading that makes its first effort
writable.

Companion document: [v2 GUI design brief](v2-gui-design-brief.md), which covers product and
interaction. This one covers only the data contract between `awt-core` and any front end.

## Why freeze at all

[ROADMAP](../ROADMAP.md) states the parity rule as **`GUI plan model == awt plan --json`**
(AC-25). Parity is only meaningful against something stable. Today the JSON was grown one
command at a time, and four of the defects found between 2026-07-28 and 2026-07-31 were
commands emitting prose where they advertised JSON (AR-03, AR-07, and the two found during the
Codex review response). Those are fixed, but they are symptoms of a surface that was never
designed as one object. Building a GUI against it would freeze the accidents along with the
intent.

## Method

Every emission site was read directly: `serde_json::json!`, `to_json()`, and
`to_string_pretty` call sites across `crates/awt-cli/src/main.rs` and `crates/awt-core/src/`.
Thirteen command modes emit JSON. Findings below cite file and line.

## The surface today

| Mode | Shape is built in | Serialization | Top level |
|---|---|---|---|
| `plan --json` | `awt-core` (`plan.rs:51`) | pretty | object |
| `apply --json` | `awt-core` (`report.rs:23`) | pretty | object |
| `repair --json` | `awt-core` (`repair.rs:75`) | pretty | object |
| `verify --json` | **`awt-cli`** (`main.rs:385`) | pretty | object |
| `doctor --json` | **`awt-cli`** (`main.rs:167`) | compact | object |
| `scan --json` | **`awt-cli`** (`main.rs:210`) | compact | object |
| `list --json` | **`awt-cli`** (`main.rs:293`) | compact | **array** |
| `rollback` report | **`awt-cli`** (`main.rs:500`) | pretty | object |
| `archive --json` | **`awt-cli`** (`main.rs:691`) | compact | object |
| `associate --json` | **`awt-cli`** (`main.rs:552`) | compact | object |
| `archive --install-hook --json` | **`awt-cli`** (`main.rs:592`) | compact | object |
| `archive --uninstall-hook --json` | **`awt-cli`** (`main.rs:605`) | compact | object |
| `archive --set-retention --json` | **`awt-cli`** (`main.rs:614`) | compact | object |

## Findings

### F1. The contract mostly does not live in `awt-core` (structural, the headline)

Only three of thirteen shapes are built in the core crate. The other ten are assembled inline
at their call site in `crates/awt-cli/src/main.rs`. A GUI linking `awt-core` directly, which is
the entire premise of "one shared model, two front ends", **cannot reach those shapes at all**
without reimplementing them and hoping they match.

This is the finding that makes the others secondary: you cannot freeze a contract that is not
in the crate being frozen. Everything below is detail about what to move.

### F2. No payload carries a version

Zero of the thirteen emit a schema version. A consumer has no way to detect a shape change; it
can only crash or, worse, silently misread. This is the minimum viable content of a freeze.

### F3. `list --json` emits a bare top-level array

`main.rs:310` prints `serde_json::json!(arr)` directly. Every other mode emits an object.

A top-level array is terminal: it can never gain a sibling field, so `list` alone could never
carry a version, totals, or warnings without a breaking change. Of everything in this document
this is the one shape that must change before a freeze rather than after.

### F4. Two serialization conventions

`plan`, `apply`, `verify`, `repair`, and the rollback report use `to_string_pretty`. `doctor`,
`scan`, `list`, `archive`, `associate`, and the three settings modes print the value's `Display`
form, which is compact single-line. So `awt plan --json` and `awt list --json` disagree on
formatting for no reason a consumer can predict.

### F5. No uniform success signal

- `verify` has top-level `"ok"` and `"failed"` (`main.rs:385`)
- the rollback report has per-check `ok` and no top-level verdict (`main.rs:500`)
- `archive` emits `{"copied","skipped"}` with no status at all (`main.rs:691`)
- the settings modes emit bespoke booleans: `{"hook_installed":true}`,
  `{"hook_removed":true}`, `{"cleanup_period_days":N}`

A consumer cannot ask "did this succeed" generically; it must know each command's shape. Exit
codes carry the verdict correctly and are well specified, but a GUI reading a payload should
not have to consult a second channel to interpret it.

### F6. The same concept has different names

The absolute project path appears as `"src"` (plan, verify, scan), `"cwd"` (list), `"path"`
(repair), and `"from"`/`"to"` (repair entries, associate). Each is locally sensible; together
they force a front end to write a translation layer, which is exactly the layer parity is
supposed to make unnecessary.

### F7. `totals` exists in two shapes and is absent from most

`plan` has `{"changes","edits"}`; `repair` has `{"repairable_values","repairable_lines",
"unrepairable_values","ambiguous_values"}`. `doctor`, `scan`, and `list` have none, so a
consumer counts array lengths and hopes that is what the tool meant.

### F8. English prose is embedded as data

`repair.rs:83` and `repair.rs:88` emit:

```json
"reason": "no present drive makes this path resolve"
"reason": "more than one drive would resolve; refusing rather than guessing"
```

These are human sentences inside the machine contract. A front end that wants to render its own
wording, group by cause, or localize has to string-match English. The fix is a stable
machine `code` plus an optional human `message`, which also makes the prose editable without a
contract break.

### F9. Path serialization is inconsistent

Everything uses `to_string_lossy()`, and separator conventions vary: some sites normalize with
`.replace('\\', "/")` before emitting (for example the export location at `main.rs:546`) while
others emit native backslashes. A consumer receives a mix and must normalize defensively.

Note this is genuinely subtle here rather than mere sloppiness: this tool's whole domain is
that Claude Code stores paths in several spellings, and AC-60 exists because the *recorded*
spelling matters. The contract should therefore be explicit about which spelling each field
carries, not merely consistent.

### F10. Only two shapes are documented

`plan.rs` and `report.rs` carry doc comments specifying their JSON. The other eleven are
defined implicitly by their call site, which means the specification is the code and any change
is undetectable by review.

## What a freeze should mean

Offered as the starting proposal for the v2 spec, not as a decision:

1. **Move every shape into `awt-core`** as a typed model with its own `to_json`. This is F1 and
   is prerequisite to all of the rest.
2. **One envelope for every mode**, carrying the version, the verdict, and the payload. For
   example `{"schema": 1, "command": "plan", "ok": true, "data": {...}, "warnings": [...]}`.
   This resolves F2, F4, F5 and makes F3 a non-issue by construction.
3. **A stated compatibility rule**: additive-only within a major version, and the version
   increments when a field is removed or retyped. Without a written rule the version number is
   decoration.
4. **Machine codes instead of prose** for anything a consumer might branch on (F8).
5. **One documented path convention per field**, saying which spelling it carries (F9).
6. **Generalize the parity test.** AC-25 currently pins `GUI plan model == awt plan --json`.
   Once the contract is uniform, the same test should exist per command rather than for `plan`
   alone; `plan` was simply the first shape to get a typed model.

## Constraints any proposal must respect

- **`awt-core` has no serde-derive.** Its dependencies are `serde_json`, `sha2`, `dunce`,
  `walkdir`, `tempfile`. Shapes are hand-built. A typed-model approach must either keep
  hand-written `to_json` or make an explicit, justified case for adding `serde` with `derive`,
  which touches the dependency-hygiene gate that CI enforces.
- **No LLM, no network** remains structural and is enforced by `no_network_deps`.
- **Exit codes are already a specified contract** and must not be duplicated or contradicted by
  the `ok` field; the envelope should agree with the exit code, never replace it.

## What this does not do

No code changes, no gate movement, and no v2 release plan. v1.0.0 is unaffected: every finding
above describes shipping behavior that works and is tested, and none of it is a defect against
any v1 acceptance criterion. The contract work is deliberately sequenced after the tag, because
changing the JSON now would break the "the tagged commit is the acceptance-tested commit"
property for no benefit that cannot wait.
