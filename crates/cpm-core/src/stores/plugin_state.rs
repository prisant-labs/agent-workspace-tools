use crate::error::Result;
use crate::model::{Change, Ctx, Hit, Move, Stale, Store, VerifyResult};
use sha2::{Digest, Sha256};
use std::fmt::Write as FmtWrite;
use std::path::{Path, PathBuf};

pub struct PluginState;
impl PluginState {
    const ID: &'static str = "plugin.state";
}

/// Codex plugin state dir suffix: sha256 of the abs path (backslash form), first 16 hex chars.
///
/// The hash is over the EXACT bytes of the original cwd as recorded in transcripts.
/// A lowercased or slash-flipped value produces a different digest and silently finds nothing.
/// Never pass a `normalize_path`-ed value here.
pub fn state_hash(abs_backslash: &str) -> String {
    let digest = Sha256::digest(abs_backslash.as_bytes());
    hex_lower(&digest)[..16].to_string()
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(s, "{b:02x}").unwrap();
    }
    s
}

impl Store for PluginState {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn probe(&self, _ctx: &Ctx) -> Result<()> {
        Ok(())
    }

    fn detect(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<Hit>> {
        // Suffix is sha256 of the EXACT backslash-form src path (first 16 hex chars).
        // We search every plugin's state dir for any entry whose name ends with that suffix.
        let suffix = state_hash(&mv.src_abs);
        let mut hits = Vec::new();
        let data = ctx.home.join(".claude").join("plugins").join("data");
        for plugin in ctx.fs.read_dir(&data).unwrap_or_default() {
            let state = plugin.join("state");
            for entry in ctx.fs.read_dir(&state).unwrap_or_default() {
                if let Some(name) = entry.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(&suffix) {
                        hits.push(Hit {
                            store: Self::ID,
                            detail: name.to_string(),
                            target: entry.clone(),
                        });
                    }
                }
            }
        }
        Ok(hits)
    }

    fn audit(&self, ctx: &Ctx) -> Result<Vec<Stale>> {
        // The search runs from the paths we know, NOT from the dirs we find.
        //
        // "I cannot explain this dir" and "this dir is stale" are different claims, and
        // conflating them is how the earlier version reported a live Codex broker dir as
        // orphaned project state: `1.0.4-c2306d34173b4c6b` has the same `<name>-<16 hex>`
        // shape, but its suffix is a version hash, not a path hash (audit finding C-1).
        // Anything under a plugin's state dir that we cannot tie to a recorded path is
        // simply not ours to judge, so it goes unmentioned rather than guessed at.
        //
        // A dir is an orphan only when both halves hold: its suffix is the hash of a cwd
        // we actually recorded, and that cwd is gone from disk. The orphan is keyed by the
        // OLD path, and `ProjectIndex.cwds` keeps every distinct recorded cwd including the
        // dead ones left behind by earlier moves - those are what find it (LEAD-04).
        let mut dead: Vec<&str> = ctx
            .index
            .cwds
            .iter()
            .map(|c| c.as_str())
            .filter(|c| !ctx.fs.is_dir(Path::new(c)))
            .collect();
        dead.sort_unstable();
        dead.dedup();

        // Read each plugin's state dir once, then match the known-dead hashes against it.
        let data = ctx.home.join(".claude").join("plugins").join("data");
        let mut entries: Vec<(String, PathBuf)> = Vec::new();
        for plugin in ctx.fs.read_dir(&data).unwrap_or_default() {
            let state = plugin.join("state");
            for entry in ctx.fs.read_dir(&state).unwrap_or_default() {
                if let Some(name) = entry.file_name().and_then(|n| n.to_str()) {
                    entries.push((name.to_string(), state.clone()));
                }
            }
        }

        let mut stale = Vec::new();
        for cwd in dead {
            let suffix = state_hash(cwd);
            for (name, state) in &entries {
                if name.ends_with(&suffix) {
                    stale.push(Stale {
                        store: Self::ID,
                        reference: name.clone(),
                        location: state.to_string_lossy().into_owned(),
                    });
                }
            }
        }
        Ok(stale)
    }

    fn plan(&self, _ctx: &Ctx, mv: &Move, hit: &Hit) -> Result<Vec<Change>> {
        // Convention <basename>-<sha256(abs)[:16]> (DESIGN.md Section 2 item 4). BOTH parts
        // derive from the DESTINATION: a plugin recomputing state for the new path looks for
        // basename(dst)-hash(dst), so keeping the OLD basename would re-orphan the dir (LEAD-04).
        let basename = |p: &str| p.rsplit(['\\', '/']).next().unwrap_or(p).to_string();
        let new_name = format!("{}-{}", basename(&mv.dst_abs), state_hash(&mv.dst_abs));
        let parent = hit.target.parent().unwrap().to_path_buf();
        Ok(vec![Change::RenameDir {
            from: hit.target.clone(),
            to: parent.join(new_name),
        }])
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
    fn hash_matches_real_codex_dir_suffix() {
        // verified: sha256("E:\Projects\Github Repos\markdown-for-humans")[:16]
        // The fixture dir test/fixtures/plugin-state/markdown-for-humans-e854827f52137cd9
        // is a real dir from an actual move and confirms this digest.
        assert_eq!(
            state_hash("E:\\Projects\\Github Repos\\markdown-for-humans"),
            "e854827f52137cd9"
        );
    }

    fn cwd_line(cwd: &str) -> String {
        format!(
            "{{\"type\":\"user\",\"cwd\":\"{}\"}}\n",
            cwd.replace('\\', "\\\\")
        )
    }

    #[test]
    fn plan_renames_state_dir_to_destination_basename_and_hash() {
        let fs = MemoryFileSystem::new();
        let src_suffix = state_hash("E:\\Projects\\A");
        let state_dir_path =
            format!("/h/.claude/plugins/data/codex/state/A-{src_suffix}/state.json");
        fs.write(Path::new(&state_dir_path), b"{}").unwrap();
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
        let hits = PluginState.detect(&ctx, &mv).unwrap();
        assert_eq!(hits.len(), 1, "detect found no hits");
        let changes = PluginState.plan(&ctx, &mv, &hits[0]).unwrap();
        let expected_suffix = format!("B-{}", state_hash("E:\\Projects\\B"));
        if let crate::model::Change::RenameDir { to, .. } = &changes[0] {
            let name = to.file_name().unwrap().to_str().unwrap();
            assert!(name.ends_with(&expected_suffix), "got: {name}");
        } else {
            panic!("expected RenameDir");
        }
    }

    /// A state dir is an orphan only when we can EXPLAIN it: its suffix is the hash of
    /// a cwd we actually recorded, and that cwd is gone from disk. A dir we cannot
    /// explain is not evidence of staleness - it is evidence of our own ignorance.
    ///
    /// The fixture holds all three populations at once, because a test that only proves
    /// "the orphan is flagged" also passes when audit flags everything (audit finding C-2).
    #[test]
    fn audit_flags_only_the_dead_project_whose_path_it_can_explain() {
        let fs = MemoryFileSystem::new();

        // 1. LIVE project: transcript records it AND the path exists on disk.
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-Live/s.jsonl"),
            cwd_line("E:\\Projects\\Live").as_bytes(),
        )
        .unwrap();
        fs.write(Path::new("E:\\Projects\\Live\\README.md"), b"live")
            .unwrap();
        fs.write(
            Path::new(&format!(
                "/h/.claude/plugins/data/codex/state/live-{}/state.json",
                state_hash("E:\\Projects\\Live")
            )),
            b"{}",
        )
        .unwrap();

        // 2. DEAD project: transcript records the pre-move path, which is gone from disk.
        //    Its state dir is the real orphan - the one thing audit must report.
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-Github-Repos-markdown-for-humans/s.jsonl"),
            cwd_line("E:\\Projects\\Github Repos\\markdown-for-humans").as_bytes(),
        )
        .unwrap();
        fs.write(
            Path::new(
                "/h/.claude/plugins/data/codex/state/markdown-for-humans-e854827f52137cd9/state.json",
            ),
            b"{}",
        )
        .unwrap();

        // 3. UNEXPLAINABLE dir: name-shape matches `<name>-<16 hex>` but the suffix is the
        //    hash of nothing we recorded. This is the live Codex broker on the author's
        //    real machine (audit finding C-1) - flagging it would invite deleting live state.
        fs.write(
            Path::new("/h/.claude/plugins/data/codex/state/1.0.4-c2306d34173b4c6b/broker.json"),
            b"{}",
        )
        .unwrap();

        let idx = ProjectIndex::build(&fs, Path::new("/h"));
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
            scope: crate::model::Scope::Standard,
        };
        let stale = PluginState.audit(&ctx).unwrap();

        let refs: Vec<&str> = stale.iter().map(|s| s.reference.as_str()).collect();
        assert_eq!(
            refs,
            vec!["markdown-for-humans-e854827f52137cd9"],
            "audit must report exactly the explainable dead project, not the live one and \
             not the broker it cannot explain"
        );
    }
}
