#[derive(Debug)]
pub enum AwtError {
    DestinationExists(String),
    WorktreeSource(String),
    Ambiguous(String),
    Locked(String),
    /// Cross-volume move attempted. Deferred to v1.x (spec AC-2). Nothing written.
    CrossVolume(String),
    /// A folder move was requested but the source is not a directory on disk (AC-55).
    /// Without this guard, apply silently skipped the move and reported success while
    /// Claude state was rewritten toward a destination no folder occupies.
    SourceMissing(String),
    UnrecognizedFormat(String),
    VerifyFailed(String),
    Io(std::io::Error),
}
impl From<std::io::Error> for AwtError {
    fn from(e: std::io::Error) -> Self {
        AwtError::Io(e)
    }
}

impl std::fmt::Display for AwtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AwtError::DestinationExists(s) => write!(f, "destination exists: {s}"),
            AwtError::WorktreeSource(s) => write!(
                f,
                "worktree source refused: {s} (its .git is a file; pass --force to override)"
            ),
            AwtError::Ambiguous(s) | AwtError::Locked(s) | AwtError::CrossVolume(s) => {
                write!(f, "{s}")
            }
            AwtError::SourceMissing(s) => write!(
                f,
                "source folder not found: {s}. A move needs an existing source directory; \
                 check the path, or use 'awt associate' to re-home history whose folder is gone"
            ),
            AwtError::UnrecognizedFormat(s) => write!(f, "unrecognized store format: {s}"),
            AwtError::VerifyFailed(s) => write!(f, "verification failed: {s}"),
            AwtError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AwtError {}
pub type Result<T> = std::result::Result<T, AwtError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_crossvolume_echoes_message_verbatim() {
        let e = AwtError::CrossVolume("cross-volume move refused: X".into());
        let s = format!("{e}");
        assert_eq!(s, "cross-volume move refused: X");
        assert!(
            !s.contains("CrossVolume("),
            "variant name must not appear in Display output"
        );
    }

    #[test]
    fn display_worktreesource_contains_path_and_hint() {
        let e = AwtError::WorktreeSource("E:\\p".into());
        let s = format!("{e}");
        assert!(s.contains("worktree source refused: E:\\p"), "got: {s}");
        assert!(s.contains(".git is a file"), "got: {s}");
    }

    #[test]
    fn display_verifyfailed_has_prefix() {
        let e = AwtError::VerifyFailed("3 checks failed".into());
        assert_eq!(format!("{e}"), "verification failed: 3 checks failed");
    }
}
