use crate::error::{CpmError, Result};
use crate::model::{Change, Ctx, Hit, Move, Stale, Store, VerifyResult};
use crate::paths::normalize_path;
use std::path::{Path, PathBuf};

pub struct ClaudeJson;

impl ClaudeJson {
    const ID: &'static str = "claude.json";

    fn path(ctx: &Ctx) -> PathBuf {
        ctx.home.join(".claude.json")
    }
}

impl Store for ClaudeJson {
    fn id(&self) -> &'static str {
        Self::ID
    }

    /// Validates that `~/.claude.json` exists and (if it does) that the
    /// `projects` field is a JSON object. Returns Ok if the file is absent -
    /// that is a valid empty state, not an error.
    fn probe(&self, ctx: &Ctx) -> Result<()> {
        let p = Self::path(ctx);
        if !ctx.fs.exists(&p) {
            return Ok(());
        }
        let bytes = ctx.fs.read(&p)?;
        let v: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| CpmError::UnrecognizedFormat(format!("claude.json parse: {e}")))?;
        match v.get("projects") {
            Some(serde_json::Value::Object(_)) | None => Ok(()),
            Some(_) => Err(CpmError::UnrecognizedFormat(
                "claude.json projects is not an object".into(),
            )),
        }
    }

    /// Finds every `projects` key whose normalized form matches `mv.src_abs`,
    /// plus every `githubRepoPaths` array element equal to `mv.src_abs`
    /// (any slash/case variant).
    fn detect(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<Hit>> {
        let p = Self::path(ctx);
        if !ctx.fs.exists(&p) {
            return Ok(vec![]);
        }
        let bytes = ctx.fs.read(&p)?;
        let v: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| CpmError::UnrecognizedFormat(e.to_string()))?;
        let key = normalize_path(&mv.src_abs);
        let mut hits = Vec::new();

        if let Some(obj) = v.get("projects").and_then(|x| x.as_object()) {
            for k in obj.keys() {
                if normalize_path(k) == key {
                    hits.push(Hit {
                        store: Self::ID,
                        detail: format!("projects key {k}"),
                        target: p.clone(),
                    });
                }
            }
        }

        if let Some(grp) = v.get("githubRepoPaths").and_then(|x| x.as_object()) {
            for (slug, arr) in grp {
                if let Some(a) = arr.as_array() {
                    for elem in a {
                        if let Some(s) = elem.as_str() {
                            if normalize_path(s) == key {
                                hits.push(Hit {
                                    store: Self::ID,
                                    detail: format!("githubRepoPaths[{slug}] = {s}"),
                                    target: p.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(hits)
    }

    /// Reports `projects` keys and `githubRepoPaths` array values whose path
    /// no longer exists on the injected filesystem. Routes existence checks
    /// through `ctx.fs` so the check is unit-testable against MemoryFileSystem
    /// and hits real disk when called from `cpm doctor` on RealFileSystem.
    fn audit(&self, ctx: &Ctx) -> Result<Vec<Stale>> {
        let p = Self::path(ctx);
        if !ctx.fs.exists(&p) {
            return Ok(vec![]);
        }
        let bytes = ctx.fs.read(&p)?;
        let v: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| CpmError::UnrecognizedFormat(e.to_string()))?;
        let mut stale = Vec::new();

        if let Some(obj) = v.get("projects").and_then(|x| x.as_object()) {
            for k in obj.keys() {
                if !ctx.fs.exists(Path::new(k.as_str())) {
                    stale.push(Stale {
                        store: Self::ID,
                        reference: k.to_string(),
                        location: "projects".into(),
                    });
                }
            }
        }

        if let Some(grp) = v.get("githubRepoPaths").and_then(|x| x.as_object()) {
            for (slug, arr) in grp {
                if let Some(a) = arr.as_array() {
                    for elem in a {
                        if let Some(s) = elem.as_str() {
                            if !ctx.fs.exists(Path::new(s)) {
                                stale.push(Stale {
                                    store: Self::ID,
                                    reference: s.to_string(),
                                    location: format!("githubRepoPaths[{slug}]"),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(stale)
    }

    fn plan(&self, _ctx: &Ctx, mv: &Move, hit: &Hit) -> Result<Vec<Change>> {
        let p = hit.target.clone();
        if let Some(rest) = hit.detail.strip_prefix("projects key ") {
            let to = crate::paths_dst_key(rest, &mv.src_abs, &mv.dst_abs);
            return Ok(vec![Change::RenameJsonKey {
                path: p,
                from: format!("\"{rest}\":"),
                to: format!("\"{to}\":"),
                expected: 1,
            }]);
        }
        if hit.detail.starts_with("githubRepoPaths") {
            // detail formatted as: githubRepoPaths[slug] = <value>
            let value = hit.detail.split(" = ").nth(1).unwrap().to_string();
            let to = crate::paths_dst_key(&value, &mv.src_abs, &mv.dst_abs);
            return Ok(vec![Change::RewriteJsonArrayValue {
                path: p,
                from: format!("\"{value}\""),
                to: format!("\"{to}\""),
                expected: 1,
            }]);
        }
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

    fn ctx_with(json: &str, fs: &MemoryFileSystem) -> ProjectIndex {
        fs.write(Path::new("/h/.claude.json"), json.as_bytes())
            .unwrap();
        ProjectIndex::build(fs, Path::new("/h"))
    }

    #[test]
    fn probe_rejects_non_object_projects() {
        let fs = MemoryFileSystem::new();
        let idx = ctx_with("{\"projects\": 5}", &fs);
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
            scope: crate::model::Scope::Standard,
        };
        assert!(ClaudeJson.probe(&ctx).is_err());
    }

    #[test]
    fn detect_counts_key_variants() {
        let fs = MemoryFileSystem::new();
        let json = r#"{"projects":{"E:\\Projects\\A":{},"E:/Projects/A":{}},"githubRepoPaths":{"o/r":["E:\\Projects\\A"]}}"#;
        let idx = ctx_with(json, &fs);
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
        let hits = ClaudeJson.detect(&ctx, &mv).unwrap();
        // 2 key variants + 1 githubRepoPaths element
        assert_eq!(hits.len(), 3);
    }

    #[test]
    fn plan_emits_key_rename_and_array_value_rewrite() {
        let fs = MemoryFileSystem::new();
        let json =
            r#"{"projects":{"E:\\Projects\\A":{}},"githubRepoPaths":{"o/r":["E:\\Projects\\A"]}}"#;
        fs.write(Path::new("/h/.claude.json"), json.as_bytes())
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
        let hits = ClaudeJson.detect(&ctx, &mv).unwrap();
        assert_eq!(hits.len(), 2);
        let key_hit = hits
            .iter()
            .find(|h| h.detail.starts_with("projects key"))
            .unwrap();
        let arr_hit = hits
            .iter()
            .find(|h| h.detail.starts_with("githubRepoPaths"))
            .unwrap();
        let key_changes = ClaudeJson.plan(&ctx, &mv, key_hit).unwrap();
        let arr_changes = ClaudeJson.plan(&ctx, &mv, arr_hit).unwrap();
        assert!(matches!(
            key_changes[0],
            crate::model::Change::RenameJsonKey { .. }
        ));
        assert!(matches!(
            arr_changes[0],
            crate::model::Change::RewriteJsonArrayValue { .. }
        ));
    }

    #[test]
    fn audit_reports_stale_key_absent_from_injected_fs() {
        let fs = MemoryFileSystem::new();
        // a projects key whose path is absent from the injected FS -> reported stale
        let idx = ctx_with(r#"{"projects":{"E:\\Gone\\P":{}}}"#, &fs);
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
            scope: crate::model::Scope::Standard,
        };
        let stale = ClaudeJson.audit(&ctx).unwrap();
        assert!(stale.iter().any(|s| s.reference.contains("Gone")));
    }
}
