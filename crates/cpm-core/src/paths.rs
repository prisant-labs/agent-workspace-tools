/// Map an absolute path to Claude Code's `projects/` directory name.
/// Every character that is not ASCII alphanumeric becomes '-'. Forward-only:
/// this is lossy (a-b, a.b, a\b, a_b all collapse) so it is NEVER used to look
/// up an existing directory - use the reverse index for that.
pub fn encode_project_dir(abs: &str) -> String {
    abs.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Case- and separator-insensitive key for comparing two absolute paths.
pub fn normalize_path(abs: &str) -> String {
    abs.replace('\\', "/").to_lowercase()
}

/// True when two absolute paths live on the same volume.
///
/// Windows drive-letter paths compare by drive (`E:`), and UNC paths compare by
/// server plus share (`\\server\share`), which is the unit a rename cannot cross.
///
/// POSIX mount points are NOT handled: every absolute POSIX path reports the same
/// volume, so a move across mounts would be treated as a rename. That gap is part
/// of the macOS/Linux bring-up tracked in DESIGN.md ("Platform scope") and must
/// close before POSIX support ships.
pub fn same_volume(a: &str, b: &str) -> bool {
    fn root(p: &str) -> String {
        let n = normalize_path(p);

        // Strip the Win32 verbatim prefix first. It is a path-parsing escape, not
        // part of the volume identity, and `std::fs::canonicalize` emits it on
        // Windows - so this form reaches us whether or not a caller expects it.
        // `\\?\C:\x` is the drive `c:`; `\\?\UNC\server\share` is that UNC volume.
        let n = match n.strip_prefix("//?/") {
            Some(rest) => match rest.strip_prefix("unc/") {
                Some(unc) => format!("//{unc}"),
                None => rest.to_string(),
            },
            None => n,
        };

        // A leading "//" marks a UNC path. `split` emits an empty field for the
        // run before the first separator, so without this branch every UNC path
        // would report a root of "" and compare equal to every other UNC path.
        if let Some(rest) = n.strip_prefix("//") {
            let mut seg = rest.split('/').filter(|s| !s.is_empty());
            let server = seg.next().unwrap_or("");
            let share = seg.next().unwrap_or("");
            return format!("//{server}/{share}");
        }

        n.split('/').next().unwrap_or("").to_string()
    }
    root(a) == root(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_like_claude_real_dirs() {
        assert_eq!(
            encode_project_dir("E:\\Projects\\prisant-labs\\vs-code-markdown-max"),
            "E--Projects-prisant-labs-vs-code-markdown-max"
        );
        // dot collapses (verified real dir): .claude -> -claude, v2.26.0 -> v2-26-0
        assert_eq!(
            encode_project_dir("E:\\Projects\\pm-skills\\docs\\internal\\release-plans\\v2.26.0"),
            "E--Projects-pm-skills-docs-internal-release-plans-v2-26-0"
        );
        assert_eq!(
            encode_project_dir("E:\\Projects\\Chrome - Bookmark Autosort"),
            "E--Projects-Chrome---Bookmark-Autosort"
        );
    }

    #[test]
    fn normalize_is_case_and_slash_insensitive() {
        assert_eq!(normalize_path("E:\\Projects\\A"), "e:/projects/a");
        assert_eq!(normalize_path("e:/Projects/A"), "e:/projects/a");
    }

    #[test]
    fn same_volume_compares_drive_root() {
        assert!(same_volume("E:\\a", "E:\\b\\c"));
        assert!(same_volume("E:\\a", "e:/b"));
        assert!(!same_volume("E:\\a", "F:\\a"));
    }

    #[test]
    fn same_volume_distinguishes_unc_servers_and_shares() {
        // A UNC volume is identified by \\server\share, not by the empty segment
        // that precedes the leading separator. Two different servers are two
        // different volumes, and a rename between them fails at the OS level.
        assert!(!same_volume(r"\\server1\share\a", r"\\server2\share\b"));
        assert!(!same_volume(r"\\server1\alpha\a", r"\\server1\beta\b"));
        assert!(same_volume(r"\\server1\share\a", r"\\SERVER1\SHARE\b"));
        // A UNC path and a local drive are never the same volume.
        assert!(!same_volume(r"\\server1\share\a", "E:\\a"));
    }

    #[test]
    fn same_volume_sees_through_verbatim_prefixes() {
        // std::fs::canonicalize returns \\?\-prefixed paths on Windows, so this
        // form reaches us in practice. The verbatim prefix is a Win32 path-parsing
        // escape, not part of the volume identity: \\?\C:\x is the same volume as
        // C:\x, and \\?\UNC\server\share is the same volume as \\server\share.
        assert!(same_volume(r"\\?\C:\a", r"C:\b"));
        assert!(!same_volume(r"\\?\C:\a", r"\\?\D:\a"));
        assert!(same_volume(r"\\?\UNC\server1\share\a", r"\\server1\share\b"));
        assert!(!same_volume(r"\\?\UNC\server1\share\a", r"\\?\UNC\server2\share\b"));
    }
}
