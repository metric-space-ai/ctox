#![forbid(unsafe_code)]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("http: {0}")]
    Http(String),
    #[error("ftp: {0}")]
    Ftp(String),
    #[error("sftp: {0}")]
    Sftp(String),
    #[error("bt: {0}")]
    Bt(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("rpc: {0}")]
    Rpc(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        let mut msg = e.to_string();
        let mut src = std::error::Error::source(&e);
        while let Some(s) = src {
            msg.push_str(": ");
            msg.push_str(&s.to_string());
            src = s.source();
        }
        Error::Http(msg)
    }
}
