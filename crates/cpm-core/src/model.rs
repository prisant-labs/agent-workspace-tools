use crate::error::Result;
use crate::fs::FileSystem;
use crate::index::ProjectIndex;
use crate::rewrite::RewriteRule;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Move {
    pub src_abs: String,
    pub dst_abs: String,
}

/// Rewrite tier. Order matters: Minimal < Standard < Full (derived Ord follows the
/// declaration order), so `scope >= Scope::Standard` gates the transcript rewrites and
/// `scope == Scope::Full` additionally emits sidecar rewrites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    Minimal,
    Standard,
    Full,
}

pub struct Ctx<'a> {
    pub fs: &'a dyn FileSystem,
    pub home: PathBuf,
    pub index: &'a ProjectIndex,
    pub scope: Scope,
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub store: &'static str,
    pub detail: String,
    pub target: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Stale {
    pub store: &'static str,
    pub reference: String,
    pub location: String,
}

#[derive(Debug, Clone)]
pub enum Change {
    RenameDir {
        from: PathBuf,
        to: PathBuf,
    },
    MoveTree {
        from: PathBuf,
        to: PathBuf,
    },
    RewriteFile {
        path: PathBuf,
        rules: Vec<RewriteRule>,
        expected: usize,
    },
    RenameJsonKey {
        path: PathBuf,
        from: String,
        to: String,
        expected: usize,
    },
    RewriteJsonArrayValue {
        path: PathBuf,
        from: String,
        to: String,
        expected: usize,
    },
}

#[derive(Debug, Clone)]
pub struct Applied {
    pub change: String,
    pub counts: usize,
}

#[derive(Debug, Clone)]
pub struct VerifyResult {
    pub check: String,
    pub ok: bool,
    pub detail: String,
}

impl<'a> Ctx<'a> {
    pub fn fs_walk_text(&self, root: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for child in self.fs.read_dir(&dir).unwrap_or_default() {
                if self.fs.is_dir(&child) {
                    stack.push(child);
                } else {
                    out.push(child);
                }
            }
        }
        out
    }
}

pub trait Store {
    fn id(&self) -> &'static str;
    fn probe(&self, ctx: &Ctx) -> Result<()>;
    fn detect(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<Hit>>;
    fn audit(&self, ctx: &Ctx) -> Result<Vec<Stale>>;
    fn plan(&self, ctx: &Ctx, mv: &Move, hit: &Hit) -> Result<Vec<Change>>;
    fn verify(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<VerifyResult>>;
}
