use crate::error::{CpmError, Result};
use crate::fs::FileSystem;
use crate::index::ProjectIndex;
use crate::model::{Change, Ctx, Move, Scope};
use crate::paths::normalize_path;
use crate::stores::registry;
use std::path::Path;

pub enum Collision {
    Refuse,
    KeepDest,
    KeepSrc,
}
pub struct PlanOpts {
    pub recursive: bool,
    pub on_collision: Collision,
    pub force: bool,
    pub scope: Scope,
}
#[derive(Debug)]
pub struct Plan {
    pub mv: Move,
    pub changes: Vec<Change>,
    pub warnings: Vec<String>,
    pub nested: Vec<String>,
    pub home: std::path::PathBuf,
}

pub fn build_plan(fs: &dyn FileSystem, home: &Path, mv: &Move, opts: &PlanOpts) -> Result<Plan> {
    // Guard: destination folder exists
    if fs.exists(Path::new(&mv.dst_abs.replace('\\', "/"))) {
        return Err(CpmError::DestinationExists(mv.dst_abs.clone()));
    }
    // Guard: worktree source (.git is a file, not a dir)
    let git = format!("{}/.git", mv.src_abs.replace('\\', "/"));
    if fs.is_file(Path::new(&git)) && !opts.force {
        return Err(CpmError::WorktreeSource(mv.src_abs.clone()));
    }
    let index = ProjectIndex::build(fs, home);
    let ctx = Ctx {
        fs,
        home: home.to_path_buf(),
        index: &index,
        scope: opts.scope,
    };
    let mut changes = Vec::new();
    let mut warnings = Vec::new();
    let mut nested = Vec::new();

    for store in registry() {
        store.probe(&ctx)?;
        for hit in store.detect(&ctx, mv)? {
            // Collision guard for claude.json destination key
            if store.id() == "claude.json" {
                if let Collision::Refuse = opts.on_collision {
                    if dest_key_exists(&ctx, mv)? {
                        return Err(CpmError::DestinationExists(format!(
                            "claude.json already has a key for {}",
                            mv.dst_abs
                        )));
                    }
                }
            }
            changes.extend(store.plan(&ctx, mv, &hit)?);
        }
    }

    // Nested project detection (keys strictly under src)
    let src_key = normalize_path(&mv.src_abs);
    for k in index.by_cwd.keys() {
        if k != &src_key && k.starts_with(&format!("{src_key}/")) {
            nested.push(k.clone());
        }
    }
    if !nested.is_empty() && !opts.recursive {
        warnings.push(format!(
            "{} nested project(s) will break unless --recursive",
            nested.len()
        ));
    }

    // Folder move is the LAST change (see apply ordering).
    changes.push(Change::MoveTree {
        from: Path::new(&mv.src_abs.replace('\\', "/")).to_path_buf(),
        to: Path::new(&mv.dst_abs.replace('\\', "/")).to_path_buf(),
    });
    Ok(Plan {
        mv: mv.clone(),
        changes,
        warnings,
        nested,
        home: home.to_path_buf(),
    })
}

fn dest_key_exists(ctx: &Ctx, mv: &Move) -> Result<bool> {
    let p = ctx.home.join(".claude.json");
    if !ctx.fs.exists(&p) {
        return Ok(false);
    }
    let v: serde_json::Value = serde_json::from_slice(&ctx.fs.read(&p)?)
        .map_err(|e| CpmError::UnrecognizedFormat(e.to_string()))?;
    let dk = normalize_path(&mv.dst_abs);
    Ok(v.get("projects")
        .and_then(|x| x.as_object())
        .map(|o| o.keys().any(|k| normalize_path(k) == dk))
        .unwrap_or(false))
}

pub fn render_plan(plan: &Plan) -> String {
    let mut s = format!("Move {} -> {}\n", plan.mv.src_abs, plan.mv.dst_abs);
    for w in &plan.warnings {
        s.push_str(&format!("  WARNING: {w}\n"));
    }
    for c in &plan.changes {
        s.push_str(&match c {
            Change::RenameDir { from, to } => {
                format!("  rename dir {} -> {}\n", from.display(), to.display())
            }
            Change::MoveTree { from, to } => {
                format!("  move tree  {} -> {}\n", from.display(), to.display())
            }
            Change::RewriteFile { path, expected, .. } => {
                format!("  rewrite    {} ({expected} edits)\n", path.display())
            }
            Change::RenameJsonKey { path, from, to, .. } => {
                format!("  json key   {} {from} -> {to}\n", path.display())
            }
            Change::RewriteJsonArrayValue { path, from, to, .. } => {
                format!("  json array {} {from} -> {to}\n", path.display())
            }
        });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemoryFileSystem;
    use crate::model::Move;
    use std::path::Path;

    fn opts() -> PlanOpts {
        PlanOpts {
            recursive: false,
            on_collision: Collision::Refuse,
            force: false,
            scope: crate::model::Scope::Standard,
        }
    }

    #[test]
    fn refuses_when_destination_folder_exists() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("E:/Projects/B/keep.txt"), b"x").unwrap();
        let mv = Move {
            src_abs: "E:\\Projects\\A".into(),
            dst_abs: "E:\\Projects\\B".into(),
        };
        let err = build_plan(&fs, Path::new("/h"), &mv, &opts()).unwrap_err();
        assert!(matches!(err, crate::error::CpmError::DestinationExists(_)));
    }

    #[test]
    fn flags_worktree_source_without_force() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("E:/Projects/A/.git"), b"gitdir: ../real")
            .unwrap(); // .git is a FILE
        let mv = Move {
            src_abs: "E:\\Projects\\A".into(),
            dst_abs: "E:\\Projects\\B".into(),
        };
        let err = build_plan(&fs, Path::new("/h"), &mv, &opts()).unwrap_err();
        assert!(matches!(err, crate::error::CpmError::WorktreeSource(_)));
    }

    #[test]
    fn render_plan_locks_format() {
        let fs = MemoryFileSystem::new();
        // Seed a claude.json with a source key so the plan emits a json key change.
        let json = r#"{"projects":{"E:\\Projects\\A":{}}}"#;
        fs.write(Path::new("/h/.claude.json"), json.as_bytes())
            .unwrap();
        let mv = Move {
            src_abs: "E:\\Projects\\A".into(),
            dst_abs: "E:\\Projects\\C".into(),
        };
        let plan = build_plan(&fs, Path::new("/h"), &mv, &opts()).unwrap();
        let got = render_plan(&plan);
        // Expected output captured from actual output on first run, then pasted back.
        // Uses assert_eq! instead of insta to avoid CRLF/acceptance tooling issues on Windows.
        // On Windows, Path::display() uses backslashes, so /h\.claude.json not /h/.claude.json.
        let expected = "Move E:\\Projects\\A -> E:\\Projects\\C\n  json key   /h\\.claude.json \"E:\\Projects\\A\": -> \"E:\\Projects\\C\":\n  move tree  E:/Projects/A -> E:/Projects/C\n";
        assert_eq!(got, expected);
    }
}
