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

// (A copy_tree helper lived here while the plan tests seeded from the golden fixture. Those
// tests now build their own real source dirs - see seed_real_move - because the fixture's
// recorded cwd only exists on the machine that produced it, and the AC-55 source guard
// correctly refuses a source that does not exist.)

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

/// Create a real, existing source project dir plus a home whose transcript records it as cwd.
/// Everything lives under caller-owned temp dirs so the AC-55 source-exists guard passes on
/// any machine, including a CI runner with no E:\ drive. (The fixture-based variant of these
/// tests hardcoded a real path from the dev machine, which only ever existed there.)
/// Returns (src_abs, dst_abs); dst shares src's volume and does not exist.
fn seed_real_move(root: &std::path::Path, home: &std::path::Path) -> (String, String) {
    let src_dir = root.join("src").join("proj");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(src_dir.join("f.txt"), b"x").unwrap();
    let src_abs = src_dir.to_str().unwrap().to_string();
    seed_home(home, &src_abs);
    let dst_abs = root.join("dst-nonexistent").to_str().unwrap().to_string();
    (src_abs, dst_abs)
}

#[test]
fn plan_is_non_empty_and_writes_nothing() {
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let (src_abs, dst_abs) = seed_real_move(root.path(), home.path());

    let encoded = encode_project_dir_local(&src_abs);
    let transcript = home
        .path()
        .join(".claude")
        .join("projects")
        .join(&encoded)
        .join("s.jsonl");
    let before = std::fs::read(&transcript).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "plan",
            "--home",
            home.path().to_str().unwrap(),
            "--src",
            &src_abs,
            "--dst",
            &dst_abs,
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
        stdout.contains("proj"),
        "plan did not mention the move: {stdout}"
    );
    assert!(
        stdout.contains("rewrite"),
        "plan produced no rewrites (empty/vacuous plan): {stdout}"
    );
    // Dry-run wrote nothing: the seeded transcript is byte-identical afterward.
    assert_eq!(
        std::fs::read(&transcript).unwrap(),
        before,
        "plan must not modify the transcript"
    );
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

/// AR-03: `--json` is a global flag documented as emitting machine-readable output, but
/// `plan` and `verify` accepted it, exited 0, and printed human text anyway. A script
/// piping the result into a parser got prose with no signal that it asked for something
/// unsupported.
///
/// This also matters beyond v1: the v2 GUI parity gate in docs/ROADMAP.md is
/// `GUI plan model == awt plan --json`, and that contract cannot be written against a
/// binary whose `plan --json` returns prose.
#[test]
fn plan_json_flag_emits_parseable_json() {
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let (src_abs, dst_abs) = seed_real_move(root.path(), home.path());

    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "plan",
            "--json",
            "--home",
            home.path().to_str().unwrap(),
            "--src",
            &src_abs,
            "--dst",
            &dst_abs,
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("plan --json did not emit JSON ({e}); got:\n{stdout}"));

    assert_eq!(v["src"], src_abs.as_str());
    assert_eq!(v["dst"], dst_abs.as_str());

    let changes = v["changes"].as_array().expect("changes must be an array");
    assert!(!changes.is_empty(), "vacuous plan: {stdout}");
    // Every change must be self-describing, so a consumer can switch on kind.
    for c in changes {
        assert!(
            c["kind"].is_string(),
            "every change needs a kind discriminant: {c}"
        );
    }
    assert!(
        changes.iter().any(|c| c["kind"] == "rewrite_file"),
        "expected at least one rewrite_file change: {stdout}"
    );
    // Totals let a UI render a summary without walking the whole array.
    assert!(v["totals"]["changes"].as_u64().unwrap() > 0);
    assert!(v["totals"]["edits"].as_u64().unwrap() > 0);
    assert!(v["warnings"].is_array());
    assert!(v["nested"].is_array());
}

/// `plan --json` must stay a dry run: emitting JSON is not a licence to write.
#[test]
fn plan_json_still_writes_nothing() {
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let (src_abs, dst_abs) = seed_real_move(root.path(), home.path());

    let encoded = encode_project_dir_local(&src_abs);
    let transcript = home
        .path()
        .join(".claude")
        .join("projects")
        .join(&encoded)
        .join("s.jsonl");
    let before = std::fs::read(&transcript).unwrap();

    Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "plan",
            "--json",
            "--home",
            home.path().to_str().unwrap(),
            "--src",
            &src_abs,
            "--dst",
            &dst_abs,
        ])
        .output()
        .unwrap();

    assert_eq!(
        std::fs::read(&transcript).unwrap(),
        before,
        "plan --json must not modify the transcript"
    );
    assert!(
        !std::path::Path::new(&dst_abs).exists(),
        "plan --json must not create the destination"
    );
}

/// `verify --json` must emit the check list as data, and must keep its exit-code contract:
/// a failing verification still exits 3 even when the output is JSON.
#[test]
fn verify_json_flag_emits_parseable_json_and_keeps_exit_code() {
    let tmp = tempfile::tempdir().unwrap();
    seed_home(tmp.path(), r"E:\A");

    // Verify a move that never happened: checks fail, exit code must be 3.
    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "verify",
            "--json",
            "--home",
            tmp.path().to_str().unwrap(),
            "--src",
            r"E:\A",
            "--dst",
            r"E:\B",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("verify --json did not emit JSON ({e}); got:\n{stdout}"));

    let checks = v["checks"].as_array().expect("checks must be an array");
    assert!(!checks.is_empty(), "vacuous verify: {stdout}");
    for c in checks {
        assert!(c["check"].is_string(), "check needs a name: {c}");
        assert!(c["ok"].is_boolean(), "check needs a boolean ok: {c}");
    }
    assert_eq!(
        v["ok"], false,
        "this move never happened, so ok must be false"
    );
    assert!(v["failed"].as_u64().unwrap() > 0);
    assert_eq!(
        out.status.code(),
        Some(3),
        "verify must still exit 3 on failure when emitting JSON"
    );
}

/// AC-58: the removed options must be REJECTED, not silently accepted. An unknown flag is a
/// clap usage error (exit 2), which lands in the same "refused, nothing written" class as
/// every other guard.
#[test]
fn removed_options_are_rejected_outright() {
    let tmp = tempfile::tempdir().unwrap();
    for extra in [
        ["--recursive", ""],
        ["--on-collision", "keep-dest"],
        ["--scope", "full"],
    ] {
        let mut args = vec!["plan", "--home", tmp.path().to_str().unwrap()];
        args.push(extra[0]);
        if !extra[1].is_empty() {
            args.push(extra[1]);
        }
        args.extend(["--src", "E:\\a", "--dst", "E:\\b"]);
        let out = Command::new(env!("CARGO_BIN_EXE_awt"))
            .args(&args)
            .output()
            .unwrap();
        assert!(!out.status.success(), "{} must be rejected", extra[0]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("unexpected argument"),
            "{}: stderr should name the unknown flag: {stderr}",
            extra[0]
        );
    }
}
