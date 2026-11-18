---
id: S-05
title: "Desktop app store - find and repair Claude desktop app state that names a moved or deleted folder"
type: spec
status: proposed
created: 2026-11-18
updated: 2026-11-18
target-release: unassigned (v1.x candidate)
linked-release: none
linked-plan: none
ac-count: 12
ac-range: AC-66..AC-77
requires-human-review: true
origin: "A project folder renamed on 2026-11-11 left Claude desktop app records pointing at the old path after the ~/.claude state had been repointed. Maintainer asked for quality-of-life tooling for this state on 2026-11-18."
---

# S-05: Desktop app store

## Problem

The Claude desktop app runs Claude Code sessions from its Code tab. It keeps its own records about those sessions and folders under `%APPDATA%\Claude\`, separate from `~/.claude`. `awt` covers only `~/.claude` today. A folder move that `awt` repairs completely can therefore still leave the desktop app pointing at the old path.

The residue observed on the maintainer's machine after one folder rename, counted by hand on 2026-11-18:

| Record | Count | Visible effect |
|---|---|---|
| Archived session records whose `cwd` names the old folder | 4 | None while archived. An unarchived one would try to start in a folder that no longer exists. |
| An entry in the sidebar's folder list naming the old path | 1 | The sidebar shows a heading for the old folder, with "No active sessions" under it. |
| Old-path entries in two permission and worktree caches | 3 | None observed. |

The sidebar also shows a heading for every folder in its folder list, including folders with no active session, when one display flag is on. Users have no way to remove a single stale heading from inside the app.

## Data model, as observed

Observed on Windows, desktop app version 6.6395, on 2026-11-18. The app does not publish this format. Every key below is an observation, not a contract, and the adapter must refuse a shape it does not recognize.

| File | Key or field | Holds |
|---|---|---|
| `%APPDATA%\Claude\claude-code-sessions\<account-id>\<org-id>\local_<uuid>.json` | `cwd`, `originCwd`, `isArchived` | One record per Code-tab session: the folder it runs in, the folder it started in, and whether it is archived. |
| `%APPDATA%\Claude\claude_desktop_config.json` | `preferences.epitaxyPrefs["code-projects-order.<account-id>"]` | The sidebar's folder list. A folder that is a GitHub clone appears as `owner/repo`. Any other folder appears as its absolute path. A folder that no longer exists can no longer be read for a remote, so it appears as a path. |
| same file | `preferences.epitaxyPrefs["dframe-code-sections"][<scope>].sections[]` | One record per sidebar section. The built-in Sessions section has `kind: "sessions"`, and its `show.emptyGroups` flag controls whether empty folder headings appear. |
| same file | `epitaxy-perm-mode-acks-v2.<account-id>`, `epitaxy-folder-permission-mode-v2.<account-id>` | Per-folder permission choices, keyed or valued by absolute path. |
| `%APPDATA%\Claude\git-worktrees.json` | path values | A cache of git repository and worktree locations. |
| `%APPDATA%\Claude\Local Storage\leveldb\` | binary | Browser storage for the app's interface. It also contains folder paths. Out of scope: `awt` must never read or write it in this spec. |

**The running app owns `claude_desktop_config.json`.** It holds the file's contents in memory and writes them back. On 2026-11-13 a hand edit to this file was overwritten within seconds while the app ran. Every write in this spec therefore requires that no desktop app process is running.

**The store of record for sidebar group membership is not settled.** On 2026-11-18 the config file listed membership for 3 of 8 custom sections, while the app reported sessions in others. Part of that state probably lives in Local Storage. This spec does not touch group membership.

## Phase 1: read only

These criteria add no write path. They follow the v0.1.0 precedent: no write code ships before an honesty checkpoint passes.

- **AC-66.** `awt doctor` reports a `desktop.sessions` finding for each desktop session record whose `cwd` or `originCwd` names a folder that does not exist. Each finding says whether the record is archived.
- **AC-67.** `awt doctor` reports a `desktop.folders` finding for each path entry in the sidebar folder list whose folder does not exist. It ignores `owner/repo` entries.
- **AC-68.** `awt scan <path>` lists every desktop record that names `<path>`, across every file in the data model table except Local Storage.
- **AC-69.** When `%APPDATA%\Claude\` does not exist, the adapter reports that the desktop app is not present and exits as it would with no findings. It is not an error.
- **AC-70.** The adapter parses each file only to validate its shape, as every other store does. A file whose shape differs from the data model table produces a refusal that names the file and the unexpected key. It never produces a guess.
- **AC-71. Honesty checkpoint.** On the maintainer's machine, the counts from AC-66 and AC-67 match counts derived by hand at the time of the run. As with the v0.1.0 checkpoint, this is a procedure, not a fixed number. The 2026-11-18 counts in the Problem section are a historical record, not a target.

## Phase 2: writes, gated

Phase 2 starts only after AC-71 passes.

- **AC-72.** Every command that writes a desktop file refuses while any desktop app process is running, with a nonzero exit and a message telling the user to quit the app fully, including from the system tray. The check looks for the app's executable, not for lock files. The live-session guard's blind spot (a session the guard cannot see) does not apply here, because the refusal covers the whole app.
- **AC-73.** `awt apply` for a folder move also rewrites `cwd` and `originCwd` in the moved folder's desktop session records, by literal byte splice, with the same backup, verify and rollback as every other store.
- **AC-74.** `awt apply` renames the moved folder's path entry in the sidebar folder list. It leaves an `owner/repo` entry alone, because the app derives that entry from the repository, not the path.
- **AC-75.** A new command removes path entries for folders that no longer exist from the sidebar folder list. It runs as a dry run unless the user passes an explicit flag to write. Working name: `awt repair --desktop-folders`.
- **AC-76.** An acceptance run on a scratch copy of `%APPDATA%\Claude\` shows that the app, started after the write, keeps the change and shows no heading for a removed folder. If the app restores the entry from another store, the run records that and AC-75 is withdrawn.
- **AC-77.** All writes in this spec keep every existing invariant: UTF-8 only, case-insensitive path comparison, no re-serialization, and no edit to any record that belongs to a different folder.

## Open decisions for the maintainer

- **D1. Display settings.** Should `awt` also flip sidebar display flags, such as `show.emptyGroups` on the Sessions section? That is a quality-of-life change, not path repair. It fits the original vision of easy workspace tools, but it widens `awt` from "state about projects" to "app preferences". Recommendation: no for now. The app's own filter menu already offers the setting.
- **D2. Version pinning.** Should the adapter record the desktop app version it last validated against and refuse to write on a newer version until a maintainer re-validates? Recommendation: yes, because the format is unpublished and changes without notice.
- **D3. Release.** Should this be its own v1.x release, or ride with P11, the parked adapter work? Recommendation: its own release, because the desktop app is a Claude surface, not another agent.

## Out of scope

- Local Storage and any other browser storage under `%APPDATA%\Claude\`.
- Sidebar group membership and section layout. The app exposes these to agents through its own tools while it runs, which is safer than editing files.
- macOS and Linux paths for the desktop app. These follow the same CI-2 gate as the rest of `awt`.
