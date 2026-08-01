//! Codex adversarial-review finding (2026-07-31, high): rollback validated manifest
//! entries and backup integrity INLINE during the restore loop, after the project folder
//! and state directories had already been renamed back. A corrupted backup or malformed
//! entry detected at position N stranded a half-restored filesystem, and a content entry
//! with a missing/empty sha256 restored unverified bytes silently.
//!
//! Contract under test: rollback runs a complete fail-closed preflight - every entry
//! shape-valid, every content backup present and hash-verified - BEFORE the first rename,
//! write, or removal. A bad manifest or bad backup leaves the filesystem byte-identical.

use awt_core::apply::apply;
use awt_core::error::AwtError;
use awt_core::fs::{FileSystem, MemoryFileSystem};
use awt_core::model::Move;
use awt_core::plan::{build_plan, PlanOpts};
use awt_core::rollback::rollback;
use std::path::Path;

const HOME: &str = "/h";
const MANIFEST: &str = "/backup/awt-T/manifest.json";

fn opts() -> PlanOpts {
    PlanOpts {
        force: false,
        move_folder: true,
    }
}

/// Seed a home + real folder, apply a move, and return the fs.
fn applied_move() -> MemoryFileSystem {
    let fs = MemoryFileSystem::new();
    fs.write(
        Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
        b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
    )
    .unwrap();
    fs.write(Path::new("E:/Projects/A/f.txt"), b"x").unwrap();
    let mv = Move {
        src_abs: r"E:\Projects\A".into(),
        dst_abs: r"E:\Projects\B".into(),
    };
    let plan = build_plan(&fs, Path::new(HOME), &mv, &opts()).unwrap();
    apply(&plan, &fs, Path::new("/backup"), "T").unwrap();
    fs
}

/// The post-apply facts that a REFUSED rollback must leave untouched.
fn assert_post_apply_state_untouched(fs: &MemoryFileSystem) {
    assert!(
        fs.exists(Path::new("E:/Projects/B/f.txt")),
        "moved folder must remain at dst"
    );
    assert!(
        !fs.exists(Path::new("E:/Projects/A/f.txt")),
        "src folder must not reappear"
    );
    assert!(
        fs.exists(Path::new("/h/.claude/projects/E--Projects-B/s.jsonl")),
        "renamed state dir must remain"
    );
    assert!(
        !fs.exists(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl")),
        "old state dir must not reappear"
    );
}

fn load_manifest(fs: &MemoryFileSystem) -> serde_json::Value {
    serde_json::from_slice(&fs.read(Path::new(MANIFEST)).unwrap()).unwrap()
}

fn save_manifest(fs: &MemoryFileSystem, v: &serde_json::Value) {
    fs.write(Path::new(MANIFEST), v.to_string().as_bytes())
        .unwrap();
}

#[test]
fn corrupted_backup_refuses_before_any_mutation() {
    let fs = applied_move();
    // Tamper the bytes of a real content backup so its recorded hash no longer matches.
    let m = load_manifest(&fs);
    let entry = m["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| !e["sha256"].as_str().unwrap_or_default().is_empty())
        .expect("a content entry exists");
    let backup_path = entry["backup"].as_str().unwrap().to_string();
    fs.write(Path::new(&backup_path), b"tampered bytes")
        .unwrap();

    let err = rollback(Path::new(MANIFEST), &fs).expect_err("corrupted backup must refuse");
    assert!(
        matches!(&err, AwtError::VerifyFailed(msg) if msg.contains("backup corrupted")),
        "got {err:?}"
    );
    // The refusal must land BEFORE the folder or state-dir renames, not mid-restore.
    assert_post_apply_state_untouched(&fs);
}

#[test]
fn content_entry_with_empty_sha_refuses_untouched() {
    let fs = applied_move();
    // Blank the hash on a content entry: restoring it would write unverified bytes.
    let mut m = load_manifest(&fs);
    let entries = m["entries"].as_array_mut().unwrap();
    let e = entries
        .iter_mut()
        .find(|e| !e["sha256"].as_str().unwrap_or_default().is_empty())
        .expect("a content entry exists");
    e["sha256"] = serde_json::Value::String(String::new());
    save_manifest(&fs, &m);

    let err = rollback(Path::new(MANIFEST), &fs)
        .expect_err("a content entry without a hash must refuse, not restore unverified bytes");
    assert!(
        matches!(&err, AwtError::UnrecognizedFormat(_)),
        "fail-closed as unrecognized format, got {err:?}"
    );
    assert_post_apply_state_untouched(&fs);
}

#[test]
fn malformed_late_entry_refuses_before_any_mutation() {
    let fs = applied_move();
    // Corrupt the LAST content entry's `original` field. Inline validation only reaches it
    // after every earlier entry was already restored; the preflight must catch it first.
    let mut m = load_manifest(&fs);
    let entries = m["entries"].as_array_mut().unwrap();
    let e = entries
        .iter_mut()
        .rev()
        .find(|e| !e["sha256"].as_str().unwrap_or_default().is_empty())
        .expect("a content entry exists");
    e["original"] = serde_json::Value::Number(12345.into());
    save_manifest(&fs, &m);

    let err = rollback(Path::new(MANIFEST), &fs).expect_err("malformed entry must refuse");
    assert!(
        matches!(&err, AwtError::UnrecognizedFormat(_)),
        "got {err:?}"
    );
    assert_post_apply_state_untouched(&fs);
}

#[test]
fn unknown_marker_prefix_refuses_untouched() {
    let fs = applied_move();
    // A marker this version does not recognize is a manifest from a different (or
    // tampered) producer; guessing at its semantics mid-restore is how data gets lost.
    let mut m = load_manifest(&fs);
    let entries = m["entries"].as_array_mut().unwrap();
    entries.push(serde_json::json!({
        "original": "/h/.claude/projects/E--Projects-A",
        "backup": "<future-marker v99>",
        "sha256": ""
    }));
    save_manifest(&fs, &m);

    let err = rollback(Path::new(MANIFEST), &fs).expect_err("unknown marker must refuse");
    assert!(
        matches!(&err, AwtError::UnrecognizedFormat(_)),
        "got {err:?}"
    );
    assert_post_apply_state_untouched(&fs);
}
