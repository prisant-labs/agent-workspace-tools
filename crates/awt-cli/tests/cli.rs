use std::io::Write as _;
use std::process::{Command, Stdio};

/// Mirror of awt_core::paths::encode_project_dir, kept local so the integration
/// test does not depend on the library's internals.
fn encode_project_dir_local(abs: &str) -> String {
    abs.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Seed a minimal Claude home: one project dir whose transcript records `src_abs`
/// as its cwd, so build_plan finds something to rewrite.
fn seed_home(home: &std::path::Path, src_abs: &str) {
    let encoded = encode_project_dir_local(src_abs);
    let proj_state = home.join(".claude").join("projects").join(&encoded);
    std::fs::create_dir_all(&proj_state).unwrap();
    // The cwd value inside the JSON must use double-backslash for Windows paths.
    let cwd_json = format!("{{\"cwd\":\"{}\"}}\n", src_abs.replace('\\', "\\\\"));
    std::fs::write(proj_state.join("s.jsonl"), cwd_json.as_bytes()).unwrap();
}

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

/// Pipe a SessionEnd JSON payload to `awt archive --hook-stdin` and verify the
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

    let mut child = Command::new(env!("CARGO_BIN_EXE_awt"))
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
        "awt archive --hook-stdin failed\nstdout: {}\nstderr: {}",
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

    // The destination must be on the same volume as src (E:) to pass the
    // cross-volume guard (AC-1). Use a non-existent string path: volume
    // comparison is string-based (both "E:"), and existence check returns
    // false for any non-existent path. No real directory is created, so
    // the test runs even when the runner has no E: drive.
    let dst_str = "E:\\awt-plan-nonexistent-dst".to_string();

    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
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

/// Verifies that `awt apply --json` prints a machine-readable JSON report to stdout
/// containing the required top-level fields (AC-22).
#[test]
fn apply_json_flag_emits_json_to_stdout() {
    // All project-side and backup paths share one temp root so they live on the
    // same volume, satisfying the cross-volume guard (AC-1) without requiring E:\.
    let root = tempfile::tempdir().unwrap();
    let home_tmp = tempfile::tempdir().unwrap();

    // Create the source project dir under root.
    let src_dir = root.path().join("src").join("proj");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(src_dir.join("f.txt"), b"x").unwrap();
    let src_abs = src_dir.to_str().unwrap().to_string();

    seed_home(home_tmp.path(), &src_abs);

    // Dst parent under root; the child proj-moved must NOT exist (DestinationExists guard).
    let dst_parent = root.path().join("dst");
    std::fs::create_dir_all(&dst_parent).unwrap();
    let dst_abs = dst_parent.join("proj-moved").to_str().unwrap().to_string();

    let backup_root = root.path().join("backup");
    std::fs::create_dir_all(&backup_root).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "apply",
            "--json",
            "--home",
            home_tmp.path().to_str().unwrap(),
            "--backup-root",
            backup_root.to_str().unwrap(),
            "--src",
            &src_abs,
            "--dst",
            &dst_abs,
        ])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "awt apply --json failed\nstderr: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must be valid JSON; got {stdout:?}: {e}"));

    assert!(v["run_id"].is_string(), "run_id must be a string");
    assert!(v["applied"].is_array(), "applied must be an array");
    assert!(
        !v["applied"].as_array().unwrap().is_empty(),
        "applied must be non-empty"
    );
    assert!(v["backup_dir"].is_string(), "backup_dir must be a string");
    assert!(
        v["verify"].is_array(),
        "verify must be an array after apply_verified"
    );
}

/// Verifies that a plain `awt apply` (no --json flag) writes `report.json` into the
/// backup dir and prints a human summary that includes the report path (AC-22).
#[test]
fn apply_default_writes_report_json_to_backup_dir() {
    // All project-side and backup paths share one temp root so they live on the
    // same volume, satisfying the cross-volume guard (AC-1) without requiring E:\.
    let root = tempfile::tempdir().unwrap();
    let home_tmp = tempfile::tempdir().unwrap();

    let src_dir = root.path().join("src").join("proj");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(src_dir.join("f.txt"), b"x").unwrap();
    let src_abs = src_dir.to_str().unwrap().to_string();

    seed_home(home_tmp.path(), &src_abs);

    let dst_parent = root.path().join("dst");
    std::fs::create_dir_all(&dst_parent).unwrap();
    let dst_abs = dst_parent.join("proj-moved").to_str().unwrap().to_string();

    let backup_root = root.path().join("backup");
    std::fs::create_dir_all(&backup_root).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "apply",
            "--home",
            home_tmp.path().to_str().unwrap(),
            "--backup-root",
            backup_root.to_str().unwrap(),
            "--src",
            &src_abs,
            "--dst",
            &dst_abs,
        ])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "awt apply failed\nstderr: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
    );

    // Stdout should be a human summary, not JSON.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("applied"),
        "human summary must mention 'applied': {stdout:?}"
    );
    assert!(
        stdout.contains("report:"),
        "human summary must include the report path: {stdout:?}"
    );

    // Find the awt-* backup dir created inside the backup root.
    let awt_dir = std::fs::read_dir(&backup_root)
        .unwrap()
        .find_map(|e| {
            let e = e.unwrap();
            if e.file_name().to_string_lossy().starts_with("awt-") {
                Some(e.path())
            } else {
                None
            }
        })
        .expect("backup root must contain an awt-* dir after apply");

    let report_path = awt_dir.join("report.json");
    assert!(
        report_path.exists(),
        "report.json must exist at {report_path:?}"
    );

    let raw = std::fs::read_to_string(&report_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("report.json must be valid JSON: {e}\ncontent: {raw:?}"));

    assert!(v["run_id"].is_string(), "run_id must be a string");
    assert!(v["applied"].is_array(), "applied must be an array");
    assert!(
        !v["applied"].as_array().unwrap().is_empty(),
        "applied must be non-empty"
    );
    assert!(v["backup_dir"].is_string(), "backup_dir must be a string");
}
