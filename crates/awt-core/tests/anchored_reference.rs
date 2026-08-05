use awt_core::rewrite::{anchored_rewrite, build_path_rules};
use std::path::Path;

fn read(rel: &str) -> String {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    String::from_utf8_lossy(&std::fs::read(base.join(rel)).unwrap()).into_owned()
}

#[test]
fn reproduces_reference_move_counts_and_preserves_non_paths() {
    let old = "E:\\Projects\\Sample Repos\\demo-notes-editor";
    let new = "E:\\Projects\\demo-labs\\demo-notes-editor-pro";
    let rules = build_path_rules(old, new);

    let mut total = 0usize;
    for f in [
        "22b2362e-e4ef-4042-9b01-e3cba5719590.jsonl",
        "28fd093e-f5ef-4dc7-af16-ea415c1840f7.jsonl",
    ] {
        let text = read(&format!(
            "test/fixtures/reference-move/before/projects/E--Projects-Sample-Repos-demo-notes-editor/{f}"));
        let (out, n) = anchored_rewrite(&text, &rules);
        total += n;
        // non-path mentions preserved
        assert_eq!(
            text.matches("demo-notes-editor@").count(),
            out.matches("demo-notes-editor@").count()
        );
        assert_eq!(
            text.matches("demo-notes-editor_dev-").count(),
            out.matches("demo-notes-editor_dev-").count()
        );
        // line count unchanged
        assert_eq!(text.lines().count(), out.lines().count());
        // no old path remains where anchored
        assert!(!out.contains(r#""cwd":"E:\\Projects\\Sample Repos\\demo-notes-editor""#));
    }
    // cwd 1467 + backslash 588 + forward 27 = 2082 anchored replacements
    assert_eq!(total, 1467 + 588 + 27);
}
