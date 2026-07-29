# Glossary

The vocabulary used across the docs and the code. Most of it is either Claude Code's own
terminology or a term this project coined for a concept that needed a name.

## Claude Code's data

**Claude home.** The pair `~/.claude/` (a directory) and `~/.claude.json` (a sibling file).
Both halves hold path-keyed state, which is why `--home` points at a directory containing both,
and why copying only `.claude/` gives you an incomplete copy.

**Transcript.** The `.jsonl` file recording one session, stored under
`~/.claude/projects/<encoded-dir>/<session-id>.jsonl`. Transcripts are the source of truth for
which real path a project directory belongs to, and the one thing the tool avoids rewriting
except where the project's own recorded `cwd` demands it.

**`cwd`.** The absolute project path recorded inside a transcript. Because the directory-name
encoding is lossy, the `cwd` recorded in the file is the only reliable way to map an encoded
directory back to a real path.

**Encoded project dir.** The directory name under `projects/`, produced by replacing every
non-`[A-Za-z0-9]` character in the absolute path with `-`. So `E:\tmp\chain-smoke` becomes
`E--tmp-chain-smoke`.

**Lossy and forward-only.** The property that makes the encoding dangerous: `a-b`, `a.b`,
`a\b`, and `a_b` all encode identically, so the transformation cannot be inverted. Never look up
an existing project directory by computing `encode(path)`.

**Reverse index.** The structure that solves that: read the `cwd` out of every transcript and
build a map from normalized path to directory. This is how the tool finds a project's state
instead of guessing at a directory name.

**The 30-day cliff.** Claude Code's default auto-deletion of transcripts after 30 days, with no
warning and no recovery. `awt list` surfaces each project's oldest transcript age against it;
`awt archive` is the defense. Note that `history.jsonl` does *not* expire, so old projects
routinely have history but no transcripts.

## This project's architecture

**Store.** One category of path-keyed state with its own on-disk shape and its own read/write
adapter. v1.0 covers six: `claude.projects`, `claude.json` (project keys), `claude.json`
(`githubRepoPaths`), `claude.history`, `plugin.state`, and `sweep.unknown`.

**Store adapter.** The code implementing read, plan, and write for one store. The adapter
boundary is what makes Codex and Gemini support an addition rather than a rewrite.

**Probe.** The pre-write check that a store matches a recognized shape. A shape the tool does not
recognize is an abort (exit 4), never a best-effort guess.

**Parse to validate, never to write.** The core invariant: JSON is parsed only to confirm a
file's shape, then the result is discarded and the file is edited by literal byte splice.
Re-serializing would reformat whitespace and destroy the ability to verify the edit. The
corollary that bit in practice: the *parsed* value is unescaped while the *file* is escaped, so
an anchor derived from the parsed value may not exist in the bytes.

**Anchored, count-checked rewrite.** Every replacement is bounded by literal delimiters and
carries an expected occurrence count. If the live count differs from the planned count, the write
is refused rather than applied. "expected 1, live 0" is this check firing.

**Sweep / report-only.** The scan of regions no adapter owns - `plugins/`, `file-history/`,
`backups/`. Old paths there are *correct by design*, because those files record a past state.
Findings are surfaced in `DoctorReport.report_only` and structurally excluded from the rewrite
path.

**"A match is not staleness."** The principle behind report-only: another project's transcript
mentioning your old path is an accurate historical record, not a bug to be fixed. The
postcondition is zero old-path references in the moved project's *own* state, not everywhere.

**Snapshot / manifest.** Before the first write, `apply` copies every file it intends to touch,
with sha256, into `<backup-root>/awt-<run-id>/`. The `manifest.json` in that directory is what
`rollback --report` consumes.

**Verifiable revert.** `rollback` does not just restore files, it re-hashes each restored file
against the snapshot and proves byte-identity, failing with exit 3 on any mismatch.

**Guard / fail closed.** A precondition that stops a run before anything is written: destination
exists, worktree source, cross-volume move, live lock, ambiguous history. All produce exit 2.
"Fail closed" means the default on uncertainty is to refuse.

**Idempotent by refusal.** Re-running `apply` on an already-migrated project exits 2 with a
destination-exists message rather than doing nothing quietly. Scripts should read that specific
exit-2 case as "already done".

**Health.** The per-project status in `awt list`: `ok`, `stale` (contains stale references), or
`unresolved` (transcripts with no resolvable `cwd`).

**Scope.** How much `apply` rewrites: `minimal` (primary project key only), `standard` (all
path-keyed stores, the default), `full` (also path mentions inside transcript conversation text).

## Process terms

**Honesty checkpoint.** The Phase 4 gate: `awt doctor` on a real machine must report exactly the
residue found by hand, no more and no less. All write-phase work was gated on passing it. It is a
procedure to re-run, not a fixed set of numbers to match.

**Acceptance run.** The manual pre-tag test against a copy of a real Claude home, documented in
[acceptance-run.md](acceptance-run.md). The one test that cannot be automated, and the gate that
caught AR-01.

**Effort (S-01, S-02).** A unit of release-scoped work with its own spec and implementation plan,
under `docs/internal/release-plans/plan_vX.Y.Z/`. S-01 is the mover CLI; S-02 is inventory,
retention, and re-associate.

**AC (AC-1, AC-26, ...).** An acceptance criterion in an effort's spec. Every AC should trace to
a test; the mapping lives in each effort's `ac-traceability.md`.

**ADR.** Architecture Decision Record in MADR v4 form, under `docs/decisions/`. ADR-0001 is the
project rename to `agent-workspace-tools`.
