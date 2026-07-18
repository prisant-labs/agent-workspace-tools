use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub trait FileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn exists(&self, path: &Path) -> bool;
    fn is_file(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    fn copy(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
    fn mtime_secs(&self, path: &Path) -> io::Result<u64>;
}

/// Returns the normalized path string: backslashes replaced with forward slashes.
fn norm(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Returns the lowercased normalized path used as the BTreeMap key in MemoryFileSystem.
/// All lookups key on this value, which makes them case-insensitive (NTFS behavior).
fn norm_key(p: &Path) -> String {
    norm(p).to_lowercase()
}

pub struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, data)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }
    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(path)? {
            out.push(entry?.path());
        }
        out.sort();
        Ok(out)
    }
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }
    fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(from, to).map(|_| ())
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_dir_all(path)
    }
    fn mtime_secs(&self, path: &Path) -> io::Result<u64> {
        Ok(std::fs::metadata(path)?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs())
    }
}

/// An in-memory filesystem that models NTFS case behavior:
/// - Case-insensitive for lookups (read, exists, is_file, is_dir, rename, copy, remove_dir_all)
/// - Case-preserving for output (read_dir returns entries with their original casing)
///
/// The map is keyed by the LOWERCASED normalized path so that all lookups are
/// case-insensitive. Each value stores the ORIGINAL-cased normalized path alongside
/// the file bytes and a mtime (Unix seconds), so read_dir can return entries without
/// lowercasing them and mtime_secs can return the stored modification time.
///
/// Overwrite policy: when a path is written again with different casing, the ORIGINAL
/// casing is preserved (first write's casing wins). This matches NTFS behavior, where
/// the filesystem remembers the name from the CreateFile call that first created the
/// directory entry; subsequent opens with different casing reuse the same entry without
/// updating the stored name.
/// Map value: (original-cased normalized path, file bytes, mtime unix secs).
type FsEntry = (String, Vec<u8>, u64);

#[derive(Default)]
pub struct MemoryFileSystem {
    // key   : lowercased normalized path (for case-insensitive lookup)
    // value : (original-cased normalized path, file bytes, mtime unix secs)
    files: Mutex<BTreeMap<String, FsEntry>>,
}

impl MemoryFileSystem {
    pub fn new() -> Self {
        Self {
            files: Mutex::new(BTreeMap::new()),
        }
    }

    /// Write a file with an explicit mtime (Unix seconds). Intended for tests
    /// that need deterministic age calculations without hitting the real clock.
    pub fn write_at(&self, path: &Path, data: &[u8], mtime: u64) {
        let key = norm_key(path);
        let original = norm(path);
        let mut f = self.files.lock().unwrap();
        let stored_orig = f
            .get(&key)
            .map(|(orig, _, _)| orig.clone())
            .unwrap_or(original);
        f.insert(key, (stored_orig, data.to_vec(), mtime));
    }
}

impl FileSystem for MemoryFileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files
            .lock()
            .unwrap()
            .get(&norm_key(path))
            .map(|(_, data, _)| data.clone())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, norm(path)))
    }

    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        let key = norm_key(path);
        let original = norm(path);
        let mut f = self.files.lock().unwrap();
        // On overwrite with different casing, keep the ORIGINAL casing (first write wins).
        // This matches NTFS: the OS preserves the name from the first CreateFile call that
        // created the directory entry; subsequent opens with different casing reuse the same
        // entry without updating the stored name.
        let stored_orig = f
            .get(&key)
            .map(|(orig, _, _)| orig.clone())
            .unwrap_or(original);
        f.insert(key, (stored_orig, data.to_vec(), 0));
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut f = self.files.lock().unwrap();
        let fp_key = norm_key(from);
        let tp = norm(to);
        let tp_key = norm_key(to);
        let moved: Vec<String> = f
            .keys()
            .filter(|k| **k == fp_key || k.starts_with(&format!("{fp_key}/")))
            .cloned()
            .collect();
        if moved.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, norm(from)));
        }
        for k in moved {
            let (orig, data, mtime) = f.remove(&k).unwrap();
            // norm_key lowercases without changing ASCII char count, so fp_key.len()
            // equals norm(from).len() and slicing orig at that offset gives the
            // original-cased suffix: "" for an exact file match, "/Child/Path" for tree entries.
            let suffix = &orig[fp_key.len()..];
            let new_key = format!("{tp_key}{}", suffix.to_lowercase());
            let new_orig = format!("{tp}{suffix}");
            f.insert(new_key, (new_orig, data, mtime));
        }
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        let p = norm_key(path);
        let f = self.files.lock().unwrap();
        f.contains_key(&p) || f.keys().any(|k| k.starts_with(&format!("{p}/")))
    }

    fn is_file(&self, path: &Path) -> bool {
        self.files.lock().unwrap().contains_key(&norm_key(path))
    }

    fn is_dir(&self, path: &Path) -> bool {
        let p = norm_key(path);
        let f = self.files.lock().unwrap();
        !f.contains_key(&p) && f.keys().any(|k| k.starts_with(&format!("{p}/")))
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let prefix_key = format!("{}/", norm_key(path));
        let f = self.files.lock().unwrap();
        let mut kids: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for (k, (orig, _, _)) in f.iter() {
            if k.strip_prefix(&prefix_key).is_some() {
                // norm_key lowercases without changing ASCII char count, so prefix_key.len()
                // is the same byte offset in both the lowercase key and the original-cased path.
                // Slicing orig at that offset gives the remainder of the path in its original
                // casing. Taking the first '/' segment gives the immediate child's original name.
                let orig_rest = &orig[prefix_key.len()..];
                let first_orig = orig_rest.split('/').next().unwrap();
                let child_orig = format!("{}{}", &orig[..prefix_key.len()], first_orig);
                kids.insert(child_orig);
            }
        }
        Ok(kids.into_iter().map(PathBuf::from).collect())
    }

    fn create_dir_all(&self, _path: &Path) -> io::Result<()> {
        Ok(())
    }

    fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        let data = self.read(from)?;
        self.write(to, &data)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        let p = norm_key(path);
        let mut f = self.files.lock().unwrap();
        f.retain(|k, _| *k != p && !k.starts_with(&format!("{p}/")));
        Ok(())
    }

    fn mtime_secs(&self, path: &Path) -> io::Result<u64> {
        self.files
            .lock()
            .unwrap()
            .get(&norm_key(path))
            .map(|(_, _, mtime)| *mtime)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, norm(path)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn memory_fs_round_trip_and_rename() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("/a/b.txt"), b"hello").unwrap();
        assert!(fs.exists(Path::new("/a/b.txt")));
        assert!(fs.is_file(Path::new("/a/b.txt")));
        assert_eq!(fs.read(Path::new("/a/b.txt")).unwrap(), b"hello");
        fs.rename(Path::new("/a/b.txt"), Path::new("/a/c.txt"))
            .unwrap();
        assert!(!fs.exists(Path::new("/a/b.txt")));
        assert_eq!(fs.read(Path::new("/a/c.txt")).unwrap(), b"hello");
        let kids = fs.read_dir(Path::new("/a")).unwrap();
        assert_eq!(kids, vec![std::path::PathBuf::from("/a/c.txt")]);
    }

    // --- LEAD-07 part 1: case-sensitivity tests for MemoryFileSystem ---

    #[test]
    fn case_insensitive_read() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("E:/Projects/Foo/a.txt"), b"data")
            .unwrap();
        // Reading via a completely different casing must succeed and return the same bytes.
        let result = fs.read(Path::new("e:/projects/foo/A.TXT")).unwrap();
        assert_eq!(result, b"data");
    }

    /// This is the exact call ProjectIndex::build makes in index.rs:
    ///   fs.is_dir(Path::new(c.as_str()))
    /// where c is a recorded cwd such as "E:\Projects\Foo". If MemoryFileSystem
    /// were case-sensitive, a cwd like "e:\projects\foo" would not match a file
    /// written at "E:\Projects\Foo\a.txt", making is_dir return false and marking
    /// a live project as missing. This test closes that divergence (LEAD-07).
    #[test]
    fn case_insensitive_is_dir() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("E:/Projects/Foo/a.txt"), b"data")
            .unwrap();
        // is_dir must return true regardless of the casing used for the directory path.
        assert!(fs.is_dir(Path::new("e:/projects/foo")));
        assert!(fs.is_dir(Path::new("E:/PROJECTS/FOO")));
        // is_dir must return false for the file itself (it is a file, not a directory).
        assert!(!fs.is_dir(Path::new("e:/projects/foo/a.txt")));
    }

    #[test]
    fn case_preserving_read_dir() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("E:/Projects/Foo/Bar.txt"), b"data")
            .unwrap();
        // read_dir called with a lowercase path must return the ORIGINAL casing, not
        // the lowercased form of the argument. ProjectIndex::build uses read_dir output
        // as PathBuf keys; existing tests assert on exact strings like "/h/.claude/projects/E--a".
        let kids = fs.read_dir(Path::new("e:/projects/foo")).unwrap();
        assert_eq!(kids, vec![PathBuf::from("E:/Projects/Foo/Bar.txt")]);
    }

    #[test]
    fn case_insensitive_overwrite_results_in_single_entry() {
        // Write with one casing, then write the same logical path with different casing.
        // The result must be ONE entry (not two), and both reads must return the new bytes.
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("/root/A/file.txt"), b"first").unwrap();
        fs.write(Path::new("/root/a/FILE.TXT"), b"second").unwrap();
        // Second write's bytes are accessible via either casing.
        assert_eq!(fs.read(Path::new("/root/A/file.txt")).unwrap(), b"second");
        assert_eq!(fs.read(Path::new("/root/a/FILE.TXT")).unwrap(), b"second");
        // read_dir returns exactly one child, proving it is one entry, not two.
        let kids = fs.read_dir(Path::new("/root/A")).unwrap();
        assert_eq!(
            kids.len(),
            1,
            "expected 1 entry after case-variant overwrite, got {}",
            kids.len()
        );
    }

    #[test]
    fn case_insensitive_rename_and_exists() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("E:/Projects/Foo/a.txt"), b"hello")
            .unwrap();
        // Rename using a different casing for the source path.
        fs.rename(
            Path::new("e:/PROJECTS/FOO/a.txt"),
            Path::new("E:/Projects/Foo/b.txt"),
        )
        .unwrap();
        // Old path must not exist under any casing.
        assert!(!fs.exists(Path::new("E:/Projects/Foo/a.txt")));
        assert!(!fs.exists(Path::new("e:/projects/foo/a.txt")));
        // New path must exist and hold the original bytes, accessible via any casing.
        assert!(fs.exists(Path::new("E:/Projects/Foo/b.txt")));
        assert!(fs.exists(Path::new("e:/PROJECTS/FOO/B.TXT")));
        assert_eq!(
            fs.read(Path::new("E:/projects/FOO/b.txt")).unwrap(),
            b"hello"
        );
    }
}
