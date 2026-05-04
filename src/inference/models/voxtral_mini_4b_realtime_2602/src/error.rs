use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Utf8(std::str::Utf8Error),
    InvalidFormat(&'static str),
    Parse(String),
    MissingTensor(String),
    Unsupported(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Utf8(err) => write!(f, "{err}"),
            Self::InvalidFormat(msg) => write!(f, "{msg}"),
            Self::Parse(msg) => write!(f, "{msg}"),
            Self::MissingTensor(name) => write!(f, "missing tensor `{name}`"),
            Self::Unsupported(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(value: std::str::Utf8Error) -> Self {
        Self::Utf8(value)
    }
}
