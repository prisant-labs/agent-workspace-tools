//! AR-10: a project folder with no readable transcript could not be moved or re-associated.
//!
//! `detect` found a project folder only through the transcript index, while `verify` looked the
//! folder up by its encoded name. A folder holding only `memory/`, only a session folder, or a
//! transcript with no `cwd` therefore stayed behind, and verification failed on it.
//!
//! The encoding is lossy, so the fix must not take another project's folder. Half of these tests
//! prove the folder follows the move. The other half prove a look-alike folder stays put.

use awt_core::apply::{apply_verified, ApplyOpts};
use awt_core::associate::{associate, AssociateOpts};
use awt_core::doctor::scan;
use awt_core::error::AwtError;
use awt_core::fs::{FileSystem, MemoryFileSystem};
use awt_core::model::{Change, Move};
use awt_core::plan::{build_plan, Plan, PlanOpts};
use awt_core::rollback::rollback;
use std::path::Path;

const HOME: &str = "/h";

fn write(fs: &MemoryFileSystem, path: &str, bytes: &[u8]) {
    fs.write(Path::new(path), bytes).unwrap();
}

fn cwd_line(cwd: &str) -> String {
    format!(
        "{{\"type\":\"user\",\"cwd\":\"{}\"}}\n",
        cwd.replace('\\', "\\\\")
    )
}

fn plan_move(fs: &MemoryFileSystem, src: &str, dst: &str) -> awt_core::error::Result<Plan> {
    let mv = Move {
        src_abs: src.into(),
        dst_abs: dst.into(),
    };
    let opts = PlanOpts {
        force: false,
        move_folder: true,
    };
    build_plan(fs, Path::new(HOME), &mv, &opts)
}

fn apply_opts() -> ApplyOpts {
    ApplyOpts {
        run_id: "T".into(),
        auto_rollback: true,
        force: false,
    }
}

fn run_move(fs: &MemoryFileSystem, src: &str, dst: &str) -> Plan {
    let plan = plan_move(fs, src, dst).unwrap();
    apply_verified(&plan, fs, Path::new("/backup"), &apply_opts()).unwrap();
    plan
}

fn exists(fs: &MemoryFileSystem, path: &str) -> bool {
    fs.exists(Path::new(path))
}

fn reassociate_only() -> AssociateOpts {
    AssociateOpts {
        reassociate: true,
        export: false,
        export_subdir: ".claude-sessions".into(),
        run_id: "T".into(),
    }
}

// ---------------------------------------------------------------------------
// The folder follows the project
// ---------------------------------------------------------------------------

#[test]
fn memory_follows_the_move_and_rollback_restores_it() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    let note = "/h/.claude/projects/E--Projects-A/memory/MEMORY.md";
    write(&fs, note, b"remember this");

    run_move(&fs, "E:\\Projects\\A", "E:\\Projects\\B");

    assert_eq!(
        fs.read(Path::new(
            "/h/.claude/projects/E--Projects-B/memory/MEMORY.md"
        ))
        .unwrap(),
        b"remember this"
    );
    assert!(!exists(&fs, note), "the old folder must not keep a copy");

    rollback(Path::new("/backup/awt-T/manifest.json"), &fs).unwrap();
    assert_eq!(fs.read(Path::new(note)).unwrap(), b"remember this");
    assert!(
        !fs.is_dir(Path::new("/h/.claude/projects/E--Projects-B")),
        "rollback must leave nothing at the new name"
    );
}

#[test]
fn a_folder_holding_only_a_session_folder_follows_the_move() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    write(
        &fs,
        "/h/.claude/projects/E--Projects-A/abc/subagents/agent-1.jsonl",
        b"{}\n",
    );

    run_move(&fs, "E:\\Projects\\A", "E:\\Projects\\B");

    assert!(exists(
        &fs,
        "/h/.claude/projects/E--Projects-B/abc/subagents/agent-1.jsonl"
    ));
}

#[test]
fn a_transcript_with_no_cwd_follows_the_move_unchanged() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    write(
        &fs,
        "/h/.claude/projects/E--Projects-A/s.jsonl",
        b"{\"type\":\"summary\"}\n",
    );

    run_move(&fs, "E:\\Projects\\A", "E:\\Projects\\B");

    assert_eq!(
        fs.read(Path::new("/h/.claude/projects/E--Projects-B/s.jsonl"))
            .unwrap(),
        b"{\"type\":\"summary\"}\n"
    );
}

#[test]
fn the_plan_says_when_a_folder_was_matched_by_name() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    write(&fs, "/h/.claude/projects/E--Projects-A/memory/M.md", b"m");

    let plan = plan_move(&fs, "E:\\Projects\\A", "E:\\Projects\\B").unwrap();

    assert!(
        plan.warnings
            .iter()
            .any(|w| w.contains("E--Projects-A") && w.contains("folder name")),
        "{:?}",
        plan.warnings
    );
}

#[test]
fn associate_moves_a_memory_only_folder() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/New/B/keep.txt", b"x");
    write(&fs, "/h/.claude/projects/E--Old-A/memory/M.md", b"m");

    associate(
        &fs,
        Path::new(HOME),
        "E:\\Old\\A",
        "E:\\New\\B",
        &reassociate_only(),
    )
    .unwrap();

    assert_eq!(
        fs.read(Path::new("/h/.claude/projects/E--New-B/memory/M.md"))
            .unwrap(),
        b"m"
    );
    assert!(!fs.is_dir(Path::new("/h/.claude/projects/E--Old-A")));
}

#[test]
fn a_differently_cased_path_still_finds_the_folder() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/New/B/keep.txt", b"x");
    write(&fs, "/h/.claude/projects/E--Old-A/memory/M.md", b"m");

    associate(
        &fs,
        Path::new(HOME),
        "e:\\old\\a",
        "E:\\New\\B",
        &reassociate_only(),
    )
    .unwrap();

    assert!(exists(&fs, "/h/.claude/projects/E--New-B/memory/M.md"));
    assert!(!fs.is_dir(Path::new("/h/.claude/projects/E--Old-A")));
}

#[test]
fn a_folder_whose_dead_paths_include_this_project_follows_the_move() {
    // Two recorded paths, both gone, one of them this project. The index cannot resolve the
    // folder, but its transcripts name this path, so the folder is this project's.
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/New/B/keep.txt", b"x");
    let dir = "/h/.claude/projects/E--Old-A";
    write(
        &fs,
        &format!("{dir}/a.jsonl"),
        cwd_line("E:\\Old\\A").as_bytes(),
    );
    write(
        &fs,
        &format!("{dir}/b.jsonl"),
        cwd_line("E:\\Old_A").as_bytes(),
    );

    associate(
        &fs,
        Path::new(HOME),
        "E:\\Old\\A",
        "E:\\New\\B",
        &reassociate_only(),
    )
    .unwrap();

    assert!(exists(&fs, "/h/.claude/projects/E--New-B/a.jsonl"));
    assert!(exists(&fs, "/h/.claude/projects/E--New-B/b.jsonl"));
}

#[test]
fn scan_reports_a_folder_matched_by_name() {
    let fs = MemoryFileSystem::new();
    write(&fs, "/h/.claude/projects/E--Projects-A/memory/M.md", b"m");

    let report = scan(&fs, Path::new(HOME), "E:\\Projects\\A").unwrap();

    assert!(
        report
            .hits
            .iter()
            .any(|h| h.store == "claude.projects" && h.detail.contains("folder name")),
        "{:?}",
        report.hits
    );
}

// ---------------------------------------------------------------------------
// A look-alike folder stays with its owner
// ---------------------------------------------------------------------------

/// `E:\foo\bar` and `E:\foo-bar` encode to the same folder name.
const SRC: &str = "E:\\foo\\bar";
const LOOKALIKE: &str = "/h/.claude/projects/E--foo-bar";

fn seed_source(fs: &MemoryFileSystem) {
    write(fs, "E:/foo/bar/f.txt", b"x");
}

fn assert_lookalike_stayed(fs: &MemoryFileSystem, plan: &Plan, file: &str, bytes: &[u8]) {
    assert_eq!(
        fs.read(Path::new(&format!("{LOOKALIKE}/{file}"))).unwrap(),
        bytes,
        "the other project's file must stay, byte for byte"
    );
    assert!(
        !fs.is_dir(Path::new("/h/.claude/projects/E--baz")),
        "nothing of the other project's may reach the new name"
    );
    assert!(exists(fs, "E:/baz/f.txt"), "the project itself still moves");
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.contains("E--foo-bar") && w.contains("left")),
        "the plan must say the folder was left alone: {:?}",
        plan.warnings
    );
}

#[test]
fn a_folder_the_index_gives_to_another_project_stays() {
    let fs = MemoryFileSystem::new();
    seed_source(&fs);
    let line = cwd_line("E:\\foo-bar");
    write(&fs, &format!("{LOOKALIKE}/s.jsonl"), line.as_bytes());

    let plan = run_move(&fs, SRC, "E:\\baz");

    assert_lookalike_stayed(&fs, &plan, "s.jsonl", line.as_bytes());
}

#[test]
fn a_folder_whose_dead_paths_are_all_other_projects_stays() {
    let fs = MemoryFileSystem::new();
    seed_source(&fs);
    let line = cwd_line("E:\\foo-bar");
    write(&fs, &format!("{LOOKALIKE}/a.jsonl"), line.as_bytes());
    write(
        &fs,
        &format!("{LOOKALIKE}/b.jsonl"),
        cwd_line("E:\\foo_bar").as_bytes(),
    );

    let plan = run_move(&fs, SRC, "E:\\baz");

    assert_lookalike_stayed(&fs, &plan, "a.jsonl", line.as_bytes());
}

#[test]
fn a_folder_with_two_live_other_paths_stays() {
    // Genuinely ambiguous between two other projects, so it cannot be this project's to take.
    let fs = MemoryFileSystem::new();
    seed_source(&fs);
    write(&fs, "E:/foo-bar/k.txt", b"x");
    write(&fs, "E:/foo_bar/k.txt", b"x");
    let line = cwd_line("E:\\foo-bar");
    write(&fs, &format!("{LOOKALIKE}/a.jsonl"), line.as_bytes());
    write(
        &fs,
        &format!("{LOOKALIKE}/b.jsonl"),
        cwd_line("E:\\foo_bar").as_bytes(),
    );

    let plan = run_move(&fs, SRC, "E:\\baz");

    assert_lookalike_stayed(&fs, &plan, "a.jsonl", line.as_bytes());
}

#[test]
fn a_memory_folder_stays_when_claude_json_knows_a_lookalike_project() {
    let fs = MemoryFileSystem::new();
    seed_source(&fs);
    write(&fs, &format!("{LOOKALIKE}/memory/M.md"), b"whose is this?");
    write(
        &fs,
        "/h/.claude.json",
        br#"{"projects":{"E:/foo/bar":{},"E:/foo-bar":{}}}"#,
    );

    let plan = run_move(&fs, SRC, "E:\\baz");

    assert_lookalike_stayed(&fs, &plan, "memory/M.md", b"whose is this?");
}

#[test]
fn a_memory_folder_stays_when_a_transcript_elsewhere_names_a_lookalike_path() {
    let fs = MemoryFileSystem::new();
    seed_source(&fs);
    write(&fs, &format!("{LOOKALIKE}/memory/M.md"), b"whose is this?");
    write(
        &fs,
        "/h/.claude/projects/E--elsewhere/s.jsonl",
        cwd_line("E:\\foo-bar").as_bytes(),
    );

    let plan = run_move(&fs, SRC, "E:\\baz");

    assert_lookalike_stayed(&fs, &plan, "memory/M.md", b"whose is this?");
}

#[test]
fn another_spelling_of_the_same_path_is_not_a_lookalike() {
    // The key differs from the typed path only in case and separators, so it is this project.
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    write(&fs, "/h/.claude/projects/E--Projects-A/memory/M.md", b"m");
    write(
        &fs,
        "/h/.claude.json",
        br#"{"projects":{"e:/projects/a":{}}}"#,
    );

    run_move(&fs, "E:\\Projects\\A", "E:\\Projects\\B");

    assert!(exists(&fs, "/h/.claude/projects/E--Projects-B/memory/M.md"));
}

#[test]
fn a_project_with_a_transcript_never_uses_the_name_match() {
    // AC-6: the transcript's cwd decides. Its folder has a different name, and a memory-only
    // folder at the encoded name is not touched by this store's plan.
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    write(
        &fs,
        "/h/.claude/projects/E--Renamed/s.jsonl",
        cwd_line("E:\\Projects\\A").as_bytes(),
    );

    let hits = scan(&fs, Path::new(HOME), "E:\\Projects\\A").unwrap().hits;
    let dirs: Vec<_> = hits
        .iter()
        .filter(|h| h.store == "claude.projects")
        .collect();

    assert_eq!(dirs.len(), 1);
    assert!(dirs[0].target.ends_with("E--Renamed"));
    assert!(!dirs[0].detail.contains("folder name"));
}

// ---------------------------------------------------------------------------
// Verification still catches a move that did not happen
// ---------------------------------------------------------------------------

#[test]
fn verify_fails_if_a_name_matched_folder_did_not_move_and_names_the_check() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    write(&fs, "/h/.claude/projects/E--Projects-A/memory/M.md", b"m");

    let mut plan = plan_move(&fs, "E:\\Projects\\A", "E:\\Projects\\B").unwrap();
    plan.changes
        .retain(|c| !matches!(c, Change::RenameDir { .. }));

    let err = apply_verified(&plan, &fs, Path::new("/backup"), &apply_opts()).unwrap_err();

    assert!(matches!(err, AwtError::VerifyFailed(_)), "{err:?}");
    assert!(
        err.to_string().contains("new projects dir exists"),
        "the message must name the failed check: {err}"
    );
}

// ---------------------------------------------------------------------------
// Merging into a folder that already exists
// ---------------------------------------------------------------------------

#[test]
fn memory_merges_into_an_existing_destination_folder() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    write(&fs, "/h/.claude/projects/E--Projects-A/memory/A.md", b"a");
    write(&fs, "/h/.claude/projects/E--Projects-B/memory/B.md", b"b");

    run_move(&fs, "E:\\Projects\\A", "E:\\Projects\\B");

    assert!(exists(&fs, "/h/.claude/projects/E--Projects-B/memory/A.md"));
    assert!(exists(&fs, "/h/.claude/projects/E--Projects-B/memory/B.md"));
    assert!(!fs.is_dir(Path::new("/h/.claude/projects/E--Projects-A")));
}

#[test]
fn a_file_on_both_sides_of_a_merge_refuses_at_plan_and_changes_nothing() {
    // MEMORY.md is a fixed name, so this collision is the normal case for two projects with
    // memory, not a freak one. Refusing at plan keeps the dry-run promise: apply would fail.
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    let a = "/h/.claude/projects/E--Projects-A/memory/MEMORY.md";
    let b = "/h/.claude/projects/E--Projects-B/memory/MEMORY.md";
    write(&fs, a, b"a");
    write(&fs, b, b"b");

    let err = plan_move(&fs, "E:\\Projects\\A", "E:\\Projects\\B").unwrap_err();

    assert!(matches!(err, AwtError::DestinationExists(_)), "{err:?}");
    assert!(err.to_string().contains("MEMORY.md"), "{err}");
    assert_eq!(fs.read(Path::new(a)).unwrap(), b"a");
    assert_eq!(fs.read(Path::new(b)).unwrap(), b"b");
}

// ---------------------------------------------------------------------------
// A rename that does not change the encoded name
// ---------------------------------------------------------------------------

#[test]
fn a_rename_with_the_same_encoded_name_rewrites_in_place() {
    // Renaming foo_bar to foo-bar is ordinary, and the folder name does not change. A merge of
    // the folder into itself failed on its own first file.
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/foo_bar/f.txt", b"x");
    let transcript = "/h/.claude/projects/E--foo-bar/s.jsonl";
    let before = cwd_line("E:\\foo_bar");
    write(&fs, transcript, before.as_bytes());

    let plan = run_move(&fs, "E:\\foo_bar", "E:\\foo-bar");

    assert!(!plan.changes.iter().any(|c| matches!(
        c,
        Change::RenameDir { from, .. } | Change::MergeDir { from, .. }
            if from.ends_with("E--foo-bar")
    )));
    let after = String::from_utf8(fs.read(Path::new(transcript)).unwrap()).unwrap();
    assert!(after.contains("E:\\\\foo-bar"), "{after}");
    assert!(exists(&fs, "E:/foo-bar/f.txt"));

    rollback(Path::new("/backup/awt-T/manifest.json"), &fs).unwrap();
    assert_eq!(fs.read(Path::new(transcript)).unwrap(), before.as_bytes());
    assert!(exists(&fs, "E:/foo_bar/f.txt"));
}
