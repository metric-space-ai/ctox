// ref: stalwart/src/utils/errors.rs:1-35
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StalwartError {
    #[error("SMTP protocol error: {code} - {message}")]
    Smtp { code: u16, message: String },
    #[error("CalDAV scheduling conflict: {message}")]
    CalDavConflict { message: String },
    #[error("Directory resolution error: {message}")]
    Directory { message: String },
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Mail parse error: {0}")]
    MailParse(#[from] mailparse::MailParseError),
    #[error("General error: {0}")]
    General(String),
}

pub type StalwartResult<T> = Result<T, StalwartError>;
