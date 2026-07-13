# CPM v2 GUI: Design Brief

**Audience:** a design agent or designer generating GUI concepts. You have never seen this
product and you do not have access to the codebase. Everything you need is in this document.

**What we want from you:** design ideas for the v2 desktop GUI. Concepts, flows, screens,
information architecture, and the emotional register of the thing. Not code.

**Status of the product today:** the engine is being built as a CLI (v1). The GUI is v2, and
nothing about it has been designed yet. That is your blank page.

---

## 1. The product in one paragraph

Developers using Claude Code accumulate a large amount of AI session history: transcripts of
every conversation, per-project settings, a global command history, plugin state. All of it is
**keyed to the project folder's absolute path**. So the moment you move or rename a project
folder, that history silently detaches. Your sessions are still on disk, but Claude can no
longer find them, `--resume` stops working, and the project's config is orphaned. **CPM (Claude
Project Mover) moves the folder and migrates all the associated state with it, so nothing
breaks.** It also inventories what history exists, archives it before it gets auto-deleted, and
can re-attach one project's history to a different project.

## 2. Who uses it, and what they are actually feeling

**Primary user:** a solo developer reorganizing their own machine. Windows first. Technical,
comfortable in a terminal, but *not* comfortable with what this tool touches.

The emotional core of this product is **fear of irreversible loss.**

Understand what is at stake. A single project's migration means rewriting thousands of lines
inside session transcripts that hold months of the user's thinking. These files are not
regenerable. There is no upstream copy. If the tool does a naive find-and-replace and corrupts
them, that work is gone permanently, and the user may not notice for weeks.

The user knows this. That is why the CLI is built the way it is, and it is the single most
important thing for your design to internalize. **The user does not want a fast tool. They want
a tool that shows them exactly what it is about to do, lets them stop it, proves afterward that
it did what it promised, and can put everything back.** Speed is nearly irrelevant. Confidence
is everything.

A second, quieter anxiety: **history is being deleted right now.** Claude Code auto-deletes
transcripts older than 30 days by default. Users often discover this after losing something.
The archival feature exists because of that clock.

### Jobs to be done

1. "I'm reorganizing my folders and I don't want to lose my AI history." (the mover)
2. "What history do I even have, and what's about to expire?" (inventory)
3. "Keep all of it, forever, somewhere safe." (archive)
4. "I'm killing this folder but its history belongs with that other one." (re-associate)
5. "Something is already broken. Tell me what." (diagnostics)

## 3. Feature and function breakdown

The engine is one shared core. The CLI and the GUI are two front ends over the identical core.

### v1 features: the mover (shipping as CLI first)

| ID | Feature | What it does |
|----|---------|-------------|
| F1 | Move engine | Relocates the folder. Same-volume moves are an atomic rename. Refuses if the destination already exists. Detects and refuses git worktrees, which break if moved. |
| F2 | Discovery / scan | Finds every place on disk that references the project by path. |
| F3 | Mapping and disambiguation | Works out which session history belongs to which folder. This is genuinely ambiguous sometimes (see below) and the tool refuses to guess. |
| F4 | Plan / dry-run | Produces a complete diff of every single change it intends to make, and writes nothing. |
| F5 | Backup and rollback | Snapshots every file it will touch, before touching anything. One command restores. |
| F6 | Apply engine | Executes the plan. Rewrites are byte-preserving except at the intended edits. |
| F7 | Verification | After applying, proves every promised postcondition actually holds. |
| F8 | Store adapters | One adapter per fragile on-disk format. Claude Code only in v1; other AI CLIs are deferred. |
| F9 | Safety and idempotency | Detects a running process holding the files. Re-running is a no-op. Hard-fails on any format it does not recognize, rather than guessing. |
| F10 | CLI | `doctor`, `scan`, `plan`, `apply`, `verify`, `rollback`. |
| F11 | **GUI** | **This is you. v2.** |
| F12 | Reporting | A machine-readable record of everything that happened. |

### v1.1 features: inventory, retention, re-associate

| ID | Feature | What it does |
|----|---------|-------------|
| F13 | **Inventory** (`list`) | Enumerates every project Claude has state for. Per project: session count, total size, **oldest and newest transcript age in days** (so the 30-day deletion cliff is visible), all the linked artifacts, and a health flag: `OK`, `STALE` (the folder is gone), or `UNRESOLVED` (we cannot tell what folder this belonged to). |
| F14 | **Archive** (`archive`) | Copies all history out to a durable user-chosen folder before it gets auto-deleted. Incremental (content-hashed, so re-running copies only what changed). Can install a hook that auto-archives each session as it ends. Can raise the retention setting as a safety net. |
| F15 | **Re-associate** (`associate`) | Takes project A's history and makes it belong to project B, and/or exports a portable copy of it into B. Works even when A's folder no longer exists on disk. |

### The three states that make this hard

These are the moments where a naive tool loses data, and where your design earns its keep:

- **AMBIGUOUS attribution.** A history's stored path does not match the folder being moved,
  because the folder was renamed earlier, or because two clones of the same repo both claim it.
  **The tool must stop and make the human choose.** It must never guess. Real example: a user
  moved a fork, but the history's stored path was the *base* repo's path, and a separate clone
  of the base still existed on disk. Whose sessions are these? Only the human knows.
- **STALE.** The project's folder no longer exists, but its history does.
- **UNRESOLVED.** A directory of session history whose owning project cannot be recovered at
  all. Roughly a third of directories on a real machine were in this state. They cannot be
  silently dropped, because dropping them is data loss by omission.

## 4. The CLI surface: a reference model, not a layout

This is what the engine can do. **It is emphatically not a proposed navigation structure.**

| Command | Meaning |
|---------|---------|
| `doctor` | "Is anything already broken?" Read-only health check of the whole machine. |
| `scan` | "What state exists for this project?" Read-only. |
| `plan` | "Show me exactly what you would change." Writes nothing. |
| `apply` | "Do it." Backs up first, then executes, then verifies. |
| `verify` | "Prove the result is correct." |
| `rollback` | "Undo it." Restores from the backup. |

Exit codes: `0` success, `2` a guard tripped and it refused, `3` verification failed, `4` it saw
a file format it did not recognize and refused to touch it.

**Read that table as a safety ritual, not a menu.** `plan` exists so the user can see the diff
before it happens. `verify` exists so they can prove it happened correctly. `rollback` exists so
they can undo it. A GUI with six equal top-level tabs would let a user reach `apply` without
ever passing through `plan`, which would destroy the entire reason the tool is shaped this way.

The interesting design question is: **what is the right shape for that ritual in a GUI, where
you have progressive disclosure, real diffs, live previews, and direct manipulation that a
terminal cannot offer?**

## 5. What a plan actually contains

This is the payload your primary screen has to render. For one project move, the plan lists:

- the folder move itself (source, destination)
- a rename of the encoded session-history directory
- for each transcript file: the exact number of replacements to be made, by category. A real
  single-project move produced **1,467 exact-field rewrites, 588 path-prefix rewrites, and 27
  alternate-separator rewrites**, across two files
- config key changes
- global history line edits
- plugin state directory renames
- and critically, **what will deliberately NOT change**: things that merely *look* like the
  project name but are not paths. A package name like `my-lib@0.2.1` (10 occurrences) and branch
  names like `my-lib_dev-*` (55 occurrences) must come through **byte-identical**. Proving the
  non-changes is as important to user trust as showing the changes.

That last point is worth dwelling on. The tool's core claim is *surgical precision*: it changes
exactly the path references and nothing else. A design that only shows what will change is
telling half the story. **Showing what is deliberately being left alone is what makes the tool
believable.**

Scale, so you can design for real data rather than a toy: a single transcript can be ~9 MB;
thousands of sessions across dozens of projects is normal.

## 6. Hard constraints your design must respect

These are non-negotiable. They come from the approved architecture.

1. **AC-25, the parity rule (the big one).** *The GUI, given the same project, renders the same
   set of planned changes as the CLI produces for that project.* Both front ends consume
   **identical plan objects** from the shared core. The GUI cannot invent an operation the core
   does not have, and cannot show a different result than the CLI would. **You are designing a
   renderer and a controller over a fixed data model, not a new product.** Every capability in
   Section 3 is available to you. Nothing beyond them is.
2. **Nothing is ever written without a plan being shown first.** No silent writes. Ever.
3. **A backup exists before the first byte is written.** Rollback must always be reachable.
4. **The tool refuses rather than guesses.** On ambiguity, on an unknown file format, on a
   destination collision, on a git worktree: it stops and asks. Your design needs a first-class,
   dignified visual language for *refusal* and for *asking the human to decide*. This is not an
   error state to be styled as a red toast. It is the tool working correctly, and it will happen
   often.
5. **Zero network. Zero LLM calls.** Everything is local and deterministic. Same input, same
   output, every time. There is no cloud account, no sync, no telemetry, no sign-in. Do not
   design any.
6. **Desktop app** (Tauri 2 + React), Windows first, macOS and Linux after. Offline, local-first.

## 7. Explicitly open: where you should push

Everything below is undecided. Have opinions.

- **The overall shape.** Is this a wizard? An inspector? A file-manager-like two-pane view? A
  dashboard that leads into a focused task flow? Nobody has decided.
- **How to render a plan** so that a diff of ~2,000 mechanical replacements across huge files is
  *comprehensible* rather than terrifying. Nobody wants to scroll 2,000 diff hunks. What is the
  right level of aggregation, and how does the user drill from a confident summary down to the
  literal bytes when they want to?
- **How to make "nothing else changed" visible and trustworthy.** See Section 5.
- **The consent moment.** What does the user actually click to authorize a destructive,
  irreversible-feeling operation, and how does the UI earn that click?
- **Per-item toggles.** The original requirement mentions letting the user selectively include or
  exclude individual planned changes. Is that a good idea, or does it create incoherent
  half-migrations? Argue either way.
- **The ambiguity decision UI.** A human must attribute an ambiguous history to a fork, a base,
  or both. This is the hardest human moment in the product. What does it look like?
- **The inventory view**, with the 30-day expiry clock visible and honest. This may be the real
  front door of the app: the screen you land on. It is also the one that surfaces STALE and
  UNRESOLVED projects, which is how users discover problems they did not know they had.
- **Archive setup**, which is a one-time configuration with an ongoing background behavior. How
  does an app communicate "I am quietly protecting your history" without nagging?
- **Rollback as an affordance.** It exists, it is safe, and users will not trust it until they
  see it. Should the app make it visible and inviting rather than hiding it as an emergency exit?
- **Tone.** This tool touches irreplaceable personal work. Should the interface be clinical and
  instrument-like (a diagnostic tool, an oscilloscope), calm and reassuring (a careful
  librarian), or something else? Justify it.

## 8. Out of scope for v2

Do not design these:

- Anything requiring a network connection, an account, or a cloud service.
- Cross-machine sync.
- A general-purpose backup product.
- A cost or usage analytics dashboard. (Every existing tool in this space is a usage/cost lens.
  CPM deliberately is not. Its differentiator is the plain "what exists and is it healthy" view,
  which nothing else provides.)
- Editing or rendering the *contents* of AI conversations as a reading experience. Session
  transcripts are cargo here, not content. (A read-only archive browser is a legitimate
  adjacent idea, but it is not the product.)
- Support for AI CLIs other than Claude Code. Deferred behind an adapter boundary.

## 9. What to deliver

Concepts and rationale. Screens, flows, and the reasoning behind them. Where you make a
trade-off, say what you traded and why. Where the constraints in Section 6 fight your instincts,
say so explicitly rather than quietly violating them, because a design that breaks parity with
the core cannot be built.

Push on Section 7. That is where the product is genuinely undecided, and where a good design
idea changes what gets built.
