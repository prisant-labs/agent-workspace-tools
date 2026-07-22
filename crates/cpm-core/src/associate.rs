use crate::apply::{apply_verified, ApplyOpts};
use crate::archive::{archive_project, ArchiveOpts};
use crate::error::{CpmError, Result};
use crate::fs::FileSystem;
use crate::model::{Move, Scope};
use crate::paths::normalize_path;
use crate::plan::{build_plan, Collision, PlanOpts};
use std::path::{Path, PathBuf};

pub struct AssociateOpts {
    pub reassociate: bool,
    pub export: bool,
    pub export_subdir: String,
    pub run_id: String,
    pub on_collision: Collision,
}

pub fn associate(
    fs: &dyn FileSystem,
    home: &Path,
    from: &str,
    to: &str,
    opts: &AssociateOpts,
) -> Result<crate::report::Report> {
    if normalize_path(from) == normalize_path(to) {
        return Err(CpmError::Locked(
            "source and destination are the same project".into(),
        ));
    }
    if !opts.reassociate && !opts.export {
        return Err(CpmError::Locked(
            "nothing to do: enable --reassociate or --export".into(),
        ));
    }
    if opts.export {
        let sub = format!("{}/{}", to.replace('\\', "/"), opts.export_subdir);
        let aopts = ArchiveOpts {
            archive_dir: PathBuf::from(sub),
            render: false,
            run_token: opts.run_id.clone(),
        };
        archive_project(fs, home, from, &aopts)?;
    }
    if opts.reassociate {
        let mv = Move {
            src_abs: from.to_string(),
            dst_abs: to.to_string(),
        };
        let plan_opts = PlanOpts {
            recursive: false,
            on_collision: opts.on_collision.clone(),
            force: false,
            move_folder: false,
            scope: Scope::Standard,
        };
        let plan = build_plan(fs, home, &mv, &plan_opts)?;
        let aopts = ApplyOpts {
            run_id: opts.run_id.clone(),
            auto_rollback: true,
            force: false,
        };
        return apply_verified(&plan, fs, &std::env::temp_dir(), &aopts);
    }
    Ok(crate::report::Report {
        run_id: opts.run_id.clone(),
        applied: vec![],
        backup_dir: String::new(),
        verify: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use std::path::Path;

    // --- Finding 5 test: from == to is rejected ---

    #[test]
    fn associate_rejects_from_equal_to() {
        let fs = MemoryFileSystem::new();
        let opts = AssociateOpts {
            reassociate: true,
            export: false,
            export_subdir: ".claude-sessions".into(),
            run_id: "T".into(),
            on_collision: crate::plan::Collision::Refuse,
        };
        let err = associate(&fs, Path::new("/h"), "E:\\A", "E:\\A", &opts).unwrap_err();
        assert!(
            matches!(err, CpmError::Locked(_)),
            "expected Locked error for from==to, got {err:?}"
        );
    }

    /// Case and separator variants of the same path are also equal.
    #[test]
    fn associate_rejects_from_equal_to_case_insensitive() {
        let fs = MemoryFileSystem::new();
        let opts = AssociateOpts {
            reassociate: true,
            export: false,
            export_subdir: ".claude-sessions".into(),
            run_id: "T".into(),
            on_collision: crate::plan::Collision::Refuse,
        };
        let err = associate(
            &fs,
            Path::new("/h"),
            "E:\\Projects\\A",
            "E:/projects/a",
            &opts,
        )
        .unwrap_err();
        assert!(
            matches!(err, CpmError::Locked(_)),
            "expected Locked error for case-variant from==to, got {err:?}"
        );
    }

    #[test]
    fn export_only_copies_but_does_not_reassociate() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\A\"}\n",
        )
        .unwrap();
        fs.write(Path::new("E:/B/keep.txt"), b"x").unwrap();
        let opts = AssociateOpts {
            reassociate: false,
            export: true,
            export_subdir: ".claude-sessions".into(),
            run_id: "T".into(),
            on_collision: crate::plan::Collision::Refuse,
        };
        associate(&fs, Path::new("/h"), "E:\\A", "E:\\B", &opts).unwrap();
        assert!(fs.exists(Path::new("/h/.claude/projects/E--A/s.jsonl"))); // records untouched
        assert!(fs.exists(Path::new("E:/B/.claude-sessions/projects/E--A/s.jsonl")));
        // export copy
    }

    #[test]
    fn export_is_scoped_to_the_from_project_only() {
        // Two projects; export FROM A must NOT copy B (the plan's archive_all would - regression guard).
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\A\"}\n",
        )
        .unwrap();
        fs.write(
            Path::new("/h/.claude/projects/E--Other/o.jsonl"),
            b"{\"cwd\":\"E:\\\\Other\"}\n",
        )
        .unwrap();
        fs.write(Path::new("E:/B/keep.txt"), b"x").unwrap();
        let opts = AssociateOpts {
            reassociate: false,
            export: true,
            export_subdir: ".claude-sessions".into(),
            run_id: "T".into(),
            on_collision: crate::plan::Collision::Refuse,
        };
        associate(&fs, Path::new("/h"), "E:\\A", "E:\\B", &opts).unwrap();
        assert!(fs.exists(Path::new("E:/B/.claude-sessions/projects/E--A/s.jsonl")));
        assert!(
            !fs.exists(Path::new("E:/B/.claude-sessions/projects/E--Other/o.jsonl")),
            "export must be scoped to the from project only"
        );
    }
}
