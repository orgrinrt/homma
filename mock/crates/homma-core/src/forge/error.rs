//! `ForgeError`: error type returned by every [`super::Forge`] method.

use std::fmt;

/// Errors surfaced by [`super::Forge`] impls.
///
/// Concrete clients (`ForgejoClient`, `GitHubClient`, future ones) wrap their
/// transport / HTTP / deserialization errors into the [`ForgeError::Backend`]
/// variant. The remaining variants name failure modes shared across every
/// hosting service.
#[derive(Debug)]
pub enum ForgeError {
    /// The named repository does not exist on the forge.
    RepoNotFound { owner: String, name: String },
    /// The forge returned an authentication failure. The token configured via
    /// `ForgeConfig::token_env` is missing, expired, or lacks scope.
    Unauthorized { reason: String },
    /// The forge rejected the operation because a repo with the same name
    /// already exists in the target namespace.
    RepoAlreadyExists { owner: String, name: String },
    /// The forge returned an unexpected HTTP status. Carries the status code
    /// and a short body excerpt.
    UnexpectedStatus { status: u16, body: String },
    /// Backend-specific transport or parse failure. Concrete clients box their
    /// internal errors into this variant.
    ///
    /// Network failures (DNS, TCP) and parse failures (JSON deserialise) both
    /// land here for now, so callers cannot distinguish "retry-safe transient"
    /// from "permanent shape mismatch" without inspecting the inner error.
    /// A future split into `NetworkError` + `RateLimited { retry_after }` is
    /// tracked for the migrate command's retry path.
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl fmt::Display for ForgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RepoNotFound { owner, name } => {
                write!(f, "repo not found: {owner}/{name}")
            }
            Self::Unauthorized { reason } => {
                write!(f, "forge rejected credentials: {reason}")
            }
            Self::RepoAlreadyExists { owner, name } => {
                write!(f, "repo already exists: {owner}/{name}")
            }
            Self::UnexpectedStatus { status, body } => {
                write!(f, "forge returned HTTP {status}: {body}")
            }
            Self::Backend(e) => write!(f, "forge backend error: {e}"),
        }
    }
}

impl std::error::Error for ForgeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Backend(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}
