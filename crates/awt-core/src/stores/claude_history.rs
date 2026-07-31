use crate::error::{AwtError, Result};
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
            .map_err(|e| AwtError::UnrecognizedFormat(format!("history.jsonl: {e}")))?;
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

    fn plan(&self, ctx: &Ctx, mv: &Move, hit: &Hit) -> Result<Vec<Change>> {
        let esc = |p: &str| p.replace('\\', "\\\\");
        let key = normalize_path(&mv.src_abs);
        let bytes = ctx.fs.read(&hit.target)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|e| AwtError::UnrecognizedFormat(format!("history.jsonl: {e}")))?;
        // One rule per DISTINCT stored `project` form that normalizes to src, each mapped to
        // dst preserving that form's separator style (mirrors claude_json's dst_key, LEAD-03).
        let mut forms = std::collections::BTreeSet::new();
        for l in text.lines() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
                if let Some(pr) = v.get("project").and_then(|x| x.as_str()) {
                    if normalize_path(pr) == key {
                        forms.insert(pr.to_string());
                    }
                }
            }
        }
        let rules: Vec<crate::rewrite::RewriteRule> = forms
            .iter()
            .map(|f| crate::rewrite::RewriteRule {
                find: format!("\"project\":\"{}\"", esc(f)),
                replace: format!(
                    "\"project\":\"{}\"",
                    esc(&crate::paths::dst_key(f, &mv.src_abs, &mv.dst_abs))
                ),
            })
            .collect();
        // expected = sum of dry-run counts across every variant rule
        let (_, n) = crate::rewrite::anchored_rewrite(text, &rules);
        Ok(vec![Change::RewriteFile {
            path: hit.target.clone(),
            rules,
            expected: n,
        }])
    }

    fn verify(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<VerifyResult>> {
        let p = Self::path(ctx);
        if !ctx.fs.exists(&p) {
            return Ok(vec![]);
        }
        let bytes = ctx.fs.read(&p)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|e| AwtError::UnrecognizedFormat(format!("history.jsonl: {e}")))?;
        let old = normalize_path(&mv.src_abs);
        // A malformed line is a verification FAILURE, not something to skip (AC-57). The
        // previous filter_map silently dropped unparseable lines, so a rewrite that corrupted
        // a line - the exact accident this check exists to catch - verified green.
        let mut count = 0usize;
        let mut malformed = 0usize;
        let mut first_bad = 0usize;
        for (i, l) in text.lines().enumerate() {
            if l.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<serde_json::Value>(l) {
                Err(_) => {
                    if malformed == 0 {
                        first_bad = i + 1;
                    }
                    malformed += 1;
                }
                Ok(v) => {
                    if v.get("project")
                        .and_then(|x| x.as_str())
                        .map(|pr| normalize_path(pr) == old)
                        .unwrap_or(false)
                    {
                        count += 1;
                    }
                }
            }
        }
        let mut out = vec![VerifyResult {
            check: "zero history lines for old path".into(),
            ok: count == 0,
            detail: format!("{count} lines"),
        }];
        out.push(VerifyResult {
            check: "every history line parses".into(),
            ok: malformed == 0,
            detail: if malformed == 0 {
                "all lines parse".into()
            } else {
                format!("{malformed} line(s) failed to parse, first at line {first_bad}")
            },
        });
        Ok(out)
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
    fn plan_emits_one_rule_per_variant_form() {
        let fs = MemoryFileSystem::new();
        // two DISTINCT stored forms of the same path: backslash and forward-slash
        let body = "{\"project\":\"E:\\\\Projects\\\\A\"}\n{\"project\":\"E:/Projects/A\"}\n";
        fs.write(Path::new("/h/.claude/history.jsonl"), body.as_bytes())
            .unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
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
        let hit = ClaudeHistory.detect(&ctx, &mv).unwrap().remove(0);
        let changes = ClaudeHistory.plan(&ctx, &mv, &hit).unwrap();
        if let crate::model::Change::RewriteFile {
            rules, expected, ..
        } = &changes[0]
        {
            assert_eq!(rules.len(), 2); // one rule per distinct variant form
            assert_eq!(*expected, 2); // both lines rewritten
        } else {
            panic!("expected RewriteFile");
        }
    }

    #[test]
    fn detect_finds_matching_project_lines() {
        let fs = MemoryFileSystem::new();
        let body = "{\"project\":\"E:\\\\Projects\\\\A\",\"sessionId\":\"1\"}\n\
                    {\"project\":\"E:\\\\Projects\\\\Other\",\"sessionId\":\"2\"}\n";
        fs.write(Path::new("/h/.claude/history.jsonl"), body.as_bytes())
            .unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
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
