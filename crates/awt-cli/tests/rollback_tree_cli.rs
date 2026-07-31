//! AC-54 end to end through the real binary on the REAL filesystem. The MemoryFileSystem
//! tests in awt-core prove the logic; this proves the same property against actual Windows
//! rename semantics, because the fix's core operation is `std::fs::rename` on a directory
//! and an in-memory model can lie about that.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() {
                stack.push(e.path());
            } else {
                let rel = e
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, std::fs::read(e.path()).unwrap());
            }
        }
    }
    out
}

#[test]
fn apply_then_rollback_restores_sidecars_on_the_real_filesystem() {
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();

    // A real source project folder.
    let src_dir = root.path().join("src").join("proj");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(src_dir.join("f.txt"), b"x").unwrap();
    let src_abs = src_dir.to_str().unwrap().to_string();
    let dst_abs = root.path().join("dst-proj").to_str().unwrap().to_string();

    // A project-state dir with a transcript AND sidecars the old snapshot never covered:
    // a nested memory file and a binary blob.
    let encoded: String = src_abs
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let proj_state = home.path().join(".claude").join("projects").join(&encoded);
    std::fs::create_dir_all(proj_state.join("memory")).unwrap();
    std::fs::create_dir_all(proj_state.join("tool-results")).unwrap();
    let cwd_json = format!("{{\"cwd\":\"{}\"}}\n", src_abs.replace('\\', "\\\\"));
    std::fs::write(proj_state.join("s.jsonl"), cwd_json.as_bytes()).unwrap();
    std::fs::write(
        proj_state.join("memory").join("notes.md"),
        b"# irreplaceable\n",
    )
    .unwrap();
    std::fs::write(
        proj_state.join("tool-results").join("blob.bin"),
        [0u8, 255, 254, 42],
    )
    .unwrap();

    let before = tree(&proj_state);
    assert_eq!(before.len(), 3, "seed sanity");

    let backup_root = root.path().join("backup");
    std::fs::create_dir_all(&backup_root).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "apply",
            "--home",
            home.path().to_str().unwrap(),
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
        "apply failed\nstderr: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );

    // Find the manifest and roll back.
    let awt_dir = std::fs::read_dir(&backup_root)
        .unwrap()
        .find_map(|e| {
            let e = e.unwrap();
            e.file_name()
                .to_string_lossy()
                .starts_with("awt-")
                .then(|| e.path())
        })
        .expect("backup dir created");
    let manifest = awt_dir.join("manifest.json");

    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "rollback",
            "--home",
            home.path().to_str().unwrap(),
            "--report",
            manifest.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "rollback failed\nstderr: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );

    // The complete tree - sidecars included - is back, byte-identical.
    assert!(proj_state.is_dir(), "project-state dir restored");
    let after = tree(&proj_state);
    assert_eq!(
        before, after,
        "the COMPLETE project-state tree must survive apply+rollback; a missing entry here \
         is a file the undo destroyed"
    );
    // The real folder came back and the renamed state dir did not linger.
    assert!(Path::new(&src_abs).is_dir(), "source folder restored");
    assert!(!Path::new(&dst_abs).exists(), "destination folder removed");
}
