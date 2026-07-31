//! Decision 2: a `githubRepoPaths` value of the wrong JSON type is skipped **silently**.
//!
//! `probe()` validates that `projects` is an object and stops there; `githubRepoPaths` gets no
//! shape validation at all, and both read sites do `if let Some(a) = arr.as_array()`, so a value
//! that is a bare string rather than an array vanishes without a word. Exit 0, no output.
//!
//! That was discovered the hard way during the 2026-07-28 acceptance run: a malformed synthetic
//! entry was ignored and briefly looked like a tool bug. A warning would have said so instantly.
//!
//! The resolution is warn, not fail. Exit 4 would let one odd entry block an otherwise-fine
//! migration, which is too aggressive for a field whose worst case is a missed rewrite rather
//! than a corrupted write. But silence was only the behavior by accident, and it costs time.

use awt_core::doctor::doctor;
use awt_core::fs::{FileSystem, MemoryFileSystem};
use awt_core::model::Move;
use awt_core::plan::{build_plan, PlanOpts};
use std::path::Path;

const HOME: &str = "/h";

/// `owner/good` is a correct array. `owner/bad-string` and `owner/bad-number` are not.
///
/// The malformed entries deliberately hold a DIFFERENT path from the good one. Sharing a literal
/// is a real and interesting case, but a different one - it makes the move refuse rather than
/// proceed - so it gets its own test below instead of being tangled into these.
fn seed() -> MemoryFileSystem {
    let fs = MemoryFileSystem::new();
    let json = concat!(
        r#"{"projects":{"E:/tmp/probe":{}},"#,
        r#""githubRepoPaths":{"#,
        r#""owner/good":["E:\\tmp\\probe"],"#,
        r#""owner/bad-string":"E:\\tmp\\elsewhere","#,
        r#""owner/bad-number":42}}"#
    );
    fs.write(Path::new("/h/.claude.json"), json.as_bytes())
        .unwrap();
    fs.write(Path::new("E:/tmp/probe/.keep"), b"x").unwrap();
    fs
}

fn plan_opts() -> PlanOpts {
    PlanOpts {
        force: false,
        move_folder: true,
    }
}

#[test]
fn doctor_warns_about_wrong_typed_github_repo_path_values() {
    let fs = seed();
    let rep = doctor(&fs, Path::new(HOME)).expect("doctor must not fail on a wrong-typed value");

    let joined = rep.warnings.join("\n");
    assert!(
        joined.contains("owner/bad-string"),
        "no warning named the string-valued entry: {:?}",
        rep.warnings
    );
    assert!(
        joined.contains("owner/bad-number"),
        "no warning named the number-valued entry: {:?}",
        rep.warnings
    );
    assert!(
        !joined.contains("owner/good"),
        "a correctly typed entry was warned about: {:?}",
        rep.warnings
    );
}

/// Warn, do not fail. A single odd entry must not block an otherwise-fine run.
#[test]
fn a_wrong_typed_value_does_not_abort_the_run() {
    let fs = seed();
    assert!(doctor(&fs, Path::new(HOME)).is_ok());
    let mv = Move {
        src_abs: r"E:\tmp\probe".into(),
        dst_abs: r"E:\tmp\probe-moved".into(),
    };
    assert!(build_plan(&fs, Path::new(HOME), &mv, &plan_opts()).is_ok());
}

/// The same warning must reach a user who is planning a move, not only one running doctor:
/// that is the person about to write, and the consequence is a silently missed rewrite.
#[test]
fn plan_carries_the_same_warning() {
    let fs = seed();
    let mv = Move {
        src_abs: r"E:\tmp\probe".into(),
        dst_abs: r"E:\tmp\probe-moved".into(),
    };
    let plan = build_plan(&fs, Path::new(HOME), &mv, &plan_opts()).unwrap();
    let joined = plan.warnings.join("\n");
    assert!(
        joined.contains("owner/bad-string"),
        "plan did not carry the type warning: {:?}",
        plan.warnings
    );
}

/// Warning is not rewriting. The malformed entries must still be left exactly as they were.
#[test]
fn wrong_typed_values_are_still_never_rewritten() {
    let fs = seed();
    let before = String::from_utf8(fs.read(Path::new("/h/.claude.json")).unwrap()).unwrap();
    let mv = Move {
        src_abs: r"E:\tmp\probe".into(),
        dst_abs: r"E:\tmp\probe-moved".into(),
    };
    let plan = build_plan(&fs, Path::new(HOME), &mv, &plan_opts()).unwrap();
    awt_core::apply::apply(&plan, &fs, Path::new("/backup"), "DEC2").unwrap();
    let after = String::from_utf8(fs.read(Path::new("/h/.claude.json")).unwrap()).unwrap();

    assert!(
        after.contains(r#""owner/bad-string":"E:\\tmp\\elsewhere""#),
        "the string-valued entry was altered: {after}"
    );
    assert!(
        after.contains(r#""owner/bad-number":42"#),
        "the number-valued entry was altered: {after}"
    );
    // The correctly typed one still gets rewritten, so this is not a vacuous pass.
    assert!(
        after.contains(r#""owner/good":["E:\\tmp\\probe-moved"]"#),
        "the well-formed entry was not rewritten: {after}"
    );
    assert_ne!(before, after, "nothing happened at all");
}

/// A file with no malformed entries produces no warnings, so the channel stays meaningful.
#[test]
fn a_clean_file_produces_no_type_warnings() {
    let fs = MemoryFileSystem::new();
    let json =
        r#"{"projects":{"E:/tmp/probe":{}},"githubRepoPaths":{"owner/good":["E:\\tmp\\probe"]}}"#;
    fs.write(Path::new("/h/.claude.json"), json.as_bytes())
        .unwrap();
    fs.write(Path::new("E:/tmp/probe/.keep"), b"x").unwrap();
    let rep = doctor(&fs, Path::new(HOME)).unwrap();
    assert!(rep.warnings.is_empty(), "{:?}", rep.warnings);
}

/// A malformed entry holding the SAME path as a well-formed one makes the move refuse, because
/// the anchor literal genuinely occurs twice in the file while only one occurrence was planned.
///
/// That refusal is correct - the tool cannot tell the two occurrences apart by literal matching,
/// and guessing is what it exists not to do. It is recorded here because the refusal used to be
/// baffling: "expected 1, live 2" with no indication why. The warning now names the malformed
/// entry, which turns an inscrutable count mismatch into a diagnosis.
#[test]
fn a_malformed_entry_sharing_a_path_makes_the_move_refuse_and_the_warning_explains_why() {
    let fs = MemoryFileSystem::new();
    let json = concat!(
        r#"{"projects":{"E:/tmp/probe":{}},"#,
        r#""githubRepoPaths":{"#,
        r#""owner/good":["E:\\tmp\\probe"],"#,
        r#""owner/shadow":"E:\\tmp\\probe"}}"#
    );
    fs.write(Path::new("/h/.claude.json"), json.as_bytes())
        .unwrap();
    fs.write(Path::new("E:/tmp/probe/.keep"), b"x").unwrap();

    let mv = Move {
        src_abs: r"E:\tmp\probe".into(),
        dst_abs: r"E:\tmp\probe-moved".into(),
    };
    let plan = build_plan(&fs, Path::new(HOME), &mv, &plan_opts()).unwrap();

    // The warning is present before the write is even attempted.
    assert!(
        plan.warnings.join("\n").contains("owner/shadow"),
        "the plan should name the shadowing entry: {:?}",
        plan.warnings
    );

    let before = String::from_utf8(fs.read(Path::new("/h/.claude.json")).unwrap()).unwrap();

    // apply_verified, not apply. The bare `apply` is the low-level primitive: it walks the change
    // list and stops at the first refusal, leaving earlier changes in place - here the projects
    // key would already be rewritten. `apply_verified` is what the CLI calls and what carries the
    // auto-rollback, so it is the layer at which "a refused run changes nothing" is actually true.
    let opts = awt_core::apply::ApplyOpts {
        run_id: "DEC2B".into(),
        auto_rollback: true,
        force: false,
    };
    let err = awt_core::apply::apply_verified(&plan, &fs, Path::new("/backup"), &opts).unwrap_err();
    assert!(
        format!("{err:?}").contains("live 2"),
        "expected a count-check refusal, got {err:?}"
    );
    assert_eq!(
        String::from_utf8(fs.read(Path::new("/h/.claude.json")).unwrap()).unwrap(),
        before,
        "auto-rollback must restore the file byte-identically after a refusal"
    );
}
