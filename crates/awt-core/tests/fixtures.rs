use awt_core::fs::{FileSystem, MemoryFileSystem};
use std::path::{Path, PathBuf};

pub fn seed_memory_fs_from(dir: &Path) -> MemoryFileSystem {
    let fs = MemoryFileSystem::new();
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file() {
            let rel = entry.path().strip_prefix(dir).unwrap();
            // The reference fixture's `before/` holds the CONTENTS of a `~/.claude` directory
            // (projects/, ...), so seed it under `<home>/.claude/`. That makes `home` a real
            // home root that ProjectIndex::build (which scans `home/.claude/projects`) resolves
            // against. Rooting the fixture directly at the home dir left projects/ one level too
            // high, so ProjectIndex found nothing and any pipeline-driven test passed vacuously.
            let virt = PathBuf::from("/home/.claude-fixture/.claude").join(rel);
            fs.write(&virt, &std::fs::read(entry.path()).unwrap())
                .unwrap();
        }
    }
    fs
}

#[test]
fn reference_before_seeds_two_transcripts() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture_dir = manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test/fixtures/reference-move/before");
    let fs = seed_memory_fs_from(&fixture_dir);
    let dir = Path::new(
        "/home/.claude-fixture/.claude/projects/E--Projects-Sample-Repos-demo-notes-editor",
    );
    assert_eq!(fs.read_dir(dir).unwrap().len(), 2);
}
