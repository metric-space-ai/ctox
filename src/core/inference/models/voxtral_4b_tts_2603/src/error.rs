use core::fmt;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    InvalidFormat(&'static str),
    Utf8(std::str::Utf8Error),
    MissingTensor(String),
    Unsupported(&'static str),
    Shape(&'static str),
    OutOfBounds(&'static str),
    Parse(String),
}

pub type Result<T> = core::result::Result<T, Error>;

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(e: std::str::Utf8Error) -> Self {
        Error::Utf8(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::InvalidFormat(s) => write!(f, "invalid format: {s}"),
            Error::Utf8(e) => write!(f, "utf8 error: {e}"),
            Error::MissingTensor(s) => write!(f, "missing tensor: {s}"),
            Error::Unsupported(s) => write!(f, "unsupported: {s}"),
            Error::Shape(s) => write!(f, "shape error: {s}"),
            Error::OutOfBounds(s) => write!(f, "out of bounds: {s}"),
            Error::Parse(s) => write!(f, "parse error: {s}"),
        }
    }
}

impl std::error::Error for Error {}
