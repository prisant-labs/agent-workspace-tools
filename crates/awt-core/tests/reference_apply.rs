mod fixtures;
use awt_core::apply::apply;
use awt_core::fs::FileSystem;
use awt_core::model::Move;
use awt_core::plan::{build_plan, PlanOpts};
use std::path::Path;

#[test]
fn reference_move_end_to_end_leaves_no_old_cwd() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let fs = fixtures::seed_memory_fs_from(&base.join("test/fixtures/reference-move/before"));
    // fixture is seeded under /home/.claude-fixture; treat that as home root
    let home = Path::new("/home/.claude-fixture");
    let mv = Move {
        src_abs: "E:\\Projects\\Github Repos\\markdown-for-humans".into(),
        dst_abs: "E:\\Projects\\prisant-labs\\vs-code-markdown-max".into(),
    };
    // seed the source folder so MoveTree has something to move
    fs.write(
        Path::new("E:/Projects/Github Repos/markdown-for-humans/.keep"),
        b"x",
    )
    .unwrap();
    let opts = PlanOpts {
        force: false,
        move_folder: true,
    };
    let plan = build_plan(&fs, home, &mv, &opts).unwrap();
    apply(&plan, &fs, Path::new("/backup"), "REF").unwrap();

    let new_dir = home.join(".claude/projects/E--Projects-prisant-labs-vs-code-markdown-max");
    let mut checked = 0;
    for child in fs.read_dir(&new_dir).unwrap() {
        if child.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            let text = String::from_utf8_lossy(&fs.read(&child).unwrap()).into_owned();
            // The old cwd is gone...
            assert!(!text.contains(r#""cwd":"E:\\Projects\\Github Repos\\markdown-for-humans""#));
            // ...and the new one is present, which proves a rewrite actually happened rather
            // than the file being empty or absent (an empty file satisfies the negative alone).
            assert!(text.contains(r#""cwd":"E:\\Projects\\prisant-labs\\vs-code-markdown-max""#));
            checked += 1;
        }
    }
    // Guard against a vacuous pass. The fixture holds exactly two transcripts and the whole
    // point of this golden is that they were found, moved, and rewritten end to end. If the
    // pipeline silently finds nothing (as it did before the fixture seed layout was fixed),
    // the loop never runs and this assertion fails instead of the test passing on an empty set.
    assert_eq!(
        checked, 2,
        "expected exactly 2 rewritten transcripts under the new dir; found {checked}"
    );
}
