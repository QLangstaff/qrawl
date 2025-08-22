use std::fmt;

pub type Result<T> = std::result::Result<T, QrawlError>;

#[derive(Debug)]
pub enum QrawlError {
    InvalidUrl(String),
    MissingDomain,
    Other(String),
}

/* Display + Error for nicer to_string() */
impl fmt::Display for QrawlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QrawlError::InvalidUrl(u) => write!(f, "invalid url: {u}"),
            QrawlError::MissingDomain => write!(f, "missing domain in URL"),
            QrawlError::Other(s) => write!(f, "{s}"),
        }
    }
}
impl std::error::Error for QrawlError {}

/* Conversions so `?` works smoothly */
impl From<std::io::Error> for QrawlError {
    fn from(e: std::io::Error) -> Self {
        QrawlError::Other(e.to_string())
    }
}
impl From<serde_json::Error> for QrawlError {
    fn from(e: serde_json::Error) -> Self {
        QrawlError::Other(e.to_string())
    }
}
impl From<reqwest::Error> for QrawlError {
    fn from(e: reqwest::Error) -> Self {
        QrawlError::Other(e.to_string())
    }
}
