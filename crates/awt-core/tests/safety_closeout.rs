//! S-04 safety closeout, phases 17.2-17.4: the three Critical findings from the 2026-07-30
//! adversarial audit, each independently verified against source before these tests were
//! written. Every test here failed before its fix.
//!
//! AC-54: rollback of a directory rename backed up only top-level `*.jsonl`, then recursively
//! deleted the renamed directory - unbacked sidecars (memory files, tool results, nested
//! anything) were destroyed BY THE UNDO, while rollback reported success. The 5/5
//! byte-identical proof was real but its denominator was the manifest, not the tree.
//!
//! AC-55: `build_plan` never required the source folder to exist, and apply wrapped the folder
//! move in `if fs.exists(from)` - a missing source was silently skipped, recorded as applied,
//! and exited 0 with Claude state rewritten toward a destination no folder occupies.
//!
//! AC-56: any read or parse failure of `settings.json` became an empty object, which the next
//! settings write serialized over the user's file.

use awt_core::apply::{apply, apply_verified, ApplyOpts};
use awt_core::error::AwtError;
use awt_core::fs::{FileSystem, MemoryFileSystem};
use awt_core::model::{Change, Move, Scope};
use awt_core::plan::{build_plan, Collision, PlanOpts};
use awt_core::rollback::rollback;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const HOME: &str = "/h";

fn opts() -> PlanOpts {
    PlanOpts {
        recursive: false,
        on_collision: Collision::Refuse,
        force: false,
        move_folder: true,
        scope: Scope::Standard,
    }
}

fn mv() -> Move {
    Move {
        src_abs: r"E:\Projects\A".into(),
        dst_abs: r"E:\Projects\B".into(),
    }
}

/// A project-state directory that looks like a real one: a transcript at the top level, plus
/// the sidecars Claude Code actually creates - a nested memory file, a binary tool result, and
/// a nested transcript. None of the sidecars are backed up by the old top-level-jsonl-only
/// snapshot, which is exactly the point.
fn seed_with_sidecars(fs: &MemoryFileSystem) {
    let d = "/h/.claude/projects/E--Projects-A";
    fs.write(
        Path::new(&format!("{d}/s.jsonl")),
        b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
    )
    .unwrap();
    fs.write(
        Path::new(&format!("{d}/memory/notes.md")),
        b"# irreplaceable notes\nno path references here\n",
    )
    .unwrap();
    fs.write(
        Path::new(&format!("{d}/tool-results/blob.bin")),
        &[0u8, 159, 146, 150, 255, 1, 2, 3],
    )
    .unwrap();
    fs.write(
        Path::new(&format!("{d}/nested/deep/extra.jsonl")),
        b"{\"cwd\":\"E:\\\\Projects\\\\A\",\"note\":\"nested transcript\"}\n",
    )
    .unwrap();
    // The real folder being moved.
    fs.write(Path::new("E:/Projects/A/src/main.rs"), b"fn main() {}\n")
        .unwrap();
}

/// Every file under `root`, path -> bytes. The unit of proof for AC-54 is the TREE, not the
/// manifest: comparing complete maps is what catches a file that silently vanished.
fn tree(fs: &MemoryFileSystem, root: &str) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        for child in fs.read_dir(&dir).unwrap_or_default() {
            if fs.is_dir(&child) {
                stack.push(child);
            } else {
                let key = child.to_string_lossy().replace('\\', "/");
                out.insert(key, fs.read(&child).unwrap());
            }
        }
    }
    out
}

// =========================================================================================
// AC-54: rollback restores the complete tree
// =========================================================================================

#[test]
fn rollback_restores_the_complete_tree_including_unbacked_sidecars() {
    let fs = MemoryFileSystem::new();
    seed_with_sidecars(&fs);
    let before = tree(&fs, "/h/.claude/projects/E--Projects-A");
    assert_eq!(before.len(), 4, "seed sanity: transcript + three sidecars");

    let plan = build_plan(&fs, Path::new(HOME), &mv(), &opts()).unwrap();
    apply(&plan, &fs, Path::new("/backup"), "AC54").unwrap();
    rollback(Path::new("/backup/awt-AC54/manifest.json"), &fs).unwrap();

    let after = tree(&fs, "/h/.claude/projects/E--Projects-A");
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>(),
        "rollback must restore the complete file set - a missing key here is a file the undo destroyed"
    );
    for (path, bytes) in &before {
        assert_eq!(
            &after[path], bytes,
            "{path} must be byte-identical after rollback"
        );
    }
    assert!(
        !fs.exists(Path::new("/h/.claude/projects/E--Projects-B")),
        "the renamed dir must not linger as an orphan"
    );
}

#[test]
fn auto_rollback_after_midapply_failure_preserves_sidecars() {
    let fs = MemoryFileSystem::new();
    seed_with_sidecars(&fs);
    fs.write(Path::new("/h/.claude.json"), br#"{"projects":{}}"#)
        .unwrap();
    let before = tree(&fs, "/h/.claude/projects/E--Projects-A");

    // Sabotage: an extra splice whose anchor does not occur. Apply pass 1 renames the project
    // dir; pass 2 hits this change, count-check refuses, and auto-rollback runs with the dir
    // already renamed - the exact window in which the old code destroyed sidecars.
    let mut plan = build_plan(&fs, Path::new(HOME), &mv(), &opts()).unwrap();
    plan.changes.push(Change::RewriteJsonArrayValue {
        path: PathBuf::from("/h/.claude.json"),
        from: "\"NO-SUCH-ANCHOR\"".into(),
        to: "\"X\"".into(),
        expected: 1,
    });
    let opts_v = ApplyOpts {
        run_id: "AC54B".into(),
        auto_rollback: true,
        force: false,
    };
    apply_verified(&plan, &fs, Path::new("/backup"), &opts_v)
        .expect_err("the sabotaged change must fail the apply");

    let after = tree(&fs, "/h/.claude/projects/E--Projects-A");
    assert_eq!(
        before, after,
        "auto-rollback after a mid-apply failure must leave the tree byte-identical"
    );
}

// =========================================================================================
// AC-55: a missing source cannot succeed
// =========================================================================================

#[test]
fn plan_refuses_when_the_source_folder_does_not_exist() {
    let fs = MemoryFileSystem::new();
    // Claude state exists for A, but the folder itself does not.
    fs.write(
        Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
        b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
    )
    .unwrap();
    let err = build_plan(&fs, Path::new(HOME), &mv(), &opts())
        .expect_err("a folder move whose source does not exist must refuse at plan time");
    assert!(
        matches!(err, AwtError::SourceMissing(_)),
        "expected SourceMissing, got {err:?}"
    );
}

#[test]
fn associate_still_plans_without_a_source_folder() {
    // The guard must apply ONLY to real folder moves. associate (move_folder: false) exists
    // precisely for gone folders and must keep working.
    let fs = MemoryFileSystem::new();
    fs.write(
        Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
        b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
    )
    .unwrap();
    let mut o = opts();
    o.move_folder = false;
    build_plan(&fs, Path::new(HOME), &mv(), &o)
        .expect("associate-style plans must not require the source folder");
}

#[test]
fn apply_hard_fails_if_the_source_vanishes_between_plan_and_apply() {
    let fs = MemoryFileSystem::new();
    seed_with_sidecars(&fs);
    let plan = build_plan(&fs, Path::new(HOME), &mv(), &opts()).unwrap();

    // The source disappears after planning (concurrent deletion, eject, typo'd rerun...).
    fs.remove_dir_all(Path::new("E:/Projects/A")).unwrap();

    let err = apply(&plan, &fs, Path::new("/backup"), "AC55")
        .expect_err("a folder move whose source vanished must fail, not silently skip");
    assert!(
        matches!(err, AwtError::SourceMissing(_)),
        "expected SourceMissing, got {err:?}"
    );
}

#[test]
fn verify_fails_when_the_folder_move_did_not_actually_happen() {
    let fs = MemoryFileSystem::new();
    seed_with_sidecars(&fs);
    let plan = build_plan(&fs, Path::new(HOME), &mv(), &opts()).unwrap();
    apply(&plan, &fs, Path::new("/backup"), "AC55V").unwrap();

    // Sabotage the postcondition: the destination folder disappears after the move.
    fs.remove_dir_all(Path::new("E:/Projects/B")).unwrap();

    let manifest =
        awt_core::backup::Manifest::load(&fs, Path::new("/backup/awt-AC55V/manifest.json"))
            .unwrap();
    let results = awt_core::verify::verify(&fs, Path::new(HOME), &mv(), Some(&manifest)).unwrap();
    assert!(
        results.iter().any(|r| !r.ok && r.check.contains("folder")),
        "verify must assert the folder postcondition when the plan moved a folder: {results:?}"
    );
}

// =========================================================================================
// AC-56: settings writes fail closed
// =========================================================================================

const SETTINGS: &str = "/h/.claude/settings.json";

fn settings_bytes(fs: &MemoryFileSystem) -> Vec<u8> {
    fs.read(Path::new(SETTINGS)).unwrap()
}

#[test]
fn set_retention_refuses_malformed_settings_and_touches_nothing() {
    let fs = MemoryFileSystem::new();
    let broken = b"{ this is not json";
    fs.write(Path::new(SETTINGS), broken).unwrap();
    let err = awt_core::settings::set_retention(&fs, Path::new(HOME), 3650, false)
        .expect_err("malformed settings must refuse, not be replaced");
    assert!(
        matches!(err, AwtError::UnrecognizedFormat(_)),
        "expected UnrecognizedFormat, got {err:?}"
    );
    assert_eq!(
        settings_bytes(&fs),
        broken,
        "the file must be byte-identical"
    );
}

#[test]
fn set_retention_refuses_invalid_utf8_settings() {
    let fs = MemoryFileSystem::new();
    let broken = vec![0xFF, 0xFE, b'{', b'}'];
    fs.write(Path::new(SETTINGS), &broken).unwrap();
    let err = awt_core::settings::set_retention(&fs, Path::new(HOME), 3650, false)
        .expect_err("invalid UTF-8 settings must refuse");
    assert!(
        matches!(err, AwtError::UnrecognizedFormat(_)),
        "got {err:?}"
    );
    assert_eq!(settings_bytes(&fs), broken);
}

#[test]
fn set_retention_refuses_a_non_object_root() {
    let fs = MemoryFileSystem::new();
    let broken = b"[1,2,3]";
    fs.write(Path::new(SETTINGS), broken).unwrap();
    let err = awt_core::settings::set_retention(&fs, Path::new(HOME), 3650, false)
        .expect_err("a non-object settings root must refuse");
    assert!(
        matches!(err, AwtError::UnrecognizedFormat(_)),
        "got {err:?}"
    );
    assert_eq!(settings_bytes(&fs), broken);
}

#[test]
fn install_hook_refuses_malformed_settings_and_touches_nothing() {
    let fs = MemoryFileSystem::new();
    let broken = b"{ nope";
    fs.write(Path::new(SETTINGS), broken).unwrap();
    let err = awt_core::settings::install_session_end_hook(
        &fs,
        Path::new(HOME),
        Path::new("/usr/bin/awt"),
        Path::new("/archive"),
    )
    .expect_err("hook install over malformed settings must refuse");
    assert!(
        matches!(err, AwtError::UnrecognizedFormat(_)),
        "got {err:?}"
    );
    assert_eq!(settings_bytes(&fs), broken);
}

#[test]
fn uninstall_hook_refuses_malformed_settings_and_touches_nothing() {
    let fs = MemoryFileSystem::new();
    let broken = b"{ nope";
    fs.write(Path::new(SETTINGS), broken).unwrap();
    let err = awt_core::settings::uninstall_session_end_hook(&fs, Path::new(HOME))
        .expect_err("hook uninstall over malformed settings must refuse");
    assert!(
        matches!(err, AwtError::UnrecognizedFormat(_)),
        "got {err:?}"
    );
    assert_eq!(settings_bytes(&fs), broken);
}

/// Only genuinely-missing may initialize. This is the guard against over-correcting into a
/// tool that cannot run on a fresh machine.
#[test]
fn a_missing_settings_file_still_initializes() {
    let fs = MemoryFileSystem::new();
    awt_core::settings::set_retention(&fs, Path::new(HOME), 3650, false)
        .expect("a missing settings file is a valid empty state");
    let v: serde_json::Value = serde_json::from_slice(&settings_bytes(&fs)).unwrap();
    assert_eq!(v["cleanupPeriodDays"], 3650);
}

/// Unrelated keys survive a settings write byte-for-byte at the value level.
#[test]
fn settings_writes_preserve_unrelated_keys() {
    let fs = MemoryFileSystem::new();
    fs.write(
        Path::new(SETTINGS),
        br#"{"model":"opus","permissions":{"allow":["Bash(ls:*)"]},"cleanupPeriodDays":30}"#,
    )
    .unwrap();
    awt_core::settings::set_retention(&fs, Path::new(HOME), 3650, false).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&settings_bytes(&fs)).unwrap();
    assert_eq!(v["cleanupPeriodDays"], 3650);
    assert_eq!(v["model"], "opus", "unrelated key lost");
    assert_eq!(
        v["permissions"]["allow"][0], "Bash(ls:*)",
        "nested unrelated key lost"
    );
}
