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
}

fn norm(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

pub struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> { std::fs::read(path) }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(path, data)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> { std::fs::rename(from, to) }
    fn exists(&self, path: &Path) -> bool { path.exists() }
    fn is_file(&self, path: &Path) -> bool { path.is_file() }
    fn is_dir(&self, path: &Path) -> bool { path.is_dir() }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(path)? { out.push(entry?.path()); }
        out.sort();
        Ok(out)
    }
    fn create_dir_all(&self, path: &Path) -> io::Result<()> { std::fs::create_dir_all(path) }
    fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        if let Some(parent) = to.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::copy(from, to).map(|_| ())
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> { std::fs::remove_dir_all(path) }
}

#[derive(Default)]
pub struct MemoryFileSystem {
    files: Mutex<BTreeMap<String, Vec<u8>>>,
}

impl MemoryFileSystem {
    pub fn new() -> Self { Self { files: Mutex::new(BTreeMap::new()) } }
}

impl FileSystem for MemoryFileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files.lock().unwrap().get(&norm(path)).cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, norm(path)))
    }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        self.files.lock().unwrap().insert(norm(path), data.to_vec());
        Ok(())
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut f = self.files.lock().unwrap();
        let (fp, tp) = (norm(from), norm(to));
        let moved: Vec<String> = f.keys()
            .filter(|k| **k == fp || k.starts_with(&format!("{fp}/")))
            .cloned().collect();
        if moved.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, fp));
        }
        for k in moved {
            let data = f.remove(&k).unwrap();
            let nk = format!("{tp}{}", &k[fp.len()..]);
            f.insert(nk, data);
        }
        Ok(())
    }
    fn exists(&self, path: &Path) -> bool {
        let p = norm(path);
        let f = self.files.lock().unwrap();
        f.contains_key(&p) || f.keys().any(|k| k.starts_with(&format!("{p}/")))
    }
    fn is_file(&self, path: &Path) -> bool {
        self.files.lock().unwrap().contains_key(&norm(path))
    }
    fn is_dir(&self, path: &Path) -> bool {
        let p = norm(path);
        let f = self.files.lock().unwrap();
        !f.contains_key(&p) && f.keys().any(|k| k.starts_with(&format!("{p}/")))
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let prefix = format!("{}/", norm(path));
        let f = self.files.lock().unwrap();
        let mut kids = std::collections::BTreeSet::new();
        for k in f.keys() {
            if let Some(rest) = k.strip_prefix(&prefix) {
                let first = rest.split('/').next().unwrap();
                kids.insert(format!("{prefix}{first}"));
            }
        }
        Ok(kids.into_iter().map(PathBuf::from).collect())
    }
    fn create_dir_all(&self, _path: &Path) -> io::Result<()> { Ok(()) }
    fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        let data = self.read(from)?;
        self.write(to, &data)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        let p = norm(path);
        let mut f = self.files.lock().unwrap();
        f.retain(|k, _| *k != p && !k.starts_with(&format!("{p}/")));
        Ok(())
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
        fs.rename(Path::new("/a/b.txt"), Path::new("/a/c.txt")).unwrap();
        assert!(!fs.exists(Path::new("/a/b.txt")));
        assert_eq!(fs.read(Path::new("/a/c.txt")).unwrap(), b"hello");
        let kids = fs.read_dir(Path::new("/a")).unwrap();
        assert_eq!(kids, vec![std::path::PathBuf::from("/a/c.txt")]);
    }
}
