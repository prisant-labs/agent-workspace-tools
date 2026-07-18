use std::process::Command;

// Recursively copy a dir tree (std only).
fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dst = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            std::fs::create_dir_all(&dst).unwrap();
            copy_tree(&e.path(), &dst);
        } else {
            std::fs::copy(e.path(), &dst).unwrap();
        }
    }
}

#[test]
fn plan_is_non_empty_and_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    // Seed the fixture UNDER <temp>/.claude so ProjectIndex (home/.claude/projects) finds it.
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let claude = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    copy_tree(&base.join("test/fixtures/reference-move/before"), &claude);

    let projects = claude.join("projects/E--Projects-Github-Repos-markdown-for-humans");
    let before: Vec<(std::path::PathBuf, Vec<u8>)> = std::fs::read_dir(&projects)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            let b = std::fs::read(&p).unwrap();
            (p, b)
        })
        .collect();
    assert!(!before.is_empty(), "fixture seeding failed");

    // Use a destination path that does not exist (inside the temp dir) so the
    // DestinationExists guard does not fire on a developer machine that already has
    // the real target folder.
    let dst = tmp.path().join("dst-project");
    let dst_str = dst.to_str().unwrap().to_string();

    let out = Command::new(env!("CARGO_BIN_EXE_cpm"))
        .args([
            "plan",
            "--home",
            tmp.path().to_str().unwrap(),
            "--src",
            "E:\\Projects\\Github Repos\\markdown-for-humans",
            "--dst",
            &dst_str,
        ])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Non-vacuous: the plan must actually describe the move and at least one rewrite.
    assert!(
        stdout.contains("markdown-for-humans"),
        "plan did not mention the move: {stdout}"
    );
    assert!(
        stdout.contains("rewrite"),
        "plan produced no rewrites (empty/vacuous plan): {stdout}"
    );
    // Dry-run wrote nothing: every seeded transcript is byte-identical afterward.
    for (p, b) in &before {
        assert_eq!(
            &std::fs::read(p).unwrap(),
            b,
            "plan must not modify {}",
            p.display()
        );
    }
}
