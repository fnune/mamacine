//! Errors carry what was attempted and refused.

use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// The request never reached the service.
    Unreachable {
        what: String,
        detail: String,
    },
    /// The service answered, and said no.
    Refused {
        what: String,
        status: u16,
        message: String,
    },
    /// Answered with something unreadable.
    Unreadable {
        what: String,
        detail: String,
    },
    /// Something the person must fix, phrased plainly.
    Setup(String),
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unreachable { what, detail } => {
                write!(formatter, "cannot reach {what}: {detail}")
            }
            Error::Refused {
                what,
                status,
                message,
            } => write!(
                formatter,
                "{what} refused the request ({status}): {message}"
            ),
            Error::Unreadable { what, detail } => {
                write!(formatter, "{what} sent something unreadable: {detail}")
            }
            Error::Setup(message) => write!(formatter, "{message}"),
            Error::Io(inner) => write!(formatter, "{inner}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(inner: std::io::Error) -> Self {
        Error::Io(inner)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
