use cpm_core::error::CpmError;

/// The exit-code contract. Phase 4 can only reach 0, 1 and 4; the guard codes exist
/// now so the mapping is settled in one place before the write phases start using it.
///
/// 1 io, 2 refused (a guard fired, nothing written), 3 verify failed, 4 unknown shape.
pub fn code_for(err: &CpmError) -> i32 {
    match err {
        CpmError::DestinationExists(_)
        | CpmError::WorktreeSource(_)
        | CpmError::Ambiguous(_)
        | CpmError::Locked(_) => 2,
        CpmError::VerifyFailed(_) => 3,
        CpmError::UnrecognizedFormat(_) => 4,
        CpmError::Io(_) => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_guard_refusal_shares_exit_2_and_the_other_failures_are_distinct() {
        // The guards are one class to the caller: "refused, nothing written".
        assert_eq!(code_for(&CpmError::DestinationExists("d".into())), 2);
        assert_eq!(code_for(&CpmError::WorktreeSource("w".into())), 2);
        assert_eq!(code_for(&CpmError::Ambiguous("a".into())), 2);
        assert_eq!(code_for(&CpmError::Locked("l".into())), 2);
        // These three each need their own code: a script must be able to tell a bad
        // write (3) from a shape we refused to touch at all (4) from plain io (1).
        assert_eq!(code_for(&CpmError::VerifyFailed("v".into())), 3);
        assert_eq!(code_for(&CpmError::UnrecognizedFormat("u".into())), 4);
        assert_eq!(
            code_for(&CpmError::Io(std::io::Error::other("boom"))),
            1,
            "io is the catch-all, and must never collide with a refusal"
        );
    }
}
