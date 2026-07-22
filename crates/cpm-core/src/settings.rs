use crate::error::{CpmError, Result};
use crate::fs::FileSystem;
use std::path::{Path, PathBuf};

const CPM_HOOK_MARKER: &str = "archive --hook-stdin";
/// Legacy marker used by the earlier broken hook that passed a session id via env var.
const CPM_HOOK_LEGACY_MARKER: &str = "archive --session";

fn settings_path(home: &Path) -> PathBuf {
    home.join(".claude").join("settings.json")
}

fn load_settings(fs: &dyn FileSystem, home: &Path) -> serde_json::Value {
    let path = settings_path(home);
    if let Ok(bytes) = fs.read(&path) {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            return v;
        }
    }
    serde_json::Value::Object(serde_json::Map::new())
}

fn save_settings(fs: &dyn FileSystem, home: &Path, v: &serde_json::Value) -> Result<()> {
    let path = settings_path(home);
    fs.write(&path, serde_json::to_vec_pretty(v).unwrap().as_slice())?;
    Ok(())
}

/// Set cleanupPeriodDays in ~/.claude/settings.json.
/// Refuses days=0 without the force_zero opt-in because issue #23710 documents
/// that 0 triggers a regression in Claude Code's cleanup scheduler.
pub fn set_retention(fs: &dyn FileSystem, home: &Path, days: u32, force_zero: bool) -> Result<()> {
    if days == 0 && !force_zero {
        return Err(CpmError::Locked(
            "cleanupPeriodDays=0 triggers issue #23710; pass --force-zero to override".into(),
        ));
    }
    let mut v = load_settings(fs, home);
    v["cleanupPeriodDays"] = serde_json::json!(days);
    save_settings(fs, home, &v)?;
    Ok(())
}

/// Returns true if a hook group entry is one installed by cpm archive (current or legacy).
fn is_cpm_entry(entry: &serde_json::Value) -> bool {
    entry["hooks"]
        .as_array()
        .map(|hooks| {
            hooks.iter().any(|h| {
                h["command"]
                    .as_str()
                    .map(|c| c.contains(CPM_HOOK_MARKER) || c.contains(CPM_HOOK_LEGACY_MARKER))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// Install a SessionEnd hook in ~/.claude/settings.json.
/// The installed command reads hook context from stdin as JSON (Claude Code's confirmed
/// contract) rather than env vars, and archives the transcript to archive_dir.
/// Idempotent: any existing cpm archive entry is removed before the new one is added.
pub fn install_session_end_hook(
    fs: &dyn FileSystem,
    home: &Path,
    exe_path: &Path,
    archive_dir: &Path,
) -> Result<()> {
    let mut v = load_settings(fs, home);
    let cmd = format!(
        "{} archive --hook-stdin --archive-dir \"{}\"",
        exe_path.to_string_lossy(),
        archive_dir.to_string_lossy()
    );
    let new_entry = serde_json::json!({
        "matcher": "",
        "hooks": [{"type": "command", "command": cmd}]
    });
    if let Some(arr) = v
        .get_mut("hooks")
        .and_then(|h| h.get_mut("SessionEnd"))
        .and_then(|se| se.as_array_mut())
    {
        arr.retain(|e| !is_cpm_entry(e));
        arr.push(new_entry);
    } else {
        v["hooks"]["SessionEnd"] = serde_json::json!([new_entry]);
    }
    save_settings(fs, home, &v)?;
    Ok(())
}

/// Remove any cpm archive SessionEnd hook entries from ~/.claude/settings.json.
/// Leaves all other SessionEnd entries untouched.
pub fn uninstall_session_end_hook(fs: &dyn FileSystem, home: &Path) -> Result<()> {
    let mut v = load_settings(fs, home);
    if let Some(arr) = v
        .get_mut("hooks")
        .and_then(|h| h.get_mut("SessionEnd"))
        .and_then(|se| se.as_array_mut())
    {
        arr.retain(|e| !is_cpm_entry(e));
    }
    save_settings(fs, home, &v)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use std::path::Path;

    #[test]
    fn set_retention_refuses_zero_without_optin() {
        let fs = MemoryFileSystem::new();
        let result = set_retention(&fs, Path::new("/h"), 0, false);
        assert!(result.is_err());
        assert!(!fs.exists(Path::new("/h/.claude/settings.json")));
    }

    /// The installed hook command must carry --hook-stdin and --archive-dir,
    /// and must NOT use the defunct $CLAUDE_SESSION_ID env var.
    #[test]
    fn install_hook_emits_hook_stdin_and_archive_dir() {
        let fs = MemoryFileSystem::new();
        install_session_end_hook(
            &fs,
            Path::new("/h"),
            Path::new("/usr/bin/cpm"),
            Path::new("/archive"),
        )
        .unwrap();
        let v: serde_json::Value =
            serde_json::from_slice(&fs.read(Path::new("/h/.claude/settings.json")).unwrap())
                .unwrap();
        let cmd = v["hooks"]["SessionEnd"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(
            cmd.contains("--hook-stdin"),
            "installed command must contain --hook-stdin; got: {cmd}"
        );
        assert!(
            cmd.contains("--archive-dir"),
            "installed command must contain --archive-dir; got: {cmd}"
        );
        assert!(
            !cmd.contains("$CLAUDE_SESSION_ID"),
            "installed command must not reference $CLAUDE_SESSION_ID; got: {cmd}"
        );
    }

    // --- Finding 1 test: legacy hook is also recognized and removed ---

    /// A settings.json with BOTH a legacy cpm entry (archive --session) and an unrelated
    /// user entry: after uninstall the legacy cpm entry is gone and the user entry survives.
    #[test]
    fn uninstall_removes_legacy_hook_and_preserves_user_entry() {
        let fs = MemoryFileSystem::new();
        let settings = serde_json::json!({
            "hooks": {
                "SessionEnd": [
                    {
                        "matcher": "",
                        "hooks": [{
                            "type": "command",
                            "command": "cpm archive --session \"$CLAUDE_SESSION_ID\" --archive-dir \"/archive\""
                        }]
                    },
                    {
                        "matcher": "",
                        "hooks": [{"type": "command", "command": "my-other-tool"}]
                    }
                ]
            }
        });
        fs.write(
            Path::new("/h/.claude/settings.json"),
            serde_json::to_vec_pretty(&settings).unwrap().as_slice(),
        )
        .unwrap();
        uninstall_session_end_hook(&fs, Path::new("/h")).unwrap();
        let v: serde_json::Value =
            serde_json::from_slice(&fs.read(Path::new("/h/.claude/settings.json")).unwrap())
                .unwrap();
        let ses_end = v["hooks"]["SessionEnd"].as_array().unwrap();
        assert_eq!(ses_end.len(), 1, "legacy cpm entry must be removed");
        let remaining_cmd = ses_end[0]["hooks"][0]["command"].as_str().unwrap();
        assert!(
            remaining_cmd.contains("my-other-tool"),
            "user entry must survive"
        );
        assert!(
            !remaining_cmd.contains("cpm archive"),
            "cpm archive must be gone"
        );
    }

    #[test]
    fn uninstall_preserves_other_hooks() {
        let fs = MemoryFileSystem::new();
        let settings = serde_json::json!({
            "hooks": {
                "SessionEnd": [
                    {
                        "matcher": "",
                        "hooks": [{"type": "command", "command": "cpm archive --hook-stdin --archive-dir \"/archive\""}]
                    },
                    {
                        "matcher": "",
                        "hooks": [{"type": "command", "command": "my-other-tool"}]
                    }
                ]
            }
        });
        fs.write(
            Path::new("/h/.claude/settings.json"),
            serde_json::to_vec_pretty(&settings).unwrap().as_slice(),
        )
        .unwrap();
        uninstall_session_end_hook(&fs, Path::new("/h")).unwrap();
        let v: serde_json::Value =
            serde_json::from_slice(&fs.read(Path::new("/h/.claude/settings.json")).unwrap())
                .unwrap();
        let ses_end = v["hooks"]["SessionEnd"].as_array().unwrap();
        assert_eq!(ses_end.len(), 1);
        let remaining_cmd = ses_end[0]["hooks"][0]["command"].as_str().unwrap();
        assert!(remaining_cmd.contains("my-other-tool"));
        assert!(!remaining_cmd.contains("cpm archive"));
    }
}
