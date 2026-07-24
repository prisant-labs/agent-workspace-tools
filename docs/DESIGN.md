# Claude Project Mover (CPM) - Validated Design

Status: approved design (brainstorming output), 2026-07-10.
Supersedes the tech-stack and encoding claims in `_local/initial-discovery/03-05`.
Evidence base: `_local/initial-discovery/01-02` plus direct empirical verification
of `~/.claude` on this machine, 2026-07-09/10 (probes recorded in the session).
The `_local/initial-discovery/*` files are intentionally NOT committed; Section 2 below
inlines the authoritative extraction of their load-bearing findings, so every correction
is independently checkable in-repo without the raw notes.

## 1. Problem

Moving a project folder is the easy 10%. The hard 90% is the AI-CLI state that
lives outside the folder and is keyed to its old absolute path: Claude Code
transcripts, `~/.claude.json`, `history.jsonl`, and (later) Codex and Gemini
equivalents. A manual move on 2026-07-09 relocated six folders, worked, but was
slow, needed judgment on a shared/renamed history, and still left `history.jsonl`
and other stores stale. CPM turns that judgment-heavy manual procedure into a
deterministic, verifiable tool with zero LLM/network use at runtime.

The goal is a tool that is provably safer than a careful human: dry-run, backup,
verify, and rollback are the point, not features.

## 2. What empirical verification changed

The discovery docs are directionally right but contained load-bearing errors.
Verified against the live machine:

1. **Path encoding replaces every non-alphanumeric character, not just `: \ / space`.**
   Proof: `...\agent-skills-toolkit\.claude\worktrees\f2-phase-c-advisory` encodes
   to `...-agent-skills-toolkit--claude-worktrees-f2-phase-c-advisory` (dot -> `-`)
   and `...\pm-skills\docs\internal\release-plans\v2.26.0` to
   `...-release-plans-v2-26-0` (dots -> `-`). No dir among 45 contains a dot or
   underscore. Rule: `[^A-Za-z0-9] -> -`.
2. **The encoding is lossy and cannot be inverted.** `a-b`, `a.b`, `a\b`, `a_b`
   all encode identically. Discovery must therefore build a reverse index from the
   `cwd` stored inside transcripts, not compute `encode(src)` and look it up.
3. **The dir name derives from the launch-time `cwd` string, not a normalized path.**
   Three dirs disagree in drive-letter case with their own stored `cwd`
   (`d--Cloud-Work-PP` holds `cwd: "D:\Cloud-Work-PP"`). Matching is
   case-insensitive; `encode(storedCwd) != dirname` in general.
4. **Two path-keyed stores the docs missed** (found by sweeping 25,801 files for
   already-moved paths):
   - `~/.claude.json -> githubRepoPaths{}`: slug -> array of abs paths (incl.
     case variants). Six entries stale today.
   - `~/.claude/plugins/data/<plugin>/state/<basename>-<sha256(absPath)[:16]>/`.
     Confirmed: `sha256("E:\Projects\Github Repos\markdown-for-humans")[:16] ==
     e854827f52137cd9`, the exact stale dir suffix. Third-party plugin => the
     path-keyed surface is open-ended, not a closed set of three CLIs.
5. **`usage.db` is 15.5 MB, not "15.5 GB-class"** (a 1000x unit error in `02` that
   overstated the SQLite risk).
6. **`projects/<enc>/` holds more than `*.jsonl`**: `memory/*.md`,
   `<sessionId>/tool-results/`, `subagents/*.meta.json`. Sidecars are a scope tier.
7. **26 transcripts belonging to OTHER projects contain the old path** (it was
   discussed in those sessions). Those must never be rewritten. The correct
   postcondition is "zero old-path refs in the moved project's own path-keyed
   state," not "zero old-path refs anywhere."

## 3. Tech stack: Rust core + Tauri (deferred), reversing the brief

The strategy brief picked TypeScript. Portfolio evidence reverses it:

- **`repo-sync-tool`** (the model): Rust + Tauri 2 + React 19. A `reposync-core`
  crate forbidden from depending on Tauri, enforced by one CI line
  (`cargo tree -p reposync-core | grep -i tauri` -> exit 1). Typed IPC via
  `tauri-specta` into `src/lib/bindings.ts`, with a stale-bindings CI gate. Ships
  as a single dependency-free binary with a winget + minisign updater pipeline.
  This structurally guarantees CPM's core/GUI parity requirement.
- **`adobe-cclib-liberator`**: Python + pywebview + PyInstaller. Its own
  `docs/tech-stack.md` says the ONLY reason it isn't Rust/Tauri is a pre-existing
  tested Python engine. It then paid: no typed IPC, PyInstaller `email`-exclusion
  crash, `_ConsoleSafeStream` shim, accidental `cryptography` bundling, no
  installer, macOS build-only. CPM has no pre-existing engine, so that constraint
  does not apply.
- The brief's one anti-Rust argument (byte-preserving rewrites are "ceremony in a
  serializer that reformats") dissolves once the algorithm is named correctly: the
  tool must NEVER serialize. It parses only to validate shape, discards the result,
  and writes by literal anchored splice on the original bytes. The hazard is
  identical in every language; the fix is "don't serialize."

v1 crates: `clap`, `serde_json` (validate-only), `sha2`, `dunce` (Windows
verbatim-path stripping, proven in repo-sync-tool), `walkdir`, `tempfile`, `insta`
(golden snapshots). No tokio, no sqlx, no git2. Toolchain present: cargo 1.96.0.

**Platform scope:** v1 targets Windows only. macOS/Linux bring-up is deferred and
tracked: path encoding, `same_volume`, cloud-sync detection, and the CI matrix each need
macOS-specific work (POSIX path forms, mount/device volume detection, iCloud Drive under
`~/Library/Mobile Documents`) before that scope opens.

## 4. Architecture

```
agent-workspace-tools/
  Cargo.toml            virtual workspace manifest
  crates/cpm-core/      no clap, no tauri, no network. the whole engine.
  crates/cpm-cli/       clap. bin: `cpm`
  src-tauri/            DEFERRED. thin edge, tauri-specta
  src/                  DEFERRED. React 19 + shadcn, generated bindings.ts
```

CI dependency-hygiene gate (copied from repo-sync-tool):
`cargo tree -p cpm-core | grep -iE 'tauri|clap' && exit 1`.

`cpm-core` is pure functions over an injectable `FileSystem` trait; tests run
against a `MemoryFileSystem`, never live `~/.claude`.

### Read layer vs write layer

Four commands only read; two mutate. The read commands share one index and the
same adapters, so verify cannot certify apply's own mistakes.

```
        doctor   (no input)      "what is stale right now?"
index + scan     (--src)         "what state exists for this project?"
stores  plan     (--src --dst)   "what would change?"
        verify   (--src --dst)   "did it change correctly?"
        apply / rollback         the only writers
```

`doctor` and `plan` (dry-run) are NOT the same: `doctor` takes no destination and
inventories machine-wide staleness; `plan` takes `--src --dst` and produces a
change list. `doctor`'s output is `plan`'s input.

### Reverse index

`ProjectIndex` scans `~/.claude/projects/*` and maps `normalize(cwd) -> [dirs]`.
`encode()` is used only to NAME a destination dir, never to look one up.

It reads **every** distinct `cwd` a directory's transcripts record, not just the first.
A machine scan on 2026-07-13 (45 dirs, 11,518 transcripts) settled why: one directory,
`E--Projects-prisant-labs-obsidian-tag-visibility`, holds 17 transcripts naming THREE
paths - the live one plus two dead ones left behind by earlier moves whose transcripts
were relocated without being rewritten. Reading only the first transcript would resolve
that directory by whichever filename happened to sort first, and would discard the stale
references, which are exactly the residue `doctor` exists to report.

Resolution, given the set of distinct `cwd`s a directory records:

| Recorded | Resolution |
|---|---|
| none | `unresolved` |
| one | resolved to it |
| several, exactly one still on disk | resolved to the live one; the rest go to `stale` |
| several, none still on disk | `unresolved`, with the dead paths kept in `stale` |
| several, more than one still on disk | `ambiguous` - refuse rather than guess (`CpmError::Ambiguous`, exit 2) |

Existence is tested against the filesystem, and NTFS matches case-insensitively, so
`MemoryFileSystem` models that too (audit finding LEAD-07). A recorded `e:\projects\foo`
and an on-disk `E:\Projects\Foo` are the same path.

The same scan measured the unresolvable population: **15 of 45** directories yield no
`cwd`. They are not empty - they hold transcripts that never recorded one. They are
reported as unresolvable, never guessed.

### Store adapter contract

```rust
trait Store {
    fn id(&self) -> &'static str;
    fn probe(&self, ctx: &Ctx)  -> Result<()>;                    // hard-fail on unknown shape, pre-write
    fn detect(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<Hit>>;   // state for one project
    fn audit(&self, ctx: &Ctx)  -> Result<Vec<Stale>>;            // doctor: refs to gone paths
    fn plan(&self, ctx: &Ctx, mv: &Move, hit: &Hit) -> Result<Vec<Change>>;
    fn verify(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<VerifyResult>>;
}
```

`apply` is NOT a `Store` method: it is centralized in the engine, matching over the
closed `Change` enum, so backup, folder-move-last ordering, and count-checked writes
live in exactly one place rather than being re-implemented per adapter. `probe` returns
`()` in v1: it hard-fails on an unrecognized shape and otherwise reports nothing, because
Shape reporting (format-drift classification) is deferred. `Change` is a closed enum,
each variant carrying expected counts so apply refuses when reality disagrees:
`MoveTree`, `RenameDir`, `RewriteFile { rules, counts }`, `RenameJsonKey`,
`RewriteJsonArrayValue`.

### v1 store registry

| Store | Location | Class | Notes |
|---|---|---|---|
| `claude.projects.dir` | `~/.claude/projects/<enc>/` | RENAME | via reverse index, case-insensitive |
| `claude.projects.transcripts` | `<enc>/*.jsonl` | REWRITE | exact `cwd` + anchored abs-path prefixes |
| `claude.json.projects` | `~/.claude.json projects{}` | RENAME-KEY | all slash/case variants (3 groups live) |
| `claude.json.githubRepoPaths` | `~/.claude.json githubRepoPaths{}` | REWRITE-VALUE | array values, 6 stale today |
| `claude.history` | `~/.claude/history.jsonl` | REWRITE | `project` field, 11 stale today |
| `plugin.state.dirs` | `plugins/data/*/state/<base>-<sha256[:16]>/` | RENAME | hash recomputed from new path |
| `sweep.unknown` | unowned regions under `~/.claude`, skipping `projects`, `history.jsonl`, `plugins`, `file-history`, `backups` | REPORT ONLY | structurally cannot write; findings land in `DoctorReport.report_only`, never `stale` |

### Rewrite tiers (rewrite indices, not records)

- **Minimal**: dir name, `cwd`, `claude.json` keys, `githubRepoPaths`,
  `history.jsonl project`, plugin state dir.
- **Standard (default)**: Minimal + anchored abs-path prefixes inside the moved
  project's OWN transcripts.
- **Full (opt-in)**: Standard + sidecars (`memory/*.md`, `tool-results/`,
  `subagents/*.meta.json`).

Other projects' transcripts are never rewritten at any tier. Hard invariant.

## 5. Correctness rules

1. **Parse to validate, never to write.** `serde_json::from_str` confirms shape,
   result discarded; all writes are literal byte splices. Never re-serialize.
2. **Every replacement boundary-anchored and count-checked.** `buildPathRules`
   emits only: exact `"cwd":"<oldEsc>"`; `<oldEsc>\\` (escaped-backslash prefix +
   sep); `<oldFwd>/` (forward-slash prefix + sep). Never a bare prefix. Validated
   to reproduce the reference-move counts (cwd 227+1240, backslash 54+534, forward
   0+27) while leaving `markdown-for-humans@0.2.1` (2+8) and `markdown-for-humans_dev-*`
   (6+49) byte-identical. (Preserved-mention totals corrected 2026-07-11 against the
   captured fixtures: 10 and 55; earlier notes had recorded the larger file's per-file
   counts as totals. See `test/fixtures/README.md`.)
3. **Per-store escaping facts.** Transcripts/history store JSON-escaped paths
   (`\\`); `claude.json` keys matched as `"<path>":` (never a value);
   `githubRepoPaths` values are arrays (rewrite element in place, length
   unchanged); plugin suffix is `sha256(newAbsBackslash)[:16]` (generated, not
   rewritten).
4. **Encoding** `encode_project_dir(abs) = replace [^A-Za-z0-9] with -`.
   Forward-only.

## 6. Safety model

Apply is one ordered, backup-first, fully reversible transaction:

```
1. probe every store        unknown shape -> abort, nothing written    (exit 4)
2. build plan               empty plan -> no-op success                 (exit 0)
3. snapshot -> backup dir   originals + manifest.json (path, backup, sha256)
4. apply state changes      claude.json, history, transcripts, plugins
5. move folder LAST         after all state edits succeed
6. verify                   any failed postcondition -> auto-rollback   (exit 3)
7. write report.json        actions, counts, backup path, verdict
```

- **Backup is a real snapshot**, not a diff. `runId` passed in, never generated in
  core (determinism). Rollback reads only the manifest.
- **Idempotency is free**: re-run finds no source dir/keys -> empty plan -> no-op.
- **Guards** (each its own exit code): destination exists -> refuse (2); worktree
  source (`.git` is a file) -> refuse without `--force` (2); live process/IDE lock
  -> warn, require `--force` (2); unknown store shape -> hard-fail pre-write (4).
- **verify is an independent read path**, not asserts inside apply, so it catches
  writes that never reached disk.

### Resolved policy decisions

- **Destination key collision in `claude.json`**: refuse by default (exit 2);
  `--on-collision=keep-dest|keep-src` to override. No silent config loss.
- **Nested project keys under the source** (e.g. `E:\Projects\Github Repos` and
  `...\markdown-for-humans` both live): migrate only the named path; detect and
  list nested keys as "will break unless `--recursive`"; touch them only with
  `--recursive`. Never silent, never surprising.

## 7. CLI surface and exit codes

Mover commands: `doctor` (no args), `scan --src`, `plan --src --dst`,
`apply --src --dst`, `verify --src --dst`, `rollback --report`.
Feature commands (F13-F15, in v1.0; see Section 10): `list` (inventory), `archive`
(retention), `associate --from --to` (re-associate/export).
Global flags: `--home` (default `os.homedir()`), `--backup-root`, `--force`,
`--scope=minimal|standard|full`, `--on-collision`, `--recursive`, `--json`,
`--no-auto-rollback`. (The `--attribute=fork|base|both` resolver for ambiguous history is
deferred to v1.x, decision 7a; v1.0 fails closed on ambiguity - see `docs/troubleshooting.md`.)

Exit codes: `0` success; `1` unexpected I/O error; `2` guard/refusal;
`3` verification failed; `4` unrecognized format.

## 8. Testing

- **Fixture-based unit tests** over `MemoryFileSystem`, seeded from captured bytes.
  Anchor: `test/fixtures/reference-move/{before,after}/` from
  `E:\tmp\claude-move-backup-20260709-090053` and current migrated files.
- **`insta` snapshots** lock every `plan` render and `report.json`.
- **No-network guarantee** is structural: the CI dependency gate
  (`cargo tree -p cpm-core | grep -iE 'reqwest|ureq|hyper|curl'`) forbids any
  network-capable crate from entering `cpm-core`, and `cargo audit` catches
  RUSTSEC advisories. No runtime outbound-request test is required.
- **Parity test** (post-GUI) asserts GUI plan model == `cpm plan --json`.
- **Three fixtures captured now** (irreproducible later): the markdown before/after
  move; a `claude.json` with the 3 variant groups + 6 stale `githubRepoPaths`; the
  `markdown-for-humans-e854827f52137cd9` plugin dir.

## 9. Phase plan

v1.0 (mover) = phases 1-9; `doctor` shippable at phase 4. v1.1 (features F13-F15)
= phases 13-16, most needing only the read layer. Deferred = 10-12.

| Phase | Delivers | Milestone |
|---|---|---|
| 1 | Workspace, `FileSystem` trait + Memory impl, fixtures, no-network + CI dep-gate | |
| 2 | `encode_project_dir` (corrected) + reverse `ProjectIndex` | |
| 3 | `Store` trait, `probe`/`detect`/`audit`, 6 adapters' read paths + `sweep.unknown` | |
| 4 | `cpm doctor` + `cpm scan` - read-only, exit codes, report | shippable v0.1 |
| 5 | Anchored rewrite engine + `buildPathRules`, count-checked, golden test | |
| 6 | `plan` (diff + machine plan), collision + nested + worktree detection | |
| 7 | `snapshot`/backup + manifest, transactional `apply`, folder-move-last | |
| 8 | `verify` + auto-rollback, idempotency, hard-fail, lock detect | |
| 9 | `rollback` from manifest, CLI complete, exit-code contract | v1.0 |
| 13 | Session-keyed linkage + `cpm list` (terminal/json/html) | F13, v1.1 |
| 14 | Archive engine + content-hash dedup + `cpm archive` bulk/hook/retention | F14, v1.1 |
| 15 | `cpm associate --from --to` (re-associate and/or export) | F15, v1.1 |
| 10 | Cross-volume copy + checksum-verify + delete | deferred |
| 11 | Codex + Gemini adapters, opt-in behind flags | deferred |
| 12 | Tauri + React GUI over the identical core | deferred |

Phase 4 is the honesty checkpoint: `doctor` on the real machine must report exactly
the residue found by hand (6 stale `githubRepoPaths`, 11 stale history values, the
orphaned plugin dir). If it does, the read layer is trustworthy and phases 5-9 build
writes on proven ground. Phases 13-14 depend only on the phase 1-4 read layer plus
phase 7 copy primitives, so they can land close to the doctor milestone; phase 15
reuses the full phase 5-9 write path plus phase 14's archive writer.

## 10. Retention features (F13-F15, shipped in v1.0)

Added 2026-07-10. Full spec: `docs/features/v1.1-inventory-retention-reassociate.md`.
Reference: `docs/reference/claude-data-model.md`, `docs/reference/existing-solutions.md`.
Claude-only, same adapter boundary; determinism and no-network apply. (The "v1.1" labels in
this section and in the Section 9 phase table predate the scope decision that folded F13-F15
into v1.0.0; see ROADMAP Section 1. The spec filename keeps its original name.)

- **F13 `cpm list` (inventory).** Report every project Claude has state for, with
  session counts, sizes, transcript ages (so the 30-day cliff is visible), linked
  SESSION-keyed stores, PATH-keyed declarations, and a health flag (OK/STALE/
  UNRESOLVED). Renders terminal / `--json` / `--html`. Needs only the read layer.
- **F14 `cpm archive` (retention).** Copy transcripts and SESSION-keyed artifacts to
  a user-defined archive folder before the 30-day cleanup deletes them; incremental,
  deduped by content SHA-256 (not mtime). `--install-hook` adds a `SessionEnd` hook
  so new sessions auto-archive. `--set-retention <days>` writes a large finite
  `cleanupPeriodDays` as a safety net. The tool is git-agnostic (user's folder, user's
  choice of git/sync); atomic writes; warns on cloud-sync roots.
- **F15 `cpm associate --from A --to B` (re-associate/export).** Keep A's history with
  B when A is being deprecated. `--reassociate` runs the mover's state migration
  minus the folder move; `--export` drops a portable copy into `B/<subdir>` (F14
  format). Both default on and independently disableable. Finds A's sessions via
  stored `cwd` even when A's folder is already gone.

New acceptance criteria AC-28 through AC-44 live in the feature spec.

### The retention hazard (design-shaping)

Transcripts auto-delete at `cleanupPeriodDays` (default 30, on startup); measured
2026-07-10, nothing on this machine survives past 30 days. `history.jsonl` never
expires. **`cleanupPeriodDays: 0` is unsafe to rely on** (issue #23710: it may
disable transcript writing; #62272: cleanup keys off mtime). So archive-out is the
real durability mechanism; CPM sets only a large finite retention value and refuses
`0` without an explicit override. See `claude-data-model.md` Section 5.

## 11. Non-goals (v1)

- Rewriting opaque SQLite telemetry (`usage.db`, Codex `logs_2.sqlite`).
  Inspect read-only at most.
- Any LLM/network call in the migration path.
- Editing project-internal files that move with the folder (unless they hardcode
  their own abs path -> REVIEW, not auto-rewrite).
- Cross-machine sync as a general product, and multi-project batch mode, in v1.
  (F14 archival is not general cross-machine sync; it is local archive-out.)
- Building a transcript viewer/renderer from scratch: reuse prior art
  (`existing-solutions.md` Section D) for F13's HTML.
