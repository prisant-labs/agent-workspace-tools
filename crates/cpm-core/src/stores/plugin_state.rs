use crate::error::Result;
use crate::model::{Change, Ctx, Hit, Move, Stale, Store, VerifyResult};
use sha2::{Digest, Sha256};
use std::fmt::Write as FmtWrite;

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
        // Build the set of all known 16-hex suffixes from every original cwd we have seen,
        // including stale ones. A plugin dir is an orphan if its suffix is NOT in this set.
        //
        // LEAD-04 fix: the orphaned dir is keyed by the hash of the OLD path. ProjectIndex.cwds
        // holds every distinct original cwd found across all transcripts, INCLUDING stale ones
        // from previous moves. Those stale cwds are the ones that will find the orphans.
        let known: std::collections::BTreeSet<String> =
            ctx.index.cwds.iter().map(|c| state_hash(c)).collect();
        let mut stale = Vec::new();
        let data = ctx.home.join(".claude").join("plugins").join("data");
        for plugin in ctx.fs.read_dir(&data).unwrap_or_default() {
            let state = plugin.join("state");
            for entry in ctx.fs.read_dir(&state).unwrap_or_default() {
                if let Some(name) = entry.file_name().and_then(|n| n.to_str()) {
                    if let Some((_, suffix)) = name.rsplit_once('-') {
                        if suffix.len() == 16
                            && suffix.chars().all(|c| c.is_ascii_hexdigit())
                            && !known.contains(suffix)
                        {
                            stale.push(Stale {
                                store: Self::ID,
                                reference: name.to_string(),
                                location: state.to_string_lossy().into_owned(),
                            });
                        }
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
    use crate::model::Ctx;
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

    #[test]
    fn audit_flags_orphan_state_dir() {
        let fs = MemoryFileSystem::new();
        // An orphan plugin state dir whose 16-hex suffix matches NO live project (the real
        // codex suffix for the pre-move path). No transcripts here -> cwds is empty -> stale.
        fs.write(
            Path::new(
                "/h/.claude/plugins/data/codex/state/markdown-for-humans-e854827f52137cd9/state.json",
            ),
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
        assert!(stale
            .iter()
            .any(|s| s.reference.ends_with("e854827f52137cd9")));
    }
}
