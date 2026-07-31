//! AC-58: the advertised-but-inert options are removed rather than left as guard bypasses.
//! Maintainer decision 2026-07-30 (option: remove all three).
//!
//! The behavioral half that stays testable in core: nested projects are now a hard REFUSAL,
//! not a warning. `--recursive` claimed "also move nested projects under src" while only
//! suppressing the warning; with the flag gone, the honest default is to refuse and tell the
//! user to move children first.

use awt_core::error::AwtError;
use awt_core::fs::{FileSystem, MemoryFileSystem};
use awt_core::model::Move;
use awt_core::plan::{build_plan, PlanOpts};
use std::path::Path;

fn opts() -> PlanOpts {
    PlanOpts {
        force: false,
        move_folder: true,
    }
}

#[test]
fn nested_projects_are_a_hard_refusal() {
    let fs = MemoryFileSystem::new();
    // Parent project with state, and a CHILD project under it with its own state.
    fs.write(
        Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
        b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
    )
    .unwrap();
    fs.write(
        Path::new("/h/.claude/projects/E--Projects-A-sub/t.jsonl"),
        b"{\"cwd\":\"E:\\\\Projects\\\\A\\\\sub\"}\n",
    )
    .unwrap();
    fs.write(Path::new("E:/Projects/A/sub/.keep"), b"x")
        .unwrap();

    let mv = Move {
        src_abs: r"E:\Projects\A".into(),
        dst_abs: r"E:\Projects\B".into(),
    };
    let err = build_plan(&fs, Path::new("/h"), &mv, &opts())
        .expect_err("a move that would break a nested project must refuse, not warn");
    match err {
        AwtError::NestedProjects(msg) => {
            assert!(
                msg.to_lowercase().contains("sub"),
                "the refusal must name the nested project: {msg}"
            );
        }
        other => panic!("expected NestedProjects, got {other:?}"),
    }
}

#[test]
fn a_project_with_no_nested_children_still_plans() {
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
    build_plan(&fs, Path::new("/h"), &mv, &opts()).expect("no nested children, plan succeeds");
}

/// The collision guard is now unconditional: there is no mode that bypasses it.
#[test]
fn destination_key_collision_always_refuses() {
    let fs = MemoryFileSystem::new();
    fs.write(
        Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
        b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
    )
    .unwrap();
    fs.write(Path::new("E:/Projects/A/.keep"), b"x").unwrap();
    // The destination already has a claude.json key.
    fs.write(
        Path::new("/h/.claude.json"),
        br#"{"projects":{"E:/Projects/A":{},"E:/Projects/B":{}}}"#,
    )
    .unwrap();
    let mv = Move {
        src_abs: r"E:\Projects\A".into(),
        dst_abs: r"E:\Projects\B".into(),
    };
    let err = build_plan(&fs, Path::new("/h"), &mv, &opts())
        .expect_err("a destination key collision must always refuse");
    assert!(
        matches!(err, AwtError::DestinationExists(_)),
        "expected DestinationExists, got {err:?}"
    );
}
