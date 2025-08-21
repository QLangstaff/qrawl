use thiserror::Error;

#[derive(Debug, Error)]
pub enum QrawlError {
    #[error("invalid url: {0}")] InvalidUrl(String),
    #[error("missing domain from url")] MissingDomain,
    #[error("io: {0}")] Io(#[from] std::io::Error),
    #[error("serde: {0}")] Serde(#[from] serde_json::Error),
    #[error("{0}")] Other(String),
}
pub type Result<T> = std::result::Result<T, QrawlError>;
