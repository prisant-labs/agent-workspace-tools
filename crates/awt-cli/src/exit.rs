use awt_core::error::AwtError;

/// The exit-code contract. Phase 4 can only reach 0, 1 and 4; the guard codes exist
/// now so the mapping is settled in one place before the write phases start using it.
///
/// 1 io, 2 refused (a guard fired, nothing written), 3 verify failed, 4 unknown shape.
pub fn code_for(err: &AwtError) -> i32 {
    match err {
        AwtError::DestinationExists(_)
        | AwtError::WorktreeSource(_)
        | AwtError::Ambiguous(_)
        | AwtError::Locked(_)
        | AwtError::CrossVolume(_)
        | AwtError::SourceMissing(_)
        | AwtError::NestedProjects(_) => 2,
        AwtError::VerifyFailed(_) => 3,
        AwtError::UnrecognizedFormat(_) => 4,
        AwtError::Io(_) => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_guard_refusal_shares_exit_2_and_the_other_failures_are_distinct() {
        // The guards are one class to the caller: "refused, nothing written".
        assert_eq!(code_for(&AwtError::DestinationExists("d".into())), 2);
        assert_eq!(code_for(&AwtError::WorktreeSource("w".into())), 2);
        assert_eq!(code_for(&AwtError::Ambiguous("a".into())), 2);
        assert_eq!(code_for(&AwtError::Locked("l".into())), 2);
        assert_eq!(code_for(&AwtError::CrossVolume("c".into())), 2);
        assert_eq!(code_for(&AwtError::SourceMissing("s".into())), 2);
        assert_eq!(code_for(&AwtError::NestedProjects("n".into())), 2);
        // These three each need their own code: a script must be able to tell a bad
        // write (3) from a shape we refused to touch at all (4) from plain io (1).
        assert_eq!(code_for(&AwtError::VerifyFailed("v".into())), 3);
        assert_eq!(code_for(&AwtError::UnrecognizedFormat("u".into())), 4);
        assert_eq!(
            code_for(&AwtError::Io(std::io::Error::other("boom"))),
            1,
            "io is the catch-all, and must never collide with a refusal"
        );
    }
}
