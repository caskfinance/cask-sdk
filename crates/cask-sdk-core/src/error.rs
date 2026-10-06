use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum Error {
    #[error("snapshot has not been loaded yet")]
    NotReady,
    #[error("customer not found: {0}")]
    CustomerNotFound(String),
    #[error("feature not found: {0}")]
    FeatureNotFound(String),
    #[error("feature {feature} is {actual}, not {expected}")]
    WrongType {
        feature: String,
        expected: &'static str,
        actual: &'static str,
    },
    #[error("authentication failed (invalid API key)")]
    Auth,
    #[error("network error: {0}")]
    Network(String),
    #[error("invalid snapshot: {0}")]
    InvalidSnapshot(String),
}
