#![allow(single_use_lifetimes)]

use std::io;
use std::num::ParseIntError;

use crate::{pipeline::Pipeline, query::Query};

/// Error responses returned by [IRRd].
///
/// [IRRd]: https://irrd.readthedocs.io/en/stable/users/queries/#responses
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Response {
    /// the query was valid, but the primary key queried for did not exist.
    #[error("the query was valid, but the primary key queried for did not exist")]
    KeyNotFound,
    /// the query was valid, but there are multiple copies of the key in one
    /// database.
    #[error("the query was valid, but there are multiple copies of the key in one database")]
    KeyNotUnique,
    /// The query was invalid.
    #[error("the query was invalid: {0}")]
    Other(String),
}

#[derive(Debug, thiserror::Error)]
#[error("{inner}")]
pub(crate) struct Wrapper<'a, 'b> {
    pipeline: Option<&'b mut Pipeline<'a>>,
    #[source]
    inner: Error,
}

impl<'a, 'b> Wrapper<'a, 'b> {
    pub(crate) fn new(pipeline: Option<&'b mut Pipeline<'a>>, inner: Error) -> Self {
        Self { pipeline, inner }
    }

    pub(crate) fn split(self) -> (Option<&'b mut Pipeline<'a>>, Error) {
        (self.pipeline, self.inner)
    }

    #[allow(clippy::missing_const_for_fn)]
    pub(crate) fn take_inner(self) -> Error {
        self.inner
    }
}

/// Error variants returned during query execution.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The server returned an error response.
    #[error("error response for query {0:?}: {1}")]
    ResponseErr(Query, #[source] Response),
    /// IO errors on the underlying transport.
    #[error("an I/O error occurred: {0}")]
    Io(#[from] io::Error),
    /// Failure parsing the "expected length" of a response.
    #[error("failed to decode response length: {0}")]
    BadLength(#[from] ParseIntError),
    /// The parse buffer did not contain enough data.
    #[error("insufficient bytes in parse buffer")]
    Incomplete,
    /// A recoverable error during parsing.
    #[error("failed to parse response")]
    ParseErr,
    /// An unrecoverable error during parsing.
    #[error("fatal parsing erroring while trying to parse response")]
    ParseFailure,
    /// An error occurred while parsing a response item.
    #[error("failed to parse item from response data: {0}")]
    ParseItem(#[source] Box<dyn std::error::Error + Send + Sync>, usize),
    /// Failed to de-queue a query response.
    #[error("failed to dequeue a query response from the pipeline")]
    Dequeue,
    /// The server indicated that data was returned for a query where none
    /// was expected.
    #[error("unexpected non-zero data length received for query {0:?}")]
    UnexpectedData(Query, usize),
    /// Attempted to extract further [`ResponseItem`]s from an already consumed [`Response`].
    #[error("attempted to extract items after EOR was reached")]
    ConsumedResponse,
    /// End of response marker was received before the expected data length had been reached.
    #[error("premature end of response after {0} bytes: expected {1} bytes")]
    ResponseDataUnderrun(usize, usize),
    /// Received all expected data without reaching end of response marker.
    #[error("response data has over run the length indicated in the response preamble")]
    ResponseDataOverrun(usize, usize),
    /// Received a zero-length response for a [`Query`] that should always return data.
    #[error("unexpectedly empty response received for query {0:?}")]
    EmptyResponse(Query),
}

impl From<Wrapper<'_, '_>> for Error {
    fn from(err: Wrapper<'_, '_>) -> Self {
        err.inner
    }
}

impl From<nom::Err<nom::error::Error<&[u8]>>> for Error {
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
