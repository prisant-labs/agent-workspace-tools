//! AC-61: reparse points (junctions/symlinks) inside operated-on trees.
//!
//! Policy, and why it differs by operation: MUTATION walks (backup, merge) REFUSE a link -
//! following one lets a snapshot or delete escape the tree, and skipping one means the
//! backup silently covers less than the directory holds; neither is tolerable mid-write.
//! The ARCHIVE walk SKIPS links - archive is a best-effort protective sweep across every
//! project, and one link should not abort protecting the rest.

use awt_core::apply::apply;
use awt_core::error::AwtError;
use awt_core::fs::{walk_files_strict, FileSystem, MemoryFileSystem};
use awt_core::model::Move;
use awt_core::plan::{build_plan, PlanOpts};
use std::path::Path;

const HOME: &str = "/h";

fn opts() -> PlanOpts {
    PlanOpts {
        force: false,
        move_folder: true,
    }
}

#[test]
fn strict_walk_refuses_a_reparse_point() {
    let fs = MemoryFileSystem::new();
    fs.write(Path::new("/tree/normal.txt"), b"x").unwrap();
    fs.write(Path::new("/tree/linked/inside.txt"), b"y")
        .unwrap();
    fs.mark_reparse(Path::new("/tree/linked"));

    let err = walk_files_strict(&fs, Path::new("/tree"))
        .expect_err("a junction inside a mutation walk must refuse");
    assert!(
        matches!(&err, AwtError::Locked(m) if m.contains("reparse")),
        "got {err:?}"
    );
}

#[test]
fn plan_refuses_when_the_project_state_dir_contains_a_junction() {
    let fs = MemoryFileSystem::new();
    fs.write(
        Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
        b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
    )
    .unwrap();
    fs.write(
        Path::new("/h/.claude/projects/E--Projects-A/linked/pulled-in.txt"),
        b"outside content",
    )
    .unwrap();
    fs.mark_reparse(Path::new("/h/.claude/projects/E--Projects-A/linked"));
    fs.write(Path::new("E:/Projects/A/.keep"), b"x").unwrap();

    let mv = Move {
        src_abs: r"E:\Projects\A".into(),
        dst_abs: r"E:\Projects\B".into(),
    };
    // Refused at PLAN time: a guard, exit 2, nothing attempted.
    let err = build_plan(&fs, Path::new(HOME), &mv, &opts())
        .expect_err("a junction in the tree must refuse at plan time");
    assert!(
        matches!(&err, AwtError::Locked(m) if m.contains("reparse")),
        "got {err:?}"
    );
    assert!(fs.exists(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl")));
    assert!(!fs.exists(Path::new("/h/.claude/projects/E--Projects-B")));
}

/// TOCTOU defense-in-depth: a junction created AFTER planning is still caught by the
/// snapshot walk at apply time, before any write.
#[test]
fn apply_refuses_a_junction_created_after_planning() {
    let fs = MemoryFileSystem::new();
    fs.write(
        Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
        b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
    )
    .unwrap();
    fs.write(Path::new("E:/Projects/A/.keep"), b"x").unwrap();
    let mv = Move {
        src_abs: r"E:\Projects\A".into(),
        dst_abs: r"E:\Projects\B".into(),
    };
    let plan = build_plan(&fs, Path::new(HOME), &mv, &opts()).unwrap();

    // The link appears between plan and apply.
    fs.write(
        Path::new("/h/.claude/projects/E--Projects-A/linked/pulled-in.txt"),
        b"outside content",
    )
    .unwrap();
    fs.mark_reparse(Path::new("/h/.claude/projects/E--Projects-A/linked"));

    let err = apply(&plan, &fs, Path::new("/backup"), "AC61T")
        .expect_err("the snapshot walk must catch a link created after planning");
    assert!(
        matches!(&err, AwtError::Locked(m) if m.contains("reparse")),
        "got {err:?}"
    );
    // Refused before any write.
    assert!(fs.exists(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl")));
    assert!(!fs.exists(Path::new("/h/.claude/projects/E--Projects-B")));
    assert!(fs.exists(Path::new("E:/Projects/A/.keep")));
}

#[test]
fn archive_skips_a_reparse_subtree_and_archives_the_rest() {
    let fs = MemoryFileSystem::new();
    fs.write(
        Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
        b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
    )
    .unwrap();
    // A junction whose target would drag outside content into the archive.
    fs.write(
        Path::new("/h/.claude/projects/E--Projects-A/linked/secret.txt"),
        b"outside content",
    )
    .unwrap();
    fs.mark_reparse(Path::new("/h/.claude/projects/E--Projects-A/linked"));

    let rep = awt_core::archive::archive_project(
        &fs,
        Path::new(HOME),
        r"E:\Projects\A",
        &awt_core::archive::ArchiveOpts {
            archive_dir: std::path::PathBuf::from("/archive"),
            render: false,
            run_token: "AC61A".into(),
        },
    )
    .unwrap();

    assert!(
        rep.copied >= 1,
        "the real transcript must still be archived"
    );
    let archived: Vec<_> = walk_files_strict(&fs, Path::new("/archive"))
        .unwrap()
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    assert!(
        archived.iter().any(|p| p.ends_with("s.jsonl")),
        "transcript archived: {archived:?}"
    );
    assert!(
        !archived.iter().any(|p| p.contains("secret")),
        "content behind the junction must NOT be pulled into the archive: {archived:?}"
    );
}
