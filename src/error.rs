use std::{
    error::Error as StdError,
    ffi::OsString,
    fmt,
    io,
    process::ExitStatus,
    string::FromUtf8Error,
};

use glob::{GlobError, PatternError};

/// Result type used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors surfaced by Qshr operations.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Command {
        program: OsString,
        status: ExitStatus,
        stderr: String,
    },
    Utf8(FromUtf8Error),
    GlobPattern(PatternError),
    Glob(GlobError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(err) => write!(f, "I/O error: {err}"),
            Error::Command {
                program,
                status,
                stderr,
            } => {
                write!(
                    f,
                    "command {:?} exited with {status} (stderr: {stderr})",
                    program
                )
            }
            Error::Utf8(err) => write!(f, "UTF-8 conversion failed: {err}"),
            Error::GlobPattern(err) => write!(f, "invalid glob pattern: {err}"),
            Error::Glob(err) => write!(f, "glob resolution failed: {err}"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Io(err) => Some(err),
            Error::Utf8(err) => Some(err),
            Error::GlobPattern(err) => Some(err),
            Error::Glob(err) => Some(err),
            Error::Command { .. } => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Error::Io(value)
    }
}

impl From<FromUtf8Error> for Error {
    fn from(value: FromUtf8Error) -> Self {
        Error::Utf8(value)
    }
}

impl From<PatternError> for Error {
    fn from(value: PatternError) -> Self {
        Error::GlobPattern(value)
    }
}

impl From<GlobError> for Error {
    fn from(value: GlobError) -> Self {
        Error::Glob(value)
    }
}
