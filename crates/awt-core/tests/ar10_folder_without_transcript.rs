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
fn associate_moves_a_memory_only_folder_for_a_recorded_path() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/New/B/keep.txt", b"x");
    write(&fs, "/h/.claude/projects/E--Old-A/memory/M.md", b"m");
    write(&fs, "/h/.claude.json", br#"{"projects":{"E:/Old/A":{}}}"#);

    let report = associate(
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
    assert!(
        report.warnings.iter().any(|w| w.contains("folder name")),
        "the warning must reach whoever writes: {:?}",
        report.warnings
    );
}

#[test]
fn a_differently_cased_path_still_finds_the_folder() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/New/B/keep.txt", b"x");
    write(&fs, "/h/.claude/projects/E--Old-A/memory/M.md", b"m");
    write(&fs, "/h/.claude.json", br#"{"projects":{"E:/Old/A":{}}}"#);

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
fn a_history_line_is_enough_to_know_the_path() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/New/B/keep.txt", b"x");
    write(&fs, "/h/.claude/projects/E--Old-A/memory/M.md", b"m");
    write(
        &fs,
        "/h/.claude/history.jsonl",
        b"{\"display\":\"x\",\"project\":\"E:\\\\Old\\\\A\"}\n",
    );

    associate(
        &fs,
        Path::new(HOME),
        "E:\\Old\\A",
        "E:\\New\\B",
        &reassociate_only(),
    )
    .unwrap();

    assert!(exists(&fs, "/h/.claude/projects/E--New-B/memory/M.md"));
}

#[test]
fn residue_of_an_older_location_does_not_stop_the_folder_following() {
    // Both recorded paths are gone, and the second encodes differently. So it is an older home
    // of this project, not a look-alike, and the folder is this project's.
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
        cwd_line("E:\\Older\\Z").as_bytes(),
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
fn a_nested_transcript_naming_this_path_is_enough_to_know_it() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/New/B/keep.txt", b"x");
    write(
        &fs,
        "/h/.claude/projects/E--Old-A/abc/subagents/a.jsonl",
        cwd_line("E:\\Old\\A").as_bytes(),
    );

    associate(
        &fs,
        Path::new(HOME),
        "E:\\Old\\A",
        "E:\\New\\B",
        &reassociate_only(),
    )
    .unwrap();

    assert!(exists(
        &fs,
        "/h/.claude/projects/E--New-B/abc/subagents/a.jsonl"
    ));
}

#[test]
fn scan_reports_a_folder_matched_by_name() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
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

#[test]
fn apply_carries_the_warning_into_its_report() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    write(&fs, "/h/.claude/projects/E--Projects-A/memory/M.md", b"m");

    let plan = plan_move(&fs, "E:\\Projects\\A", "E:\\Projects\\B").unwrap();
    let report = apply_verified(&plan, &fs, Path::new("/backup"), &apply_opts()).unwrap();

    assert!(report.warnings.iter().any(|w| w.contains("folder name")));
    assert!(report.to_json()["warnings"][0]
        .as_str()
        .unwrap()
        .contains("folder name"));
}

#[test]
fn a_project_with_a_resolving_transcript_gets_no_name_match_warning() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    write(
        &fs,
        "/h/.claude/projects/E--Projects-A/s.jsonl",
        cwd_line("E:\\Projects\\A").as_bytes(),
    );

    let plan = plan_move(&fs, "E:\\Projects\\A", "E:\\Projects\\B").unwrap();

    assert!(
        !plan.warnings.iter().any(|w| w.contains("folder name")),
        "{:?}",
        plan.warnings
    );
}

// ---------------------------------------------------------------------------
// A name alone is not enough
// ---------------------------------------------------------------------------

#[test]
fn a_path_nothing_records_does_not_take_a_folder_with_its_name() {
    // A typo: foo.bar for the real foo-bar. Nothing records foo.bar, so the old refusal stands.
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/foo-bar/k.txt", b"x");
    let note = "/h/.claude/projects/E--foo-bar/memory/M.md";
    write(&fs, note, b"foo-bar's memory");
    write(&fs, "E:/New/B/keep.txt", b"x");

    let hits = scan(&fs, Path::new(HOME), "E:\\foo.bar").unwrap().hits;
    assert!(hits.is_empty(), "{hits:?}");

    let err = associate(
        &fs,
        Path::new(HOME),
        "E:\\foo.bar",
        "E:\\New\\B",
        &reassociate_only(),
    )
    .unwrap_err();

    assert!(matches!(err, AwtError::Locked(_)), "{err:?}");
    assert_eq!(fs.read(Path::new(note)).unwrap(), b"foo-bar's memory");
}

#[test]
fn a_file_at_the_encoded_name_is_not_a_folder() {
    let fs = MemoryFileSystem::new();
    seed_source(&fs);
    write(&fs, LOOKALIKE, b"a file, not a folder");

    let plan = plan_move(&fs, SRC, "E:\\baz").unwrap();

    assert!(!plan
        .changes
        .iter()
        .any(|c| matches!(c, Change::RenameDir { from, .. } if from.ends_with("E--foo-bar"))));
    assert!(plan.warnings.iter().all(|w| !w.contains("E--foo-bar")));
}

#[test]
fn two_dead_lookalike_owners_refuse_rather_than_guess() {
    // Both spellings are gone and both encode to this folder, so it could be either's.
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/New/B/keep.txt", b"x");
    let dir = "/h/.claude/projects/E--Old-A";
    let (a, b) = (cwd_line("E:\\Old\\A"), cwd_line("E:\\Old_A"));
    write(&fs, &format!("{dir}/a.jsonl"), a.as_bytes());
    write(&fs, &format!("{dir}/b.jsonl"), b.as_bytes());

    let err = associate(
        &fs,
        Path::new(HOME),
        "E:\\Old\\A",
        "E:\\New\\B",
        &reassociate_only(),
    )
    .unwrap_err();

    assert!(matches!(err, AwtError::Ambiguous(_)), "{err:?}");
    assert_eq!(
        fs.read(Path::new(&format!("{dir}/a.jsonl"))).unwrap(),
        a.as_bytes()
    );
    assert_eq!(
        fs.read(Path::new(&format!("{dir}/b.jsonl"))).unwrap(),
        b.as_bytes()
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
fn a_memory_folder_stays_when_history_names_a_lookalike_path() {
    let fs = MemoryFileSystem::new();
    seed_source(&fs);
    write(&fs, &format!("{LOOKALIKE}/memory/M.md"), b"whose is this?");
    write(
        &fs,
        "/h/.claude/history.jsonl",
        b"{\"display\":\"x\",\"project\":\"E:\\\\foo_bar\"}\n",
    );

    let plan = run_move(&fs, SRC, "E:\\baz");

    assert_lookalike_stayed(&fs, &plan, "memory/M.md", b"whose is this?");
}

#[test]
fn a_memory_folder_stays_when_a_key_has_a_trailing_separator() {
    let fs = MemoryFileSystem::new();
    seed_source(&fs);
    write(&fs, &format!("{LOOKALIKE}/memory/M.md"), b"whose is this?");
    write(
        &fs,
        "/h/.claude.json",
        br#"{"projects":{"E:\\foo_bar\\":{}}}"#,
    );

    let plan = run_move(&fs, SRC, "E:\\baz");

    assert_lookalike_stayed(&fs, &plan, "memory/M.md", b"whose is this?");
}

#[test]
fn a_memory_folder_stays_when_github_repo_paths_names_a_lookalike() {
    let fs = MemoryFileSystem::new();
    seed_source(&fs);
    write(&fs, &format!("{LOOKALIKE}/memory/M.md"), b"whose is this?");
    write(
        &fs,
        "/h/.claude.json",
        br#"{"githubRepoPaths":{"o/r":["E:\\foo_bar"]}}"#,
    );

    let plan = run_move(&fs, SRC, "E:\\baz");

    assert_lookalike_stayed(&fs, &plan, "memory/M.md", b"whose is this?");
}

#[test]
fn a_lookalike_differing_also_in_case_still_counts() {
    let fs = MemoryFileSystem::new();
    seed_source(&fs);
    write(&fs, &format!("{LOOKALIKE}/memory/M.md"), b"whose is this?");
    write(&fs, "/h/.claude.json", br#"{"projects":{"e:/FOO_BAR":{}}}"#);

    let plan = run_move(&fs, SRC, "E:\\baz");

    assert_lookalike_stayed(&fs, &plan, "memory/M.md", b"whose is this?");
}

#[test]
fn a_nested_transcript_naming_another_path_keeps_the_folder_with_it() {
    let fs = MemoryFileSystem::new();
    seed_source(&fs);
    let line = cwd_line("E:\\foo_bar");
    write(
        &fs,
        &format!("{LOOKALIKE}/abc/subagents/a.jsonl"),
        line.as_bytes(),
    );

    let plan = run_move(&fs, SRC, "E:\\baz");

    assert_lookalike_stayed(&fs, &plan, "abc/subagents/a.jsonl", line.as_bytes());
}

#[test]
fn an_indexed_owner_with_a_different_encoding_keeps_its_folder() {
    // The transcript says E:\elsewhere, which does not encode to this name. AC-6: its cwd decides.
    let fs = MemoryFileSystem::new();
    seed_source(&fs);
    let line = cwd_line("E:\\elsewhere");
    write(&fs, &format!("{LOOKALIKE}/s.jsonl"), line.as_bytes());

    let plan = run_move(&fs, SRC, "E:\\baz");

    assert_lookalike_stayed(&fs, &plan, "s.jsonl", line.as_bytes());
}

#[test]
fn two_live_owners_with_a_different_encoding_keep_their_folder() {
    let fs = MemoryFileSystem::new();
    seed_source(&fs);
    write(&fs, "E:/one/k.txt", b"x");
    write(&fs, "E:/two/k.txt", b"x");
    let line = cwd_line("E:\\one");
    write(&fs, &format!("{LOOKALIKE}/a.jsonl"), line.as_bytes());
    write(
        &fs,
        &format!("{LOOKALIKE}/b.jsonl"),
        cwd_line("E:\\two").as_bytes(),
    );

    let plan = run_move(&fs, SRC, "E:\\baz");

    assert_lookalike_stayed(&fs, &plan, "a.jsonl", line.as_bytes());
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

#[test]
fn verify_fails_if_a_merge_did_not_happen() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    write(&fs, "/h/.claude/projects/E--Projects-A/memory/A.md", b"a");
    write(&fs, "/h/.claude/projects/E--Projects-B/memory/B.md", b"b");

    let mut plan = plan_move(&fs, "E:\\Projects\\A", "E:\\Projects\\B").unwrap();
    plan.changes
        .retain(|c| !matches!(c, Change::MergeDir { .. }));

    let err = apply_verified(&plan, &fs, Path::new("/backup"), &apply_opts()).unwrap_err();

    assert!(matches!(err, AwtError::VerifyFailed(_)), "{err:?}");
    assert!(err.to_string().contains("old projects dir gone"), "{err}");
}

// ---------------------------------------------------------------------------
// Refusing at plan what apply could not do
// ---------------------------------------------------------------------------

#[test]
fn a_transcript_with_a_broken_line_refuses_at_plan() {
    // Verification requires every line to parse, so apply could only fail and roll back.
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    write(
        &fs,
        "/h/.claude/projects/E--Projects-A/s.jsonl",
        b"this is not json\n",
    );

    let err = plan_move(&fs, "E:\\Projects\\A", "E:\\Projects\\B").unwrap_err();

    assert!(matches!(err, AwtError::UnrecognizedFormat(_)), "{err:?}");
    assert!(err.to_string().contains("s.jsonl"), "{err}");
}

#[test]
fn a_destination_folder_owned_by_a_lookalike_refuses() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    write(&fs, "/h/.claude/projects/E--Projects-A/memory/A.md", b"a");
    let theirs = cwd_line("E:\\b_c");
    write(&fs, "/h/.claude/projects/E--b-c/s.jsonl", theirs.as_bytes());

    let err = plan_move(&fs, "E:\\Projects\\A", "E:\\b-c").unwrap_err();

    assert!(matches!(err, AwtError::DestinationExists(_)), "{err:?}");
    assert!(!exists(&fs, "/h/.claude/projects/E--b-c/memory/A.md"));
}

#[test]
fn a_nested_child_with_only_a_memory_folder_refuses_the_parent_move() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/child/f.txt", b"x");
    write(
        &fs,
        "/h/.claude/projects/E--Projects-A-child/memory/M.md",
        b"m",
    );
    write(
        &fs,
        "/h/.claude.json",
        br#"{"projects":{"E:/Projects/A/child":{}}}"#,
    );

    let err = plan_move(&fs, "E:\\Projects\\A", "E:\\Projects\\B").unwrap_err();

    assert!(matches!(err, AwtError::NestedProjects(_)), "{err:?}");
}

#[test]
fn a_file_where_the_merge_needs_a_folder_refuses_at_plan() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    write(&fs, "/h/.claude/projects/E--Projects-A/memory/A.md", b"a");
    write(&fs, "/h/.claude/projects/E--Projects-B/memory", b"a file");

    let err = plan_move(&fs, "E:\\Projects\\A", "E:\\Projects\\B").unwrap_err();

    assert!(matches!(err, AwtError::DestinationExists(_)), "{err:?}");
}

#[test]
fn a_collision_that_appears_after_plan_refuses_and_changes_nothing() {
    // Rollback would remove the merged path from B, so reaching that state would delete B's file.
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    let a = "/h/.claude/projects/E--Projects-A/memory/MEMORY.md";
    let b = "/h/.claude/projects/E--Projects-B/memory/MEMORY.md";
    write(&fs, a, b"a");
    write(
        &fs,
        "/h/.claude/projects/E--Projects-B/memory/other.md",
        b"o",
    );

    let plan = plan_move(&fs, "E:\\Projects\\A", "E:\\Projects\\B").unwrap();
    write(&fs, b, b"b arrived after plan");

    let err = apply_verified(&plan, &fs, Path::new("/backup"), &apply_opts()).unwrap_err();

    assert!(matches!(err, AwtError::DestinationExists(_)), "{err:?}");
    assert_eq!(fs.read(Path::new(a)).unwrap(), b"a");
    assert_eq!(fs.read(Path::new(b)).unwrap(), b"b arrived after plan");
    assert!(exists(&fs, "E:/Projects/A/f.txt"));
}

#[test]
fn a_state_folder_that_is_itself_a_link_refuses() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    write(&fs, "/h/.claude/projects/E--Projects-A/memory/A.md", b"a");
    fs.mark_reparse(Path::new("/h/.claude/projects/E--Projects-A"));

    let err = plan_move(&fs, "E:\\Projects\\A", "E:\\Projects\\B").unwrap_err();

    assert!(
        matches!(&err, AwtError::Locked(m) if m.contains("reparse")),
        "{err:?}"
    );
}

#[test]
fn a_memory_folder_beside_a_differently_named_transcript_folder_refuses_at_plan() {
    // Both folders are this project's and both would take the new name. Before this, the memory
    // folder stayed behind with exit 0.
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    write(
        &fs,
        "/h/.claude/projects/E--Renamed/s.jsonl",
        cwd_line("E:\\Projects\\A").as_bytes(),
    );
    write(&fs, "/h/.claude/projects/E--Projects-A/memory/M.md", b"m");

    let err = plan_move(&fs, "E:\\Projects\\A", "E:\\Projects\\B").unwrap_err();

    assert!(matches!(err, AwtError::DestinationExists(_)), "{err:?}");
}

#[test]
fn a_lower_case_drive_folder_with_a_transcript_moves() {
    // The line-count check swapped encoded names by exact text, so it looked in the wrong place.
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/Projects/A/f.txt", b"x");
    write(
        &fs,
        "/h/.claude/projects/e--Projects-A/s.jsonl",
        cwd_line("E:\\Projects\\A").as_bytes(),
    );

    run_move(&fs, "E:\\Projects\\A", "E:\\Projects\\B");

    assert!(exists(&fs, "/h/.claude/projects/E--Projects-B/s.jsonl"));
}

#[test]
fn a_same_name_rename_differing_in_case_stays_in_place() {
    let fs = MemoryFileSystem::new();
    write(&fs, "E:/foo_bar/f.txt", b"x");
    write(
        &fs,
        "/h/.claude/projects/e--foo-bar/s.jsonl",
        cwd_line("E:\\foo_bar").as_bytes(),
    );

    let plan = plan_move(&fs, "E:\\foo_bar", "E:\\foo-bar").unwrap();

    assert!(!plan
        .changes
        .iter()
        .any(|c| matches!(c, Change::RenameDir { .. } | Change::MergeDir { .. })));
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
