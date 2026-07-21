use crate::error::{CpmError, Result};
use crate::fs::FileSystem;
use std::path::{Path, PathBuf};

const CPM_HOOK_MARKER: &str = "cpm archive";

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

/// Returns true if a hook group entry is one installed by cpm archive.
fn is_cpm_entry(entry: &serde_json::Value) -> bool {
    entry["hooks"]
        .as_array()
        .map(|hooks| {
            hooks.iter().any(|h| {
                h["command"]
                    .as_str()
                    .map(|c| c.contains(CPM_HOOK_MARKER))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// Install a SessionEnd hook in ~/.claude/settings.json that calls
/// `<exe_path> archive --session "$CLAUDE_SESSION_ID"`.
/// Idempotent: any existing cpm archive entry is removed before the new one is added.
pub fn install_session_end_hook(fs: &dyn FileSystem, home: &Path, exe_path: &Path) -> Result<()> {
    let mut v = load_settings(fs, home);
    let cmd = format!(
        "{} archive --session \"$CLAUDE_SESSION_ID\"",
        exe_path.to_string_lossy()
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

    #[test]
    fn uninstall_preserves_other_hooks() {
        let fs = MemoryFileSystem::new();
        let settings = serde_json::json!({
            "hooks": {
                "SessionEnd": [
                    {
                        "matcher": "",
                        "hooks": [{"type": "command", "command": "cpm archive --session x"}]
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
