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

### 1.2 Fix AR-02 (associate refuses transcript-less projects) - **DONE 2026-07-28**

Decision was "fix now". `associate` resolves its target through `doctor::scan` across every store
instead of the transcript-keyed index, and the export step no-ops when there are no transcripts
rather than aborting. A second layer was fixed with it: `verify` no longer demands a destination
transcript directory for a project that never had one.

Proven against the case that motivated it - this repo's own pre-rename residue in your
`~/.claude` re-associated cleanly: 3 changes applied, exit 0, all 4 references relocated.

### 1.3 Fix AR-03 (`--json` ignored by `plan` and `verify`) - **DONE 2026-07-28**

Decision was "fix now", and implemented rather than rejected because the v2 parity contract needs
the output to exist. `Plan::to_json()` emits the full plan model with a `kind` discriminant per
change, literal find/replace rules for byte-level drill-down, and a `totals` object.
`verify --json` emits the check list plus `failed` and `ok`. Exit codes are unchanged by format.

**This unblocks v2.** `GUI plan model == awt plan --json` can now be written against the shipped
binary, which was the blocking dependency for starting GUI work.

### 1.4 Fix AR-04 (duplicate `githubRepoPaths` values) - **DONE 2026-07-28**

Found while re-verifying AR-01, and a release blocker in its own right. Two slugs holding the same
path value each planned an edit expecting one match, while each edit counts across the whole file
and saw two. Identical splices are now coalesced with the correct total.

Worth knowing because it generalises: fixing AR-01 changed the error from `live 0` to `live 2`,
which looked like progress and was in fact a second defect surfacing from underneath the first.

### 1.5 Re-run the acceptance run **[DELEGABLE, but you own the verdict]**

All four findings are fixed, so this is now the last technical gate. Repeat
[`docs/acceptance-run.md`](../acceptance-run.md) end to end - each fix was verified against the
step that failed, but the full sequence has not been run since. The scratch home from the
2026-07-28 run is retained at `E:\Projects\_temp\awt-acceptance-2026-07-28` so the same data
can be reused.

### 1.6 Sign off S-01 **[HUMAN - nobody else can do this]**

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
