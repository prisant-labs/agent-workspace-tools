//! Regression guard for AC-26: the migration path makes zero network or LLM calls.
//! Enforced structurally by keeping network/HTTP/LLM crates out of the dependency tree.
//! This test fails if such a crate enters the workspace lockfile.

const LOCKFILE: &str = include_str!("../../../Cargo.lock");

/// Crate names that would introduce network or LLM capability. If one legitimately
/// appears for a non-network reason, remove it here with a comment explaining why.
const FORBIDDEN: &[&str] = &[
    "reqwest",
    "hyper",
    "ureq",
    "curl",
    "curl-sys",
    "isahc",
    "surf",
    "attohttpc",
    "awc",
    "tungstenite",
    "tokio-tungstenite",
    "native-tls",
    "openssl",
    "openssl-sys",
    "rustls",
    "async-openai",
    "openai",
    "anthropic",
];

#[test]
fn no_network_or_llm_crates_in_dependency_tree() {
    for name in FORBIDDEN {
        let decl = format!("name = \"{name}\"");
        assert!(
            !LOCKFILE.contains(&decl),
            "AC-26 violation: network/LLM crate `{name}` is in Cargo.lock. The migration \
             path must make zero network or LLM calls. If this crate is needed for a \
             non-network reason, remove it from FORBIDDEN with a justification.",
        );
    }
}
