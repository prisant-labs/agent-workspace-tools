use cpm_core::associate::{associate, AssociateOpts};
use cpm_core::fs::{FileSystem, MemoryFileSystem};
use cpm_core::plan::Collision;
use std::path::Path;

#[test]
fn associate_finds_sessions_when_source_folder_is_gone() {
    let fs = MemoryFileSystem::new();
    // project dir exists in ~/.claude but the source folder does NOT exist on disk
    fs.write(
        Path::new("/h/.claude/projects/E--Old-A/s.jsonl"),
        b"{\"cwd\":\"E:\\\\Old\\\\A\"}\n",
    )
    .unwrap();
    fs.write(Path::new("E:/New/B/keep.txt"), b"x").unwrap();
    let opts = AssociateOpts {
        reassociate: true,
        export: true,
        export_subdir: ".claude-sessions".into(),
        run_id: "T".into(),
        on_collision: Collision::Refuse,
    };
    associate(&fs, Path::new("/h"), "E:\\Old\\A", "E:\\New\\B", &opts).unwrap();
    assert!(fs.exists(Path::new("/h/.claude/projects/E--New-B/s.jsonl"))); // reassociated
    assert!(
        fs.exists(Path::new(
            "E:/New/B/.claude-sessions/projects/E--New-B/s.jsonl"
        )) || fs.exists(Path::new(
            "E:/New/B/.claude-sessions/projects/E--Old-A/s.jsonl"
        ))
    ); // exported
}
