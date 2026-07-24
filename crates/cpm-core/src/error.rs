#[derive(Debug)]
pub enum CpmError {
    DestinationExists(String),
    WorktreeSource(String),
    Ambiguous(String),
    Locked(String),
    /// Cross-volume move attempted. Deferred to v1.x (spec AC-2). Nothing written.
    CrossVolume(String),
    UnrecognizedFormat(String),
    VerifyFailed(String),
    Io(std::io::Error),
}
impl From<std::io::Error> for CpmError {
    fn from(e: std::io::Error) -> Self {
        CpmError::Io(e)
    }
}

impl std::fmt::Display for CpmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CpmError::DestinationExists(s) => write!(f, "destination exists: {s}"),
            CpmError::WorktreeSource(s) => write!(
                f,
                "worktree source refused: {s} (its .git is a file; pass --force to override)"
            ),
            CpmError::Ambiguous(s) | CpmError::Locked(s) | CpmError::CrossVolume(s) => {
                write!(f, "{s}")
            }
            CpmError::UnrecognizedFormat(s) => write!(f, "unrecognized store format: {s}"),
            CpmError::VerifyFailed(s) => write!(f, "verification failed: {s}"),
            CpmError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CpmError {}
pub type Result<T> = std::result::Result<T, CpmError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_crossvolume_echoes_message_verbatim() {
        let e = CpmError::CrossVolume("cross-volume move refused: X".into());
        let s = format!("{e}");
        assert_eq!(s, "cross-volume move refused: X");
        assert!(
            !s.contains("CrossVolume("),
            "variant name must not appear in Display output"
        );
    }

    #[test]
    fn display_worktreesource_contains_path_and_hint() {
        let e = CpmError::WorktreeSource("E:\\p".into());
        let s = format!("{e}");
        assert!(s.contains("worktree source refused: E:\\p"), "got: {s}");
        assert!(s.contains(".git is a file"), "got: {s}");
    }

    #[test]
    fn display_verifyfailed_has_prefix() {
        let e = CpmError::VerifyFailed("3 checks failed".into());
        assert_eq!(format!("{e}"), "verification failed: 3 checks failed");
    }
}
