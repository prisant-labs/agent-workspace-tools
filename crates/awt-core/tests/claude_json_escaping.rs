//! Regression guard for AR-01: `~/.claude.json` stores Windows paths JSON-escaped
//! (`E:\\a\\b`), but the plan derives its anchor from the PARSED value, which is
//! unescaped (`E:\a\b`). The rewrite is a literal byte splice, so an unescaped anchor
//! matches nothing, the count check sees `expected 1, live 0`, and apply fails closed.
//!
//! Found by the 2026-07-28 manual acceptance run. It reached release-candidate stage
//! because every existing assertion looked at parsed values or `plan` output, both of
//! which show the unescaped form and therefore look correct. These tests assert on the
//! RAW BYTES of the rewritten file, which is the only layer where the defect exists.
//!
//! This also wires up `test/fixtures/claude-json-variants/`, which sat referenced by no
//! test at all while containing exactly the shape that reproduces the bug.

use awt_core::apply::apply;
use awt_core::fs::{FileSystem, MemoryFileSystem};
use awt_core::model::Move;
use awt_core::plan::{build_plan, PlanOpts};
use std::path::Path;

const HOME: &str = "/home/.claude-fixture";
const OLD: &str = r"E:\Projects\Sample Repos\demo-notes-editor";
const NEW: &str = r"E:\Projects\demo-labs\demo-notes-editor-pro";

/// Raw, JSON-escaped forms as they appear in the file's bytes.
const OLD_RAW: &str = r#""E:\\Projects\\Sample Repos\\demo-notes-editor""#;
const NEW_RAW: &str = r#""E:\\Projects\\demo-labs\\demo-notes-editor-pro""#;

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

/// Seeds a memory FS with the `claude-json-variants` fixture as `<home>/.claude.json`,
/// plus a source folder for `MoveTree` to move.
fn seed() -> MemoryFileSystem {
    let fs = MemoryFileSystem::new();
    let fixture = repo_root().join("test/fixtures/claude-json-variants/claude.json");
    let bytes = std::fs::read(&fixture)
        .unwrap_or_else(|e| panic!("fixture {} unreadable: {e}", fixture.display()));

    // Sanity-check the fixture still contains the escaped shape under test. If someone
    // "tidies" the fixture into forward slashes, this test would silently stop covering
    // the defect it exists for.
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(
        text.contains(OLD_RAW),
        "fixture no longer contains the escaped path {OLD_RAW}; this test would pass vacuously"
    );

    fs.write(Path::new(HOME).join(".claude.json").as_path(), &bytes)
        .unwrap();
    fs.write(
        Path::new(r"E:\Projects\Sample Repos\demo-notes-editor\.keep"),
        b"x",
    )
    .unwrap();
    fs
}

fn plan_and_apply(fs: &MemoryFileSystem) {
    let mv = Move {
        src_abs: OLD.into(),
        dst_abs: NEW.into(),
    };
    let opts = PlanOpts {
        force: false,
        move_folder: true,
    };
    let plan = build_plan(fs, Path::new(HOME), &mv, &opts).unwrap();
    apply(&plan, fs, Path::new("/backup"), "AR01").expect(
        "apply must succeed; a VerifyFailed 'expected 1, live 0' here is AR-01 reproducing",
    );
}

fn claude_json_text(fs: &MemoryFileSystem) -> String {
    let bytes = fs
        .read(Path::new(HOME).join(".claude.json").as_path())
        .unwrap();
    String::from_utf8(bytes).unwrap()
}

/// The headline case: apply must complete, and the raw bytes must carry the new path in
/// its escaped form with no trace of the old one.
#[test]
fn apply_rewrites_escaped_paths_in_claude_json() {
    let fs = seed();
    plan_and_apply(&fs);
    let text = claude_json_text(&fs);

    assert!(
        !text.contains(OLD_RAW),
        "old escaped path survived the rewrite in the raw bytes"
    );
    assert!(
        text.contains(NEW_RAW),
        "new escaped path is absent from the raw bytes; the splice did not happen"
    );
}

/// The file must still be valid JSON afterwards, and the rewrite must be visible through
/// the parsed view too. A byte splice that produced the right bytes but broke the escaping
/// would pass the test above and fail here.
#[test]
fn rewritten_claude_json_still_parses_with_correct_values() {
    let fs = seed();
    plan_and_apply(&fs);
    let v: serde_json::Value = serde_json::from_str(&claude_json_text(&fs))
        .expect("claude.json must remain valid JSON after the rewrite");

    let projects = v.get("projects").and_then(|p| p.as_object()).unwrap();
    assert!(
        projects.contains_key(NEW),
        "parsed projects keys should contain the new path, got: {:?}",
        projects.keys().collect::<Vec<_>>()
    );
    assert!(
        !projects.contains_key(OLD),
        "parsed projects keys still contain the old path"
    );

    let arr = v
        .get("githubRepoPaths")
        .and_then(|g| g.get("owner/demo-notes-editor"))
        .and_then(|a| a.as_array())
        .expect("githubRepoPaths entry should survive the rewrite");
    assert!(
        arr.iter().any(|e| e.as_str() == Some(NEW)),
        "githubRepoPaths value was not rewritten to the new path: {arr:?}"
    );
}

/// Unrelated entries must be untouched. A fix that over-escapes or rewrites too broadly
/// would corrupt neighbours, and the fixture deliberately contains a `D:\\Cloud-Work-PP`
/// entry plus a forward-slash variant of it that must both survive verbatim.
#[test]
fn unrelated_entries_are_byte_identical_after_rewrite() {
    let fs = seed();
    let before = claude_json_text(&fs);
    plan_and_apply(&fs);
    let after = claude_json_text(&fs);

    for untouched in [
        r#""D:\\Cloud-Work-PP""#,
        r#""d:/cloud-work-pp""#,
        r#""E:\\Projects\\Chrome - Bookmark Autosort""#,
    ] {
        assert_eq!(
            before.matches(untouched).count(),
            after.matches(untouched).count(),
            "unrelated entry {untouched} changed occurrence count"
        );
    }
}

/// The forward-slash convention must keep working. Real files mix both (measured
/// 2026-07-28: 71 of 79 project keys forward-slash, 8 backslash), and a fix that escapes
/// correctly but breaks the unescaped path would trade one blocker for another.
#[test]
fn forward_slash_paths_still_rewrite() {
    let fs = MemoryFileSystem::new();
    let raw =
        r#"{"projects":{"E:/tmp/probe":{}},"githubRepoPaths":{"owner/probe":["E:/tmp/probe"]}}"#;
    fs.write(
        Path::new(HOME).join(".claude.json").as_path(),
        raw.as_bytes(),
    )
    .unwrap();
    fs.write(Path::new("E:/tmp/probe/.keep"), b"x").unwrap();

    let mv = Move {
        src_abs: "E:/tmp/probe".into(),
        dst_abs: "E:/tmp/probe-moved".into(),
    };
    let opts = PlanOpts {
        force: false,
        move_folder: true,
    };
    let plan = build_plan(&fs, Path::new(HOME), &mv, &opts).unwrap();
    apply(&plan, &fs, Path::new("/backup"), "AR01FWD").unwrap();

    let text = claude_json_text(&fs);
    assert!(text.contains(r#""E:/tmp/probe-moved""#), "got: {text}");
    assert!(!text.contains(r#""E:/tmp/probe""#), "got: {text}");
}
