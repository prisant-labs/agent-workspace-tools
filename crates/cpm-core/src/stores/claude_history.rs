use crate::error::{CpmError, Result};
use crate::model::{Change, Ctx, Hit, Move, Stale, Store, VerifyResult};
use crate::paths::normalize_path;
use std::path::PathBuf;

pub struct ClaudeHistory;
impl ClaudeHistory {
    const ID: &'static str = "claude.history";
    fn path(ctx: &Ctx) -> PathBuf {
        ctx.home.join(".claude").join("history.jsonl")
    }
}

impl Store for ClaudeHistory {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn probe(&self, _ctx: &Ctx) -> Result<()> {
        Ok(())
    }

    fn detect(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<Hit>> {
        // LEAD-03 fix: compare using normalize_path on BOTH sides so that all slash and
        // case variants of a path are matched by a single normalized key. If the project
        // field stores "E:\Projects\A" and the move src is "e:/projects/a", they compare
        // equal after normalization. Any variant detect finds will also be found by a
        // rewrite that normalizes before comparing, so detect and rewrite cover the same set.
        let p = Self::path(ctx);
        if !ctx.fs.exists(&p) {
            return Ok(vec![]);
        }
        let bytes = ctx.fs.read(&p)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|e| CpmError::UnrecognizedFormat(format!("history.jsonl: {e}")))?;
        let key = normalize_path(&mv.src_abs);
        let mut count = 0usize;
        for l in text.lines() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
                if let Some(pr) = v.get("project").and_then(|x| x.as_str()) {
                    if normalize_path(pr) == key {
                        count += 1;
                    }
                }
            }
        }
        Ok(if count > 0 {
            vec![Hit {
                store: Self::ID,
                detail: format!("{count} history lines"),
                target: p,
            }]
        } else {
            vec![]
        })
    }

    fn audit(&self, ctx: &Ctx) -> Result<Vec<Stale>> {
        // Report distinct project values whose path no longer exists on disk.
        // Uses lossy UTF-8 (read-only heuristic; we never re-serialize here).
        let p = Self::path(ctx);
        if !ctx.fs.exists(&p) {
            return Ok(vec![]);
        }
        let bytes = ctx.fs.read(&p)?;
        let text = String::from_utf8_lossy(&bytes);
        let mut seen = std::collections::BTreeSet::new();
        let mut stale = Vec::new();
        for l in text.lines() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
                if let Some(pr) = v.get("project").and_then(|x| x.as_str()) {
                    if seen.insert(pr.to_string()) && !ctx.fs.exists(std::path::Path::new(pr)) {
                        stale.push(Stale {
                            store: Self::ID,
                            reference: pr.to_string(),
                            location: "history.jsonl".into(),
                        });
                    }
                }
            }
        }
        Ok(stale)
    }

    fn plan(&self, _ctx: &Ctx, _mv: &Move, _hit: &Hit) -> Result<Vec<Change>> {
        Ok(vec![])
    }

    fn verify(&self, _ctx: &Ctx, _mv: &Move) -> Result<Vec<VerifyResult>> {
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use crate::index::ProjectIndex;
    use crate::model::{Ctx, Move};
    use std::path::{Path, PathBuf};

    #[test]
    fn detect_finds_matching_project_lines() {
        let fs = MemoryFileSystem::new();
        let body = "{\"project\":\"E:\\\\Projects\\\\A\",\"sessionId\":\"1\"}\n\
                    {\"project\":\"E:\\\\Projects\\\\Other\",\"sessionId\":\"2\"}\n";
        fs.write(Path::new("/h/.claude/history.jsonl"), body.as_bytes())
            .unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h"));
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
            scope: crate::model::Scope::Standard,
        };
        let mv = Move {
            src_abs: "E:\\Projects\\A".into(),
            dst_abs: "E:\\Projects\\B".into(),
        };
        let hits = ClaudeHistory.detect(&ctx, &mv).unwrap();
        assert_eq!(hits.len(), 1);
    }
}
