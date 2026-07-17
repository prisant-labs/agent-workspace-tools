---
date: 2026-07-17T09:45:00-07:00
type: session-log
repo: https://github.com/prisant-labs/claude-project-mover
branch: main
summary: "Landed the three Phase 3 review criticals (C-1/C-2/C-3), then built Phase 4 (doctor + scan engine and the cpm CLI). Real-machine run passed the honesty gate and exposed a sweep-scope policy question, now open."
files-changed:
  - crates/cpm-core/src/stores/plugin_state.rs
  - crates/cpm-core/src/stores/sweep.rs
  - crates/cpm-core/src/doctor.rs
  - crates/cpm-core/src/lib.rs
  - crates/cpm-cli/Cargo.toml
  - crates/cpm-cli/src/main.rs
  - crates/cpm-cli/src/exit.rs
  - Cargo.toml
  - Cargo.lock
session-type: interactive
parent-session: 2026-07-11_22-24_fable_audit-repair-release-scaffold-phase1-build.md
model: claude opus 4.8
model-settings: explanatory output style
agent: claude-code
status: clean-stop
decisions-count: 5
commit-sha: 9c824e0
transcript-path: C:/Users/jpris/.claude/projects/E--Projects-prisant-labs-claude-project-mover/ccfd8a02-2996-48c9-a6d3-64ee7d9c402b.jsonl
tags: [phase-3-review, phase-4, doctor, scan, cli, tdd, systematic-debugging, honesty-gate]
---

# Session Log: Phase 3 Review Fixes, then Phase 4 (doctor + scan + CLI)

## Where this picked up

The previous session ended (2026-07-11, "interrupted") after committing Phase 3 to `main`
at `c3401d9`, then running a whole-phase review that found three criticals - none of which
had been applied to the tree. So the tree was green (33 tests) but knowingly wrong. The
user's instruction this session was "do it all": land the review fixes, then continue the
plan into Phase 4.

Plan reference: `docs/superpowers/plans/2026-07-10-claude-project-mover.md`, Phase 4 at line 2188.

## What was done (in order)

### 1. C-1 - plugin_state audit inverted staleness into a guess (`8721157`)

**The bug.** `plugin_state::audit` asked "can I explain this state dir?" and treated "no"
as proof of staleness. It walked every `plugins/data/*/state/*` dir, and flagged any whose
`<name>-<16 hex>` suffix was NOT in the set of hashes of known cwds. Run against a real
`~/.claude` that reported 8 orphans where 1 was real, including `1.0.4-c2306d34173b4c6b` -
a **live Codex broker** whose name coincidentally has the same shape but whose suffix is a
version hash, not a path hash. "I cannot explain this dir" and "this dir is stale" are
different claims; conflating them is a direct violation of the refuse-rather-than-guess
invariant (DESIGN.md), and would have fed a live-directory delete into Phase 5+.

**The fix.** Invert the search: run from the paths we know, not the dirs we find. Compute
the set of recorded cwds that are gone from disk (`ctx.index.cwds` filtered by
`!fs.is_dir`), hash each, and match those hashes against the state dirs. A dir is an orphan
only when both halves hold - its suffix is the hash of a recorded cwd AND that cwd is gone.
Dirs we cannot tie to any recorded path go unmentioned. This is `crates/cpm-core/src/stores/plugin_state.rs:61`.

### 2. C-2 - the test that let C-1 survive (same commit)

The old `audit_flags_orphan_state_dir` used a fixture with **no transcripts at all**, so
`ProjectIndex.cwds` was empty and *every* dir looked stale. It passed whether or not the
discrimination logic existed - a vacuous test, exactly the Phase 3 pattern the review
flagged. Replaced with `audit_flags_only_the_dead_project_whose_path_it_can_explain`, a
fixture holding all three populations at once (a live project, a dead one, and the
unexplainable broker dir) and asserting on the **exact** set `["markdown-for-humans-e854827f52137cd9"]`.
Watched it RED first: it reported `["1.0.4-c2306d34173b4c6b"]` - the live broker - proving
the inversion before the fix.

### 3. C-3 - sweep "stub" was the design, not a defect (same commit)

The review read `sweep::audit` returning empty (with real logic in the `sweep_for` free
function) as an unfinished stub. It is not. DESIGN.md:187 and plan Task 3.5 are explicit:
sweep is `REPORT ONLY | structurally cannot write`. Its inert `Store` methods ARE the
safety property - an empty `plan` cannot emit a change, so no `apply` can act on a sweep
finding. And `sweep_for` must be a free function because it needs the gone-path needles
gathered from every OTHER adapter's audit, which the `Store::audit(&ctx)` signature cannot
carry. The plan carried a comment explaining this; the implementation dropped it, which is
why review misread it. **Resolution: restore the intent as a type-level doc comment**, not a
restructure.

**But** the review's fourth point (sweep's `OWNED` check) was a real latent bug: `OWNED`
was substring-matched against the **absolute** path, so a home under a directory called
`projects` would read as adapter-owned and silently disable the entire sweep. Fixed to match
the first path component **relative to** `~/.claude`. Two RED tests, one of which
(`owned_regions_are_still_skipped`) passed immediately and was kept as a guard against
over-fixing. `crates/cpm-core/src/stores/sweep.rs`.

### 4. Phase 4 Task 4.1 - doctor + scan engine (`afc8ecf`)

`crates/cpm-core/src/doctor.rs`. `doctor` probes every store before auditing any (unknown
shape aborts the whole run), aggregates `audit()` across the registry, then runs `sweep_for`
last with the deduped needles. `scan` runs `detect()` across the registry for one src path.

**TDD discipline note (important):** I first wrote impl + tests together - a violation. I
parked the impl to scratchpad, stubbed the bodies to `Ok(empty)`, and ran the tests against
the stub. One test (`scan ... reports_no_hits_for_an_unrelated_path`) **passed against the
do-nothing stub** - the C-2 pattern reappearing live. Rewrote it into
`scan_distinguishes_the_named_project_from_an_unrelated_path`, pairing the positive and
negative case over one fixture so absence alone can't satisfy it. All three then RED against
the stub, then GREEN after restoring the impl.

### 5. Phase 4 Task 4.2 - cpm CLI (`9c824e0`)

New `crates/cpm-cli` crate (bin `cpm`), clap-derived, `doctor` and `scan` subcommands, global
`--home` and `--json`. Exit-code map (`exit.rs`) TDD'd with a single test asserting the four
guard errors share exit 2 and the other three failures are distinct. **One deliberate
departure from the plan:** the plan's `home_of` calls `.expect("home dir")` (panic) when no
home env var is set; I made it exit 1 with a message telling the user to pass `--home`. A CLI
that panics on an unset env var is reporting our stack, not the user's problem.

## The honesty gate (plan Task 4.2 Step 4) - passed, and it found something

Ran `cpm doctor` against the real `~/.claude`. First run appeared to hang (piped to `head`,
cold cache over ~900 MB). I invoked systematic-debugging rather than guess:

- **Bisection via `scan`:** `scan` builds the *same* `ProjectIndex` but never sweeps. It
  returned correct hits in **2.0s**, proving the index is not the bottleneck.
- **Instrumented `doctor`:** all five audits are sub-35ms; the walk finds 50,938 files in
  1.5s; **`sweep_for` takes 14.5s**. So "hang" was wrong - it's a 15s cold-cache run, and the
  first attempt just blocked on the `head` pipe. Corrected the claim.

**Gate result (real machine):** 126 stale references, 15 unresolvable project dirs (matches
the independent machine scan in DESIGN.md), the known stale `githubRepoPaths` including
`E:\Projects\Chrome - Bookmark Autosort`, 11 stale `history.jsonl` values, and 2 real
`plugin.state` orphans (obsidian-tag-curator residue). **C-1 verified end-to-end**: the live
Codex broker `1.0.4-c2306d34173b4c6b` no longer appears at all (8 orphans -> 2, both real).

## The open question (why the session stopped here)

Of the 96 `sweep.unknown` findings, **exactly 1** is genuine unknown-region residue
(`finish-tag-visibility-rename.log` at the `~/.claude` root). The other 95:

| Region | Count | Reality |
|---|---|---|
| `file-history/` | 48 | Claude Code's own file snapshots - an old path there is the *point* |
| `backups/` | 30 | rotated `.claude.json` restore points - must keep old paths to be restorable |
| `plugins/{marketplaces,repos}` | 15 | third-party git checkouts - not our state, reinstallable |
| `plugins/data/**` | 2 | `plugin.state`'s OWNED region - a real double-report bug |

This is C-1's category error one layer up: sweep is finding *history* and calling it *rot*.
The `plugins/data` double-report is an unambiguous bug to fix regardless. The archival
regions are a policy call the user must make, so I asked via AskUserQuestion. The user did
not select an option; they asked to understand the trade-offs first, I explained, then they
asked to wrap the session. **The decision is still open.**

Three options were on the table (see continuation prompt for full text):
1. Skip owned + archival + vendored regions (fast, ~31 findings, my recommendation's base).
2. Fix only the double-report, split sweep into its own report heading (nothing hidden).
3. Both - skip the noisy regions AND label sweep output report-only. (My overall recommendation.)

## Verification (all re-run at wrap time, not remembered)

- `cargo test`: **40 passing** (38 cpm-core + 1 cpm-cli + 1 integration), 0 failed.
- `cargo clippy --all-targets`: **0 warnings**.
- `cargo fmt`: clean.
- `git status`: clean tree.
- **All commits pushed** - `main` was fast-forwarded to origin at wrap time (the 3 code
  commits `8721157`, `afc8ecf`, `9c824e0` plus this session-log commit). See "Reconcile"
  below.

## Reconcile (branches + worktrees, done at wrap)

The Phase 3 parallel-agent build left 4 stale worktrees under `.claude/worktrees/`, all on
their own `worktree-agent-*` branches pinned at `131ff50` (the Phase 3 base). Before removing
them I verified they held nothing unique:

- `git merge-base --is-ancestor 131ff50 main` -> true, and `git log main..<branch>` was empty
  for all four, so the **branches** carried no unique commits.
- Each **worktree** had uncommitted working-tree edits (which merged-branch checks never see),
  so I diffed every dirty file against `main` HEAD. All were strictly OLDER than main - two
  literally contained the bugs fixed this session: the `plugin_state.rs` worktree still had
  the inverted `!known.contains(suffix)` audit (C-1), and the `sweep.rs` worktree still had
  the substring `OWNED` check. `claude_projects.rs` still had the weak `assert_eq!(len, 0)`
  tests. Main is strictly ahead of all four; discarding lost nothing.

Then `git worktree remove --force` on all four, `git worktree prune`, and `git branch -D` the
four orphaned branches. Result: one worktree (`main`), one branch (`main`), clean tree.

## Decisions

1. C-1 fix inverts the audit to search from known-dead paths, not from found dirs - the only
   shape that structurally cannot report a dir it can't explain.
2. C-3 is resolved by documentation, not restructure: sweep's inert Store methods are the
   report-only safety property. `sweep_for` stays a free function by necessity (needle
   passing).
3. CLI departs from the plan: missing home -> exit 1 with guidance, not `.expect()` panic.
4. Sweep-scope (the 95 archival findings) is a user policy decision - not made unilaterally,
   because silencing findings in a residue-finder is a judgment the tool's owner should own.
5. Left `plugins/data` double-report unfixed *for now* only because it's cleanest to fix
   together with whichever sweep-scope option the user picks (all three options fix it).

## Outstanding / not done

- **The sweep-scope decision is open** - blocks a clean v0.1 `doctor` report. This is the
  one real carryover; everything else is landed and pushed.
- Phase 4 is functionally complete, committed, and pushed; Phase 5 (anchored rewrite engine)
  is the next plan section (line 2419) once sweep-scope is settled.

## Verbose continuation prompt

```
Continue the claude-project-mover build. Repo: E:\Projects\prisant-labs\claude-project-mover,
branch main, clean tree at 9c824e0. Read AGENTS.md and CLAUDE.md first. Plan lives at
docs/superpowers/plans/2026-07-10-claude-project-mover.md.

STATE: Phases 1-4 are done, committed, AND PUSHED - main is level with origin/main, clean
tree, one branch and one worktree (the stale Phase 3 worktrees were reconciled away this
session; see the log's Reconcile section). The three Phase 3 review criticals (C-1 inverted
plugin_state audit, C-2 vacuous test, C-3 misread sweep stub) are fixed and verified against
the real ~/.claude - the live Codex broker 1.0.4-c2306d34173b4c6b is no longer misreported.
40 tests pass, clippy clean, fmt clean. Nothing outstanding in git.

IMMEDIATE NEXT ACTION - resolve the open sweep-scope decision. Running `cpm doctor` against
the real machine returns 126 stale references, but 95 of the 96 sweep.unknown findings are
noise: 48 in file-history/ (Claude's own snapshots), 30 in backups/ (rotated claude.json
restore points), 15 in plugins/{marketplaces,repos} (vendored git checkouts), and 2 inside
plugins/data/** (which plugin.state already OWNS - a real double-report bug). Only 1 finding
(finish-tag-visibility-rename.log) is genuine residue. An old path in an archive/backup is
correct by design and must NEVER be rewritten, so this is the same "found a match != it's
stale" category error as C-1, one layer up.

The user was shown three options and asked to understand them before deciding; they have not
yet chosen. Re-offer and get an explicit pick:
  1. Skip owned+archival+vendored regions (add plugins/data, file-history, backups,
     plugins/cache|repos|marketplaces to Sweep::OWNED-style skip). Fast (~15s -> a few s,
     since those regions are most of the bytes), ~31 total findings, all actionable.
  2. Fix only the plugins/data double-report; keep scanning everything but move sweep
     findings under their own "report only, never rewritten" heading so the headline count
     stops conflating owned stale state with mere mentions.
  3. Both (my recommendation): skip the noisy regions AND give sweep its own report-only
     heading.
Whichever is chosen: the plugins/data double-report gets fixed regardless (plugin.state owns
that region). Do it TDD - the systematic-debugging + TDD discipline this session used caught
two live vacuous-test instances, so stub-first-and-watch-it-RED is non-negotiable here.

AFTER sweep-scope: Phase 5, the anchored rewrite engine (plan line 2419). This is the first
phase that WRITES, so the byte-anchored splice safety concern noted earlier (preserve key
ordering and whitespace in ~/.claude.json; never parse-and-reserialize) becomes load-bearing.
LEAD-03 (the original bug this whole tool exists to kill) can only be confirmed dead once the
rewrite half exists - detect works, rewrite does not yet.

Working style that fit this repo: superpowers TDD (RED must be demonstrated against a stub,
not assumed), systematic-debugging over guessing (it corrected a false "it's hung" claim to
a real 15s cold-cache measurement), and the honesty gate of running against the real machine
after every read-layer phase. No em-dashes/en-dashes in any output (global rule + hook).
```
