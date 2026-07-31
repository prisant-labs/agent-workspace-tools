use crate::error::{AwtError, Result};
use crate::fs::FileSystem;
use std::path::{Path, PathBuf};

const AWT_HOOK_MARKER: &str = "archive --hook-stdin";
/// Legacy marker used by the earlier broken hook that passed a session id via env var.
const AWT_HOOK_LEGACY_MARKER: &str = "archive --session";

fn settings_path(home: &Path) -> PathBuf {
    home.join(".claude").join("settings.json")
}

/// Load settings.json, failing CLOSED on anything except genuine absence (AC-56).
///
/// Only io::ErrorKind::NotFound may initialize a fresh, empty settings object: a machine that
/// has never run Claude Code is a valid empty state. Every other outcome - a read failure, a
/// parse failure, invalid UTF-8, a root that is not an object - propagates as an error,
/// because the caller's next step is to WRITE this value back over the user's file. The first
/// shipped version returned an empty object on any failure, which meant a transient read
/// error or one malformed byte silently erased the user's entire settings on the next
/// settings-touching command.
fn load_settings(fs: &dyn FileSystem, home: &Path) -> Result<serde_json::Value> {
    let path = settings_path(home);
    let bytes = match fs.read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(serde_json::Value::Object(serde_json::Map::new()));
        }
        Err(e) => return Err(AwtError::Io(e)),
    };
    let v: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
        AwtError::UnrecognizedFormat(format!(
            "{}: {e}. Refusing to modify settings that do not parse - a write here would \
             replace your settings with a nearly-empty file. Fix or remove the file first",
            path.display()
        ))
    })?;
    if !v.is_object() {
        return Err(AwtError::UnrecognizedFormat(format!(
            "{}: root is not a JSON object. Refusing to modify settings of an unrecognized \
             shape",
            path.display()
        )));
    }
    Ok(v)
}

/// Write settings atomically: serialize to a sibling temp file, then rename over the final
/// path, so a crash mid-write can never leave a half-written settings.json.
fn save_settings(fs: &dyn FileSystem, home: &Path, v: &serde_json::Value) -> Result<()> {
    let path = settings_path(home);
    let tmp = path.with_file_name("settings.json.awt-tmp");
    fs.write(&tmp, serde_json::to_vec_pretty(v).unwrap().as_slice())?;
    fs.rename(&tmp, &path)?;
    Ok(())
}

/// Set cleanupPeriodDays in ~/.claude/settings.json.
/// Refuses days=0 without the force_zero opt-in because issue #23710 documents
/// that 0 triggers a regression in Claude Code's cleanup scheduler.
pub fn set_retention(fs: &dyn FileSystem, home: &Path, days: u32, force_zero: bool) -> Result<()> {
    if days == 0 && !force_zero {
        return Err(AwtError::Locked(
            "cleanupPeriodDays=0 triggers issue #23710; pass --force-zero to override".into(),
        ));
    }
    let mut v = load_settings(fs, home)?;
    v["cleanupPeriodDays"] = serde_json::json!(days);
    save_settings(fs, home, &v)?;
    Ok(())
}

/// Returns true if a hook group entry is one installed by awt archive (current or legacy).
fn is_awt_entry(entry: &serde_json::Value) -> bool {
    entry["hooks"]
        .as_array()
        .map(|hooks| {
            hooks.iter().any(|h| {
                h["command"]
                    .as_str()
                    .map(|c| c.contains(AWT_HOOK_MARKER) || c.contains(AWT_HOOK_LEGACY_MARKER))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// Install a SessionEnd hook in ~/.claude/settings.json.
/// The installed command reads hook context from stdin as JSON (Claude Code's confirmed
/// contract) rather than env vars, and archives the transcript to archive_dir.
/// Idempotent: any existing awt archive entry is removed before the new one is added.
pub fn install_session_end_hook(
    fs: &dyn FileSystem,
    home: &Path,
    exe_path: &Path,
    archive_dir: &Path,
) -> Result<()> {
    let mut v = load_settings(fs, home)?;
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
        arr.retain(|e| !is_awt_entry(e));
        arr.push(new_entry);
    } else {
        v["hooks"]["SessionEnd"] = serde_json::json!([new_entry]);
    }
    save_settings(fs, home, &v)?;
    Ok(())
}

/// Remove any awt archive SessionEnd hook entries from ~/.claude/settings.json.
/// Leaves all other SessionEnd entries untouched.
pub fn uninstall_session_end_hook(fs: &dyn FileSystem, home: &Path) -> Result<()> {
    let mut v = load_settings(fs, home)?;
    if let Some(arr) = v
        .get_mut("hooks")
        .and_then(|h| h.get_mut("SessionEnd"))
        .and_then(|se| se.as_array_mut())
    {
        arr.retain(|e| !is_awt_entry(e));
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
            Path::new("/usr/bin/awt"),
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

    /// A settings.json with BOTH a legacy awt entry (archive --session) and an unrelated
    /// user entry: after uninstall the legacy awt entry is gone and the user entry survives.
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
                            "command": "awt archive --session \"$CLAUDE_SESSION_ID\" --archive-dir \"/archive\""
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
        assert_eq!(ses_end.len(), 1, "legacy awt entry must be removed");
        let remaining_cmd = ses_end[0]["hooks"][0]["command"].as_str().unwrap();
        assert!(
            remaining_cmd.contains("my-other-tool"),
            "user entry must survive"
        );
        assert!(
            !remaining_cmd.contains("awt archive"),
            "awt archive must be gone"
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
                        "hooks": [{"type": "command", "command": "awt archive --hook-stdin --archive-dir \"/archive\""}]
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
        assert!(!remaining_cmd.contains("awt archive"));
    }
}
