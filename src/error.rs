use std::error::Error;
use std::fmt;
use std::io;
use std::num::ParseIntError;
use std::str::Utf8Error;

use crate::pipeline::Pipeline;

/// Error responses returned by [IRRd].
///
/// [IRRd]: https://irrd.readthedocs.io/en/stable/users/queries/#responses
// TODO: these should contain the original query
#[derive(Debug, PartialEq)]
pub enum ResponseError {
    /// the query was valid, but the primary key queried for did not exist.
    KeyNotFound,
    /// the query was valid, but there are multiple copies of the key in one
    /// database.
    KeyNotUnique,
    /// The query was invalid.
    Other(String),
}

impl fmt::Display for ResponseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::KeyNotFound => write!(
                f,
                "the query was valid, but the primary key queried for did not exist"
            ),
            Self::KeyNotUnique => write!(
                f,
                "the query was valid, but there are multiple copies of the key in one database"
            ),
            Self::Other(msg) => write!(f, "the query was invalid: {}", msg),
        }
    }
}

impl Error for ResponseError {}

#[derive(Debug)]
pub(crate) struct WrappingQueryError<'a, 'b> {
    pipeline: &'b mut Pipeline<'a>,
    inner: QueryError,
}

impl<'a, 'b> WrappingQueryError<'a, 'b> {
    pub fn new(pipeline: &'b mut Pipeline<'a>, inner: QueryError) -> Self {
        Self { pipeline, inner }
    }

    pub fn inner(&self) -> &QueryError {
        &self.inner
    }

    pub fn take_inner(self) -> QueryError {
        self.inner
    }

    pub fn take_pipeline(self) -> &'b mut Pipeline<'a> {
        self.pipeline
    }
}

impl fmt::Display for WrappingQueryError<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.inner().fmt(f)
    }
}

impl Error for WrappingQueryError<'_, '_> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.inner())
    }
}

/// Error variants returned during query execution.
#[derive(Debug)]
pub enum QueryError {
    /// The server returned an error response.
    ResponseErr(ResponseError),
    /// IO errors on the underlying transport.
    Io(io::Error),
    /// UTF-8 decoding failure from the received byte stream.
    Utf8Decode(Utf8Error),
    /// Failure parsing the "expected length" of a response.
    BadLength(ParseIntError),
    /// The parse buffer did not contain enough data.
    Incomplete,
    /// A recoverable error during parsing.
    ParseErr,
    /// An unrecoverable error during parsing.
    ParseFailure,
    /// An error occured while parsing a response item.
    ItemParse(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::ResponseErr(err) => write!(f, "error response from server: {}", err),
            Self::Io(err) => write!(f, "an IO error occurred: {}", err),
            Self::Utf8Decode(err) => write!(f, "failed to decode bytes as UTF-8: {}", err),
            Self::BadLength(err) => write!(f, "failed to decode response length: {}", err),
            Self::Incomplete => write!(f, "insufficient bytes in parse buffer"),
            Self::ParseErr => write!(f, "failed to parse response"),
            Self::ParseFailure => write!(f, "failed to parse response"),
            Self::ItemParse(err) => write!(f, "failed to parse response item: {}", err),
        }
    }
}

impl Error for QueryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ResponseErr(err) => Some(err),
            Self::Io(err) => Some(err),
            Self::Utf8Decode(err) => Some(err),
            Self::BadLength(err) => Some(err),
            Self::ItemParse(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

impl From<WrappingQueryError<'_, '_>> for QueryError {
    fn from(err: WrappingQueryError) -> Self {
        err.inner
    }
}

impl From<nom::Err<nom::error::Error<&[u8]>>> for QueryError {
    fn from(err: nom::Err<nom::error::Error<&[u8]>>) -> Self {
        match err {
            nom::Err::Incomplete(_) => Self::Incomplete,
            nom::Err::Error(_) => Self::ParseErr,
            _ => {
                log::debug!("parse error: {:?}", err);
                Self::ParseFailure
            }
        }
    }
}

impl From<ResponseError> for QueryError {
    fn from(err: ResponseError) -> Self {
        Self::ResponseErr(err)
    }
}

impl From<io::Error> for QueryError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<Utf8Error> for QueryError {
    fn from(err: Utf8Error) -> Self {
        Self::Utf8Decode(err)
    }
}

impl From<ParseIntError> for QueryError {
    fn from(err: ParseIntError) -> Self {
        Self::BadLength(err)
    }
}
