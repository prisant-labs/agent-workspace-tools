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
pub type Result<T> = std::result::Result<T, CpmError>;
