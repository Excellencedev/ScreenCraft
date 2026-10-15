use std::fmt;

/// Everything that can go wrong while encoding, muxing or demuxing.
#[derive(Debug)]
pub enum Error {
    /// The settings can't be encoded (odd size, unsupported codec/container pair ...).
    Config(String),
    /// A codec library reported a failure.
    Codec(String),
    /// A container file is malformed or uses something unsupported.
    Format(String),
    /// Used the pipeline in the wrong order (push after finish ...).
    State(String),
    Io(std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Config(s) => write!(f, "invalid settings: {s}"),
            Error::Codec(s) => write!(f, "codec error: {s}"),
            Error::Format(s) => write!(f, "container error: {s}"),
            Error::State(s) => write!(f, "pipeline state: {s}"),
            Error::Io(e) => write!(f, "i/o error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
