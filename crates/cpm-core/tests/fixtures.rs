use cpm_core::fs::{FileSystem, MemoryFileSystem};
use std::path::{Path, PathBuf};

pub fn seed_memory_fs_from(dir: &Path) -> MemoryFileSystem {
    let fs = MemoryFileSystem::new();
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file() {
            let rel = entry.path().strip_prefix(dir).unwrap();
            let virt = PathBuf::from("/home/.claude-fixture").join(rel);
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
    let dir =
        Path::new("/home/.claude-fixture/projects/E--Projects-Github-Repos-markdown-for-humans");
    assert_eq!(fs.read_dir(dir).unwrap().len(), 2);
}
