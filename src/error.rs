use std::error::Error;
use std::fmt;
use std::io;
use std::num::ParseIntError;

use crate::pipeline::Pipeline;

/// Error responses returned by [IRRd].
///
/// [IRRd]: https://irrd.readthedocs.io/en/stable/users/queries/#responses
// TODO: these should contain the original query
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, PartialEq, Eq)]
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KeyNotFound => write!(
                f,
                "the query was valid, but the primary key queried for did not exist"
            ),
            Self::KeyNotUnique => write!(
                f,
                "the query was valid, but there are multiple copies of the key in one database"
            ),
            Self::Other(msg) => write!(f, "the query was invalid: {msg}"),
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
    pub(crate) fn new(pipeline: &'b mut Pipeline<'a>, inner: QueryError) -> Self {
        Self { pipeline, inner }
    }

    pub(crate) const fn inner(&self) -> &QueryError {
        &self.inner
    }

    #[allow(clippy::missing_const_for_fn)]
    pub(crate) fn take_inner(self) -> QueryError {
        self.inner
    }

    pub(crate) fn take_pipeline(self) -> &'b mut Pipeline<'a> {
        self.pipeline
    }
}

impl fmt::Display for WrappingQueryError<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner().fmt(f)
    }
}

impl Error for WrappingQueryError<'_, '_> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.inner())
    }
}

/// Error variants returned during query execution.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub enum QueryError {
    /// The server returned an error response.
    ResponseErr(ResponseError),
    /// IO errors on the underlying transport.
    Io(io::Error),
    /// Failure parsing the "expected length" of a response.
    BadLength(ParseIntError),
    /// The parse buffer did not contain enough data.
    Incomplete,
    /// A recoverable error during parsing.
    ParseErr,
    /// An unrecoverable error during parsing.
    ParseFailure,
    /// An error occurred while parsing a response item.
    ItemParse(Box<dyn Error + Send + Sync>),
    /// An error occurred while parsing a response item.
    ///
    /// This variant wraps [`ItemParse`][Self::ItemParse], including the
    /// length of the failed item.
    SizedItemParse(Box<QueryError>, usize),
    /// Failed to de-queue a query response.
    Dequeue,
}

impl QueryError {
    /// Construct a [`ItemParse`][Self::ItemParse] variant from an underlying
    /// error.
    pub(crate) fn from_item_parse_err<E>(err: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self::ItemParse(Box::new(err))
    }

    pub(crate) fn into_sized(self, size: usize) -> Self {
        match self {
            err @ Self::ItemParse(_) => Self::SizedItemParse(Box::new(err), size),
            err => panic!("attempted to construct a `QueryError::SizedItemParse` from {err:?}"),
        }
    }
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResponseErr(err) => write!(f, "error response from server: {err}"),
            Self::Io(err) => write!(f, "an IO error occurred: {err}"),
            Self::BadLength(err) => write!(f, "failed to decode response length: {err}"),
            Self::Incomplete => write!(f, "insufficient bytes in parse buffer"),
            Self::ParseErr | Self::ParseFailure => write!(f, "failed to parse response"),
            Self::ItemParse(err) => write!(f, "failed to parse response item: {err}"),
            Self::SizedItemParse(err, _) => write!(f, "{err}"),
            Self::Dequeue => write!(f, "failed to dequeue a query response from the pipeline"),
        }
    }
}

impl Error for QueryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ResponseErr(err) => Some(err),
            Self::Io(err) => Some(err),
            Self::BadLength(err) => Some(err),
            Self::ItemParse(err) => Some(err.as_ref()),
            Self::SizedItemParse(err, _) => Some(err.as_ref()),
            _ => None,
        }
    }
}

impl From<WrappingQueryError<'_, '_>> for QueryError {
    fn from(err: WrappingQueryError<'_, '_>) -> Self {
        err.inner
    }
}

impl From<nom::Err<nom::error::Error<&[u8]>>> for QueryError {
    fn from(err: nom::Err<nom::error::Error<&[u8]>>) -> Self {
        match err {
            nom::Err::Incomplete(_) => Self::Incomplete,
            nom::Err::Error(_) => Self::ParseErr,
            nom::Err::Failure(_) => {
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

impl From<ParseIntError> for QueryError {
    fn from(err: ParseIntError) -> Self {
        Self::BadLength(err)
    }
}
