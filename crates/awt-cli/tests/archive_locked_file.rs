//! AR-09 / AC-35: a file held open by a live session must not stop the archive sweep.
//!
//! The SessionEnd hook runs against the live Claude home while other sessions are running,
//! and those sessions hold files such as `tasks/<sid>/.lock` with sharing denied. The sweep
//! must record the failure, copy everything else, keep the failed file out of the manifest,
//! and exit 0 so the hook keeps working.

#[cfg(windows)]
mod windows_only {
    use std::os::windows::fs::OpenOptionsExt;
    use std::process::Command;

    #[test]
    fn locked_store_file_is_reported_and_the_rest_is_archived() {
        let home = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let claude = home.path().join(".claude");

        let proj = claude.join("projects").join("E--tmp-lock-proj");
        std::fs::create_dir_all(proj.join("s1").join("subagents")).unwrap();
        std::fs::write(
            proj.join("s1.jsonl"),
            b"{\"cwd\":\"E:\\\\tmp\\\\lock-proj\"}\n",
        )
        .unwrap();
        std::fs::write(proj.join("s1").join("subagents").join("a.jsonl"), b"sub").unwrap();

        let tasks = claude.join("tasks").join("s1");
        std::fs::create_dir_all(&tasks).unwrap();
        std::fs::write(tasks.join("1.json"), b"task").unwrap();
        let lock = tasks.join(".lock");
        std::fs::write(&lock, b"").unwrap();
        let env_dir = claude.join("session-env").join("s1");
        std::fs::create_dir_all(&env_dir).unwrap();
        std::fs::write(env_dir.join("hook.sh"), b"env").unwrap();
        std::fs::create_dir_all(claude.join("todos")).unwrap();
        std::fs::write(claude.join("todos").join("s1-agent-s1.json"), b"[]").unwrap();

        // Hold the lock file open with sharing denied, as a live session does. The handle
        // must outlive the child process, so it is bound to a name and dropped at the end.
        let _held = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&lock)
            .unwrap();

        let arch = root.path().join("archive");
        let out = Command::new(env!("CARGO_BIN_EXE_awt"))
            .args([
                "archive",
                "--home",
                home.path().to_str().unwrap(),
                "--archive-dir",
                arch.to_str().unwrap(),
                "--json",
            ])
            .output()
            .unwrap();

        // Exit code: per-file failures do not fail the sweep, so the hook keeps working.
        assert_eq!(
            out.status.code(),
            Some(0),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let v: serde_json::Value = serde_json::from_str(stdout.trim())
            .unwrap_or_else(|e| panic!("stdout must be JSON ({e}): {stdout:?}"));
        assert_eq!(v["failed"].as_u64(), Some(1), "{v}");
        assert_eq!(v["copied"].as_u64(), Some(5), "{v}");

        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(".lock"),
            "the failed path must be named on stderr: {stderr}"
        );

        // Everything else was copied.
        let art = arch.join("session-artifacts");
        assert!(arch
            .join("projects")
            .join("E--tmp-lock-proj")
            .join("s1.jsonl")
            .is_file());
        assert!(arch
            .join("projects")
            .join("E--tmp-lock-proj")
            .join("s1")
            .join("subagents")
            .join("a.jsonl")
            .is_file());
        assert!(art.join("tasks").join("s1").join("1.json").is_file());
        assert!(art.join("session-env").join("s1").join("hook.sh").is_file());
        assert!(art.join("todos").join("s1-agent-s1.json").is_file());
        // The locked file is neither a destination file nor a stray temp file.
        assert!(!art.join("tasks").join("s1").join(".lock").exists());

        // The manifest lists the five archived files and not the locked one.
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(arch.join("manifest.json")).unwrap()).unwrap();
        let files = manifest["files"].as_array().unwrap();
        assert_eq!(files.len(), 5, "{manifest}");
        assert!(!files
            .iter()
            .any(|e| e["src"].as_str().unwrap_or("").contains(".lock")));
        assert!(arch.join("index.json").is_file());
    }
}
