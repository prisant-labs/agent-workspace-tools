---
title: "Maintainer to-do - the single human checklist"
type: checklist
updated: 2026-07-28
---

# Maintainer to-do

**This file is the canonical human to-do list for this repo.** If you only open one document to
answer "what is left for me to do", open this one.

Before 2026-07-28 the answer was scattered across four places - the release plan's hygiene gates,
the release plan's doc checklist, the runbook's pre-tag gates, and the roadmap's status section -
which meant no single place could tell you whether you were done. Those documents remain the
detailed source of truth for their own areas; this one aggregates only the items that need
**you**, a human, and says which are yours alone.

Legend: **[HUMAN]** only you can do this. **[DELEGABLE]** an agent or contributor can do it, you
review.

---

## 1. Blocking the v1.0.0 tag

### 1.1 Fix AR-01 (githubRepoPaths JSON escaping) - **DONE 2026-07-28**

Fixed in `crates/awt-core/src/stores/claude_json.rs` via `json_string_literal()`. Four
raw-byte regression tests added at `crates/awt-core/tests/claude_json_escaping.rs`, which also
wire up the previously orphaned `test/fixtures/claude-json-variants/`. Verified on the real
scratch home: apply exits 0 with 6 changes, verify all `[ok]`, rollback 5/5 byte-identical.
Workspace suite 114 tests green.

Nothing left for you here except reviewing the diff.

### 1.2 Decide on AR-02 (associate refuses transcript-less projects) **[HUMAN]**

`associate` refuses a project whose transcripts have expired but whose `history.jsonl` and
`claude.json` state survive - which is the main case the command exists for. Your call:

- **Fix it now**, alongside AR-01, or
- **Defer to v1.x** and document the limitation in `docs/reference/commands.md` and the FAQ.

Deferring is defensible; shipping it silently is not. Note this bites you personally: your own
`~/.claude` still carries `E:\Projects\prisant-labs\claude-project-mover` residue from this
repo's rename, and AR-02 is why `associate` cannot currently clean it up.

### 1.3 Decide on AR-03 (`--json` ignored by `plan` and `verify`) **[HUMAN]**

`--json` is documented as a global flag but is only implemented for `doctor`, `list`, and `scan`.
`plan` and `verify` accept it, exit 0, and print human text anyway. Either implement it or reject
the flag where unsupported; silently ignoring it is the one indefensible option.

This is also a **v2 prerequisite**: the roadmap's GUI parity gate is
`GUI plan model == awt plan --json`, and that contract cannot be written today. Implementing it is
unblocked work that can proceed while the v1.0 tag is stuck.

### 1.4 Re-run the acceptance run **[DELEGABLE, but you own the verdict]**

After AR-01 is fixed, repeat [`docs/acceptance-run.md`](../acceptance-run.md) end to end. The
scratch home from the 2026-07-28 run is retained at
`E:\Projects\_temp\awt-acceptance-2026-07-28` so the same data can be reused.

### 1.5 Sign off S-01 **[HUMAN - nobody else can do this]**

[`plan_v1.0.0/S-01_mover-cli/spec.md`](release-plans/plan_v1.0.0/S-01_mover-cli/spec.md) carries
`status: draft` and `requires-human-review: true`. It was carved from the gitignored umbrella spec
CPM-01 by an agent and **has never been read back by a human**. Hygiene gate (a) fails until you
read it and flip the frontmatter to `status: committed`.

Evidence to read alongside it:
[`S-01_mover-cli/ac-traceability.md`](release-plans/plan_v1.0.0/S-01_mover-cli/ac-traceability.md)
maps each acceptance criterion to the test enforcing it.

This is deliberately the one gate an agent cannot clear. Do not let anyone flip it for you.

---

## 2. Open decisions waiting on you **[HUMAN]**

| ID | Question | Where |
|---|---|---|
| D6 (history drive-letter repair) | 45 `history.jsonl` entries in your live home have `::\Projects\...` where `E:\Projects\...` belongs. Leave them, build `awt repair --drive-letter` in v1.x, or fix by hand? Recommendation: leave for v1.0, consider for v1.x. | [plan_v1.0.0.md](release-plans/plan_v1.0.0/plan_v1.0.0.md) D6 |
| githubRepoPaths type strictness | A `githubRepoPaths` value of the wrong JSON type is currently ignored silently rather than raising exit 4. Deliberate or accidental? | AR-01 report, Observations |
| Dependabot | `.github/dependabot.yml` was **not** added, to avoid imposing PR noise on you. Want it? | n/a |

D1-D5 are resolved. D4 (retro-tag v0.1.0) and D5 (manual archive copy) were closed on 2026-07-28
as superseded by events.

---

## 3. The tag ceremony **[HUMAN]**

Only once sections 1 and 2 are clear. Full detail in
[`docs/release-runbook.md`](../release-runbook.md); the human-only steps are:

1. Confirm CI is green on the exact commit you intend to tag.
2. Flip the README status badge from `pre-release` to released and remove the pre-release
   callout. (Deliberately not done in advance - it would be false until the tag exists.)
3. Date the `[1.0.0] - unreleased` heading in the root `CHANGELOG.md`.
4. `git tag -a v1.0.0 -m "v1.0.0"` and `git push origin v1.0.0`.
5. Update `docs/ROADMAP.md` Section 7 with the tag date.
6. Delete the scratch homes:
   `E:\Projects\_temp\awt-acceptance-2026-07-28{,-backups,-archive}`.

---

## 4. Not blocking anything

- **CI-3, the signed binary channel.** Explicitly not a v1.0.0 blocker; v1.0 ships source-first,
  so there is no SmartScreen or Gatekeeper surface. Revisit when you actually want to ship
  binaries.
- **P10 cross-volume move (AC-2)** and **P11 Codex/Gemini adapters (AC-27).** Parked v1.x
  candidates behind the existing adapter boundary.
- **v2.0.0 "Taura", the Tauri + React GUI.** Brief at
  [`v2-gui-design-brief.md`](v2-gui-design-brief.md). Design and scaffolding work can proceed in
  parallel with v1.0 testing; see that brief and `docs/ROADMAP.md` Section 1.

---

## How to keep this file honest

Update it whenever a gate changes state, and prefer moving detail *out* of here and into the
owning document rather than duplicating it. The failure mode this file exists to prevent is four
documents each holding a quarter of the answer, so the moment it starts restating the release
plan instead of pointing at it, it has become the fifth quarter of the problem.
