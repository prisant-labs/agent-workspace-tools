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

/// True when two absolute paths live on the same volume (Windows drive letter
/// or leading mount segment).
pub fn same_volume(a: &str, b: &str) -> bool {
    fn root(p: &str) -> String {
        let n = normalize_path(p);
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
}
