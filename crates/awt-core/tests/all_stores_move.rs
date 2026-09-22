//! Closes the four criteria the S-01 sign-off aid rates "Test-thin": AC-5, AC-8, AC-13, AC-14.
//!
//! The four share one root cause, and it is in the suite rather than in the code: every
//! existing test seeds ONE store and asserts on that store. AC-5's stated gap is literally
//! "no single test seeds all stores at once", so four separate files would leave it open no
//! matter how thorough each was. This file therefore builds one home holding all four WRITING
//! stores, runs one move through it, and hangs each criterion's missing assertion off that
//! single scenario.
//!
//! The fifth registered store, `sweep.unknown`, is deliberately absent. It is report-only and
//! structurally incapable of emitting a change (see the doc comment on `stores/sweep.rs`), so
//! no plan can ever list it. "Every store" in AC-5 means every store that can write.
//!
//! Anti-vacuity: every postcondition is paired with a precondition asserting that the thing it
//! looks for is present BEFORE the move. Without that pairing, "the old directory is gone" and
//! "no old path survives in history" both pass trivially against an empty fixture. That is the
//! exact failure mode that silently emptied two assertions in
//! `unrelated_entries_are_byte_identical_after_rewrite` until a presence check was added, so
//! the guard is applied here by construction rather than bolted on afterwards.

use awt_core::apply::apply;
use awt_core::fs::{FileSystem, MemoryFileSystem};
use awt_core::model::Move;
use awt_core::plan::{build_plan, Plan, PlanOpts};
use awt_core::stores::plugin_state::state_hash;
use serde_json::Value;
use std::path::Path;

const HOME: &str = "/h";
const OLD: &str = r"E:\Projects\A";
const NEW: &str = r"E:\Projects\B";
const OTHER: &str = r"E:\Projects\Other";

const OLD_ENC: &str = "/h/.claude/projects/E--Projects-A";
const NEW_ENC: &str = "/h/.claude/projects/E--Projects-B";

/// The `project` field as it appears in the RAW bytes of `history.jsonl`, JSON-escaped.
const OLD_IN_HISTORY: &str = r#""project":"E:\\Projects\\A""#;
const NEW_IN_HISTORY: &str = r#""project":"E:\\Projects\\B""#;
const OTHER_IN_HISTORY: &str = r#""project":"E:\\Projects\\Other""#;

/// Two `projects` entries plus one `githubRepoPaths` slug, all in the escaped Windows form
/// that `.claude.json` actually stores (the AR-01 shape).
const CLAUDE_JSON: &str = r#"{"projects":{"E:\\Projects\\A":{"allowedTools":["Bash"]},"E:\\Projects\\Other":{"allowedTools":[]}},"githubRepoPaths":{"owner/a":["E:\\Projects\\A"]}}"#;

const HISTORY: &str = concat!(
    r#"{"display":"hi","project":"E:\\Projects\\A","sessionId":"s1"}"#,
    "\n",
    r#"{"display":"yo","project":"E:\\Projects\\Other","sessionId":"s2"}"#,
    "\n"
);

const TRANSCRIPT: &str = concat!(r#"{"cwd":"E:\\Projects\\A"}"#, "\n");

/// One home holding all four writing stores, each referencing `OLD`, plus one unrelated
/// project (`OTHER`) in the two stores that can hold more than one entry. The unrelated
/// project is what makes "nothing was touched that was not listed" observable.
fn seed() -> MemoryFileSystem {
    let fs = MemoryFileSystem::new();

    // 1. claude.projects - the encoded transcript directory.
    fs.write(
        Path::new(OLD_ENC).join("s1.jsonl").as_path(),
        TRANSCRIPT.as_bytes(),
    )
    .unwrap();

    // 2. claude.json - per-project config plus a githubRepoPaths array value.
    fs.write(Path::new("/h/.claude.json"), CLAUDE_JSON.as_bytes())
        .unwrap();

    // 3. claude.history - never expires, so it outlives the transcripts.
    fs.write(Path::new("/h/.claude/history.jsonl"), HISTORY.as_bytes())
        .unwrap();

    // 4. plugin.state - directory suffix is sha256(path)[:16] over the backslash form.
    let dir = format!(
        "/h/.claude/plugins/data/codex/state/A-{}/state.json",
        state_hash(OLD)
    );
    fs.write(Path::new(&dir), br#"{"n":1}"#).unwrap();

    // The project folder itself, so the missing-source guard is satisfied.
    fs.write(Path::new("E:/Projects/A/f.txt"), b"x").unwrap();

    fs
}

fn plan_of(fs: &MemoryFileSystem) -> Plan {
    let mv = Move {
        src_abs: OLD.into(),
        dst_abs: NEW.into(),
    };
    let opts = PlanOpts {
        force: false,
        move_folder: true,
    };
    build_plan(fs, Path::new(HOME), &mv, &opts).expect("planning this home must succeed")
}

fn apply_move(fs: &MemoryFileSystem) {
    let plan = plan_of(fs);
    apply(&plan, fs, Path::new("/backup"), "ALLSTORES").expect("apply must succeed");
}

/// Every filesystem path the plan names, drawn from `to_json()` rather than from the `Change`
/// enum. `Change` carries no store id, so a store's participation can only be shown by the
/// path it touches; reading that through the public JSON contract also means this assertion
/// covers the shape the CLI and the v2 GUI both consume.
fn planned_paths(plan: &Plan) -> Vec<String> {
    let j = plan.to_json();
    let mut out = Vec::new();
    for c in j["changes"].as_array().expect("changes must be an array") {
        for key in ["path", "from", "to"] {
            if let Some(s) = c.get(key).and_then(|v| v.as_str()) {
                out.push(s.replace('\\', "/"));
            }
        }
    }
    out
}

fn read_text(fs: &MemoryFileSystem, p: &str) -> String {
    let bytes = fs
        .read(Path::new(p))
        .unwrap_or_else(|e| panic!("{p} unreadable: {e}"));
    String::from_utf8(bytes).expect("utf-8")
}

/// The keys of the `projects` object in `.claude.json`, read back through a full parse.
/// "Entry count" in AC-13 is a property of the parsed object, not of the raw bytes.
fn project_keys(fs: &MemoryFileSystem) -> Vec<String> {
    let text = read_text(fs, "/h/.claude.json");
    let v: Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("claude.json must still parse: {e}\n{text}"));
    v["projects"]
        .as_object()
        .expect("projects must be an object")
        .keys()
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------------------
// AC-5: the plan lists every store that references the project
// ---------------------------------------------------------------------------------------

/// The gap was never that a store went unplanned; it was that no fixture ever held more than
/// one store, so "the plan covers all of them" had never been observed as a single fact.
#[test]
fn ac5_one_move_plans_a_change_in_every_writing_store() {
    let fs = seed();
    let plan = plan_of(&fs);
    let paths = planned_paths(&plan);

    for (store, needle) in [
        ("claude.projects", "/.claude/projects/E--Projects-A"),
        ("claude.json", "/.claude.json"),
        ("claude.history", "/.claude/history.jsonl"),
        ("plugin.state", "/.claude/plugins/data/codex/state/"),
    ] {
        assert!(
            paths.iter().any(|p| p.contains(needle)),
            "AC-5: the plan must name store {store} (looked for {needle}). Planned paths: {paths:?}"
        );
    }
}

/// The other half of AC-5: nothing outside the listed set is touched. The unrelated project
/// shares both multi-entry stores with the moved one, so a rewrite that over-matched would
/// show up here.
#[test]
fn ac5_an_unrelated_project_is_untouched_by_the_move() {
    let fs = seed();
    assert!(
        project_keys(&fs).iter().any(|k| k == OTHER),
        "precondition: the unrelated project must be in claude.json before the move"
    );

    apply_move(&fs);

    assert!(
        project_keys(&fs).iter().any(|k| k == OTHER),
        "AC-5: the unrelated project's claude.json entry must survive the move"
    );
    assert!(
        read_text(&fs, "/h/.claude/history.jsonl").contains(OTHER_IN_HISTORY),
        "AC-5: the unrelated project's history line must survive the move"
    );
}

// ---------------------------------------------------------------------------------------
// AC-8: the encoded transcript directory is renamed and the old one is gone
// ---------------------------------------------------------------------------------------

/// Existing tests confirm the NEW encoded directory exists afterwards. A copy-instead-of-rename
/// satisfies that and leaves a duplicate behind, which is what this asserts against. The
/// criterion says "gone", so "gone" is what gets checked.
#[test]
fn ac8_the_old_encoded_directory_is_gone_after_apply() {
    let fs = seed();
    let old_file = Path::new(OLD_ENC).join("s1.jsonl");
    let new_file = Path::new(NEW_ENC).join("s1.jsonl");

    assert!(
        fs.exists(&old_file),
        "precondition: the old encoded dir must hold a transcript before the move, or the \
         gone-check below would pass against an empty fixture"
    );

    apply_move(&fs);

    assert!(
        fs.exists(&new_file),
        "AC-8: the transcript must exist under the new encoding"
    );
    assert!(
        !fs.exists(&old_file),
        "AC-8: the transcript must NOT still exist under the old encoding; a copy rather than \
         a rename passes every other assertion in the suite and fails only here"
    );
    assert!(
        !fs.is_dir(Path::new(OLD_ENC)),
        "AC-8: the old encoded directory itself must be gone, not merely emptied"
    );
}

// ---------------------------------------------------------------------------------------
// AC-13: every variant key migrates, the file still parses, entry count unchanged
// ---------------------------------------------------------------------------------------

/// Apply-then-readback for `.claude.json` is already proven on raw bytes and on the parsed
/// view (`claude_json_escaping.rs`). The unasserted clause is narrower: that the NUMBER of
/// entries is the same afterwards. AR-04 lived in exactly this family, where two slugs sharing
/// one path value produced two splices each expecting one match, so a rewrite that drops or
/// duplicates an entry while still parsing is the shape this clause exists to catch.
#[test]
fn ac13_claude_json_entry_count_is_unchanged_by_a_move() {
    let fs = seed();
    let before = project_keys(&fs);
    assert_eq!(
        before.len(),
        2,
        "precondition: the fixture must hold two project entries, got {before:?}"
    );
    assert!(
        before.iter().any(|k| k == OLD),
        "precondition: the moved project must be one of them, got {before:?}"
    );

    apply_move(&fs);

    let after = project_keys(&fs);
    assert_eq!(
        after.len(),
        before.len(),
        "AC-13: the projects entry count must be unchanged by a move. before={before:?} after={after:?}"
    );
    assert!(
        after.iter().any(|k| k == NEW),
        "AC-13: the moved project must be present under its new path. after={after:?}"
    );
    assert!(
        !after.iter().any(|k| k == OLD),
        "AC-13: the old path must not survive as a key. after={after:?}"
    );
}

// ---------------------------------------------------------------------------------------
// AC-14: history.jsonl entries follow the project to its new path
// ---------------------------------------------------------------------------------------

/// An apply-then-readback of `history.jsonl` exists, but it exercises `associate`
/// (`associate_and_duplicates.rs`). A move drives a different code path: the history store's
/// `plan` emits one anchored rewrite rule per distinct stored form. This reads the file back
/// after a MOVE, on the raw bytes and then line by line as JSON.
#[test]
fn ac14_history_jsonl_reads_back_at_the_new_path_after_a_move() {
    let fs = seed();
    let before = read_text(&fs, "/h/.claude/history.jsonl");
    assert!(
        before.contains(OLD_IN_HISTORY),
        "precondition: history must reference the old path before the move"
    );

    apply_move(&fs);

    let after = read_text(&fs, "/h/.claude/history.jsonl");
    assert!(
        after.contains(NEW_IN_HISTORY),
        "AC-14: the moved project's history line must reference the new path. Got:\n{after}"
    );
    assert!(
        !after.contains(OLD_IN_HISTORY),
        "AC-14: no history line may still reference the old path. Got:\n{after}"
    );

    // Still valid JSONL: a byte splice that corrupted a line would leave the text looking
    // right to `contains` while breaking every consumer.
    let lines: Vec<&str> = after.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        2,
        "AC-14: the line count must be unchanged by a rewrite. Got:\n{after}"
    );
    for line in lines {
        serde_json::from_str::<Value>(line).unwrap_or_else(|e| {
            panic!("AC-14: every rewritten history line must still parse as JSON: {e}\n{line}")
        });
    }
}
