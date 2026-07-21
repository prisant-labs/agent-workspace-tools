use std::io::Write as _;
use std::process::{Command, Stdio};

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

/// Pipe a SessionEnd JSON payload to `cpm archive --hook-stdin` and verify the
/// transcript is archived to the target directory (end-to-end hook integration test).
#[test]
fn hook_stdin_archives_transcript() {
    let tmp_home = tempfile::tempdir().unwrap();
    let tmp_arch = tempfile::tempdir().unwrap();

    // Seed a transcript in the temp home tree.
    let transcript_path = tmp_home
        .path()
        .join(".claude")
        .join("projects")
        .join("E--A")
        .join("s.jsonl");
    std::fs::create_dir_all(transcript_path.parent().unwrap()).unwrap();
    std::fs::write(&transcript_path, b"{\"cwd\":\"E:\\\\A\"}\n").unwrap();

    // Build the SessionEnd JSON that Claude Code would deliver on stdin.
    // serde_json handles path escaping (backslashes on Windows, etc.).
    let hook_json = serde_json::json!({
        "session_id": "test-session",
        "transcript_path": transcript_path.to_str().unwrap(),
        "cwd": tmp_home.path().to_str().unwrap(),
        "hook_event_name": "SessionEnd",
        "source": "test"
    })
    .to_string();

    let mut child = Command::new(env!("CARGO_BIN_EXE_cpm"))
        .args([
            "archive",
            "--home",
            tmp_home.path().to_str().unwrap(),
            "--hook-stdin",
            "--archive-dir",
            tmp_arch.path().to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(hook_json.as_bytes())
        .unwrap();

    let output = child.wait_with_output().unwrap();

    assert!(
        output.status.success(),
        "cpm archive --hook-stdin failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let archived = tmp_arch
        .path()
        .join("projects")
        .join("E--A")
        .join("s.jsonl");
    assert!(
        archived.exists(),
        "archived transcript must exist at {}",
        archived.display()
    );
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
