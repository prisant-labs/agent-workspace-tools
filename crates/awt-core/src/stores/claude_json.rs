use crate::error::{AwtError, Result};
use crate::model::{Change, Ctx, Hit, Move, Stale, Store, VerifyResult};
use crate::paths::normalize_path;
use std::path::{Path, PathBuf};

/// Render `s` as a JSON string literal, surrounding quotes included, escaped exactly as a
/// JSON writer emits it.
///
/// This is load-bearing, not cosmetic. Hits and their details are built from the PARSED
/// document, where a Windows path is unescaped (`E:\a\b`), but every rewrite is a literal
/// byte splice against the RAW file, where the same path is stored escaped (`E:\\a\\b`).
/// Anchoring on the parsed form means anchoring on a byte sequence that does not occur, so
/// the count check reports `expected 1, live 0` and the run fails closed. That was AR-01,
/// the defect that failed the 2026-07-28 acceptance run: it made `apply` and `associate`
/// impossible for any project carrying a `githubRepoPaths` entry.
///
/// Forward-slash paths need no escaping and pass through unchanged, which is why the
/// majority-case `projects` keys kept working and hid the defect for so long.
///
/// Regression coverage: `crates/awt-core/tests/claude_json_escaping.rs`.
fn json_string_literal(s: &str) -> String {
    serde_json::Value::String(s.to_string()).to_string()
}

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
            .map_err(|e| AwtError::UnrecognizedFormat(format!("claude.json parse: {e}")))?;
        match v.get("projects") {
            Some(serde_json::Value::Object(_)) | None => Ok(()),
            Some(_) => Err(AwtError::UnrecognizedFormat(
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
            .map_err(|e| AwtError::UnrecognizedFormat(e.to_string()))?;
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
    /// and hits real disk when called from `awt doctor` on RealFileSystem.
    fn audit(&self, ctx: &Ctx) -> Result<Vec<Stale>> {
        let p = Self::path(ctx);
        if !ctx.fs.exists(&p) {
            return Ok(vec![]);
        }
        let bytes = ctx.fs.read(&p)?;
        let v: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| AwtError::UnrecognizedFormat(e.to_string()))?;
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

    /// `githubRepoPaths` values are expected to be arrays of path strings. Anything else is
    /// skipped by `detect` and `audit`, which means a path recorded under a malformed entry is
    /// never rewritten by a move. That is the safe behavior, but doing it without a word is not:
    /// a silently skipped entry looks exactly like a tool that failed to notice.
    fn warn(&self, ctx: &Ctx) -> Result<Vec<String>> {
        let p = Self::path(ctx);
        if !ctx.fs.exists(&p) {
            return Ok(vec![]);
        }
        let bytes = ctx.fs.read(&p)?;
        // A parse failure is probe's business (exit 4), not a warning. Stay quiet here.
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return Ok(vec![]);
        };
        let mut out = Vec::new();
        if let Some(grp) = v.get("githubRepoPaths").and_then(|x| x.as_object()) {
            for (slug, val) in grp {
                if val.as_array().is_none() {
                    out.push(format!(
                        "claude.json githubRepoPaths[{slug}] is {}, expected an array of paths; \
                         it will not be examined or rewritten",
                        match val {
                            serde_json::Value::String(_) => "a string",
                            serde_json::Value::Number(_) => "a number",
                            serde_json::Value::Bool(_) => "a boolean",
                            serde_json::Value::Null => "null",
                            serde_json::Value::Object(_) => "an object",
                            serde_json::Value::Array(_) => unreachable!(),
                        }
                    ));
                }
            }
        }
        Ok(out)
    }

    fn plan(&self, _ctx: &Ctx, mv: &Move, hit: &Hit) -> Result<Vec<Change>> {
        let p = hit.target.clone();
        if let Some(rest) = hit.detail.strip_prefix("projects key ") {
            let to = crate::paths_dst_key(rest, &mv.src_abs, &mv.dst_abs);
            return Ok(vec![Change::RenameJsonKey {
                path: p,
                from: format!("{}:", json_string_literal(rest)),
                to: format!("{}:", json_string_literal(&to)),
                expected: 1,
            }]);
        }
        if hit.detail.starts_with("githubRepoPaths") {
            // detail formatted as: githubRepoPaths[slug] = <value>
            let value = hit.detail.split(" = ").nth(1).unwrap().to_string();
            let to = crate::paths_dst_key(&value, &mv.src_abs, &mv.dst_abs);
            return Ok(vec![Change::RewriteJsonArrayValue {
                path: p,
                from: json_string_literal(&value),
                to: json_string_literal(&to),
                expected: 1,
            }]);
        }
        Ok(vec![])
    }

    fn verify(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<VerifyResult>> {
        let p = Self::path(ctx);
        if !ctx.fs.exists(&p) {
            return Ok(vec![]);
        }
        let bytes = ctx.fs.read(&p)?;
        let v = match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(v) => v,
            Err(_) => {
                return Ok(vec![VerifyResult {
                    check: "claude.json parses".into(),
                    ok: false,
                    detail: p.to_string_lossy().into_owned(),
                }]);
            }
        };
        let old = normalize_path(&mv.src_abs);
        let old_key_found = v
            .get("projects")
            .and_then(|x| x.as_object())
            .map(|obj| obj.keys().any(|k| normalize_path(k) == old))
            .unwrap_or(false);
        Ok(vec![
            VerifyResult {
                check: "claude.json parses".into(),
                ok: true,
                detail: String::new(),
            },
            VerifyResult {
                check: "no projects key for old path".into(),
                ok: !old_key_found,
                detail: if old_key_found {
                    mv.src_abs.clone()
                } else {
                    String::new()
                },
            },
        ])
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
        ProjectIndex::build(fs, Path::new("/h")).unwrap()
    }

    #[test]
    fn probe_rejects_non_object_projects() {
        let fs = MemoryFileSystem::new();
        let idx = ctx_with("{\"projects\": 5}", &fs);
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
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
        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
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
        };
        let stale = ClaudeJson.audit(&ctx).unwrap();
        assert!(stale.iter().any(|s| s.reference.contains("Gone")));
    }
}
