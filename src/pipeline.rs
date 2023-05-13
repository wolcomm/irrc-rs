use std::collections::VecDeque;
use std::convert::TryFrom;
use std::fmt;
use std::marker::PhantomData;
use std::str::{from_utf8, FromStr};

use circular::Buffer;

use crate::{
    client::Connection,
    error::{self, Error},
    parse,
    query::Query,
};

/// A sequence of queries to be executed sequentially using pipelining.
///
/// See [`Connection::pipeline()`] for details.
pub struct Pipeline<'a> {
    conn: &'a mut Connection,
    buf: Buffer,
    queue: VecDeque<Query>,
}

impl<'a> Pipeline<'a> {
    pub(crate) fn new(conn: &'a mut Connection, capacity: usize) -> Self {
        let buf = Buffer::with_capacity(capacity);
        let queue = VecDeque::new();
        Self { conn, buf, queue }
    }

    pub(crate) fn from_initial<'b, T, F, I>(
        conn: &'a mut Connection,
        initial: Query,
        f: F,
    ) -> Result<Self, Error>
    where
        'a: 'b,
        T: FromStr + fmt::Debug,
        T::Err: std::error::Error + Send + Sync + 'static,
        F: FnMut(Result<ResponseItem<T>, Error>) -> Option<I>,
        I: IntoIterator<Item = Query>,
    {
        let mut pipeline = conn.pipeline();
        let raw_self: *mut Self = pipeline.push(initial)?;
        pipeline
            .pop()
            .unwrap_or_else(|| Err(Error::Dequeue))?
            .filter_map(f)
            .flatten()
            .for_each(move |query| {
                #[allow(unsafe_code)]
                // SAFETY:
                // This is safe here, as nothing is concurrently popping `self.queue`
                // or writing to `self.conn`.
                let result = unsafe { (*raw_self).push(query) };
                // TODO: this error should be fatal
                if let Err(err) = result {
                    log::warn!("error enqueing query: {}", err);
                }
            });
        Ok(pipeline)
    }

    /// Add a query to be executed in order using this [`Pipeline`].
    ///
    /// This method will block until the query is written to the underlying
    /// TCP socket.
    ///
    /// # Errors
    ///
    /// An [`Error`] is returned if the query cannot be written to the
    /// underlying TCP socket.
    ///
    /// # Example
    ///
    /// ``` no_run
    /// # use irrc::{IrrClient, Error};
    /// # fn main() -> Result<(), Error> {
    /// # let mut conn = IrrClient::new("whois.radb.net:43").connect()?;
    /// use irrc::Query;
    ///
    /// let mut pipeline = conn.pipeline();
    /// let query = Query::Ipv6Routes("AS65000".parse().unwrap());
    /// pipeline.push(query)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn push(&mut self, query: Query) -> Result<&mut Self, Error> {
        self.conn.send(&query.cmd())?;
        self.queue.push_back(query);
        Ok(self)
    }

    /// Get the next query response from this [`Pipeline`].
    ///
    /// This method will block until enough data has been read from the
    /// underlying TCP socket to determine the response status and length.
    ///
    /// The [`Response<T>`] contained in the returned result provides methods
    /// for reading any data returned by the server, where `T` is a type
    /// implementing [`FromStr`]. The data elements contained in the response
    /// will be parsed into `T`s during iteration over [`Response<T>`].
    ///
    /// The compiler may need to be told which `T` to parse into in some cases,
    /// as in the following example.
    ///
    /// # Example
    ///
    /// ``` no_run
    /// # use irrc::{IrrClient, Query, Error};
    /// # fn main() -> Result<(), Error> {
    /// # let mut conn = IrrClient::new("whois.radb.net:43").connect()?;
    /// let mut pipeline = conn.pipeline();
    /// pipeline.push(Query::Version)?;
    ///
    /// assert!(pipeline.pop::<String>().is_some());
    /// assert!(pipeline.pop::<String>().is_none());
    /// # Ok(())
    /// # }
    /// ```
    pub fn pop<'b, T>(&'b mut self) -> Option<Result<Response<'a, 'b, T>, Error>>
    where
        T: FromStr + fmt::Debug,
        T::Err: std::error::Error + Send + Sync + 'static,
    {
        self.pop_wrapped()
            .map(|wrapped| wrapped.map_err(error::Wrapper::take_inner))
    }

    fn pop_wrapped<'b, T>(
        &'b mut self,
    ) -> Option<Result<Response<'a, 'b, T>, error::Wrapper<'a, 'b>>>
    where
        'a: 'b,
        T: FromStr + fmt::Debug,
        T::Err: std::error::Error + Send + Sync + 'static,
    {
        self.queue.pop_front().map(move |query| {
            let expect = loop {
                match parse::response_status(self.buf.data()) {
                    Ok((_, (consumed, response_result))) => {
                        _ = self.buf.consume(consumed);
                        match response_result {
                            Ok(Some(len)) => break len,
                            Ok(None) => break 0,
                            Err(err) => return Err(error::Wrapper::new(self, err.into())),
                        }
                    }
                    Err(nom::Err::Incomplete(_)) => {
                        if let Err(err) = self.fetch() {
                            return Err(error::Wrapper::new(self, err));
                        };
                    }
                    Err(err) => {
                        log::error!("query {:?} failed: {}", query, err);
                        let inner_err = err.into();
                        return Err(error::Wrapper::new(self, inner_err));
                    }
                }
            };
            if query.expect_data() {
                if expect == 0 {
                    log::warn!("zero length response for query {:?}", &self);
                }
                log::debug!("expecting response length {} bytes", expect);
                Ok(Response::new(query, self, expect))
            } else if expect == 0 {
                Ok(Response::new(query, self, expect))
            } else {
                // TODO
                panic!("unexpected data")
            }
        })
    }

    /// Get an iterator over the [`ResponseItem`]s returned by the server for
    /// each outstanding query issued, in order.
    ///
    /// A single type `T: FromStr` will be used to parse every data element
    /// from every query.
    ///
    /// If the compiler cannot determine the appropriate type, the turbo-fish
    /// (`::<T>`) syntax may be necessary.
    ///
    /// Error responses received from the server will be logged at the
    /// `WARNING` level and then skipped.
    ///
    /// If some other error handling is required, use
    /// [`pop()`][Self::pop] instead.
    ///
    /// # Example
    ///
    /// ``` no_run
    /// # use irrc::{IrrClient, Query, Error};
    /// # fn main() -> Result<(), Error> {
    /// let autnum = "AS65000".parse().unwrap();
    /// IrrClient::new("whois.radb.net:43")
    ///     .connect()?
    ///     .pipeline()
    ///     .push(Query::Ipv4Routes(autnum))?
    ///     .push(Query::Ipv6Routes(autnum))?
    ///     .responses::<String>()
    ///     .filter_map(Result::ok)
    ///     .for_each(|route| println!("{:?}", route));
    /// # Ok(())
    /// # }
    /// ```
    pub fn responses<'b, T>(&'b mut self) -> Responses<'a, 'b, T>
    where
        'a: 'b,
        T: FromStr + fmt::Debug,
        T::Err: std::error::Error + Send + Sync + 'static,
    {
        Responses {
            pipeline: Some(self),
            current_reponse: None,
        }
    }

    fn fetch(&mut self) -> Result<usize, Error> {
        self.buf.shift();
        let space = self.buf.space();
        log::trace!("trying to fetch up to {} bytes", space.len());
        let fetched = self.conn.read(space)?;
        log::trace!("fetched {} bytes", fetched);
        let filled = self.buf.fill(fetched);
        log::trace!("{:?}", self);
        Ok(filled)
    }

    /// Clear an existing [`Pipeline`] by consuming and discarding
    /// any unread responses from the server.
    ///
    /// Because query responses are transmitted by the server serially, the
    /// client relies on the known ordering of queries in order to match
    /// query to response. Therefore any un consumed responses must be read
    /// and dropped before the [`Pipeline`] can be reused for a new sequence
    /// of queries.
    ///
    /// # [`Drop`]
    ///
    /// The [`Drop`] implementation for [`Pipeline`] will perform the necessary
    /// cleanup of the receive buffer, so that the underlying [`Connection`] can
    /// be re-used.
    ///
    /// Calling [`clear()`][Pipeline::clear] is only necessary if the
    /// [`Pipeline`] (rather than the underlying [`Connection`]) will be
    /// re-used.
    ///
    /// # Example
    ///
    /// ``` no_run
    /// # use irrc::{IrrClient, Query, Error};
    /// # fn main() -> Result<(), Error> {
    /// let mut irr = IrrClient::new("whois.radb.net:43")
    ///     .connect()?;
    /// let mut pipeline = irr.pipeline();
    /// if let Some(autnum) = pipeline
    ///     .push(Query::Origins("192.0.2.0/24".to_string()))?
    ///     .responses::<String>()
    ///     .filter_map(Result::ok)
    ///     .next()
    /// {
    ///     println!("only care the first origin: {}", autnum.content());
    /// }
    /// pipeline.clear();
    /// // do more work with `pipeline`...
    /// # Ok(())
    /// # }
    /// ```
    pub fn clear(&mut self) -> &mut Self {
        self.responses::<String>().consume();
        self
    }
}

impl Drop for Pipeline<'_> {
    fn drop(&mut self) {
        _ = self.clear();
    }
}

impl Extend<Query> for Pipeline<'_> {
    fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = Query>,
    {
        iter.into_iter().for_each(|q| {
            if let Err(err) = self.push(q) {
                log::error!("error enqueuing query: {}", err);
            }
        });
    }
}

impl fmt::Debug for Pipeline<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let max_length = 100;
        let (buf_data, truncated) = if self.buf.available_data() <= max_length {
            (self.buf.data(), "")
        } else {
            (&self.buf.data()[..max_length], " ...")
        };
        let buf_decoded = String::from_utf8_lossy(buf_data);
        f.debug_struct("Pipeline")
            .field("conn", &self.conn)
            .field(
                "buf",
                &format_args!("'{}{}'", buf_decoded.escape_debug(), truncated),
            )
            .field("queue", &self.queue)
            .finish()
    }
}

/// Iterator returned by [`responses()`][Pipeline::responses] method.
///
/// See [`Pipeline::responses`] for details.
#[derive(Debug)]
pub struct Responses<'a, 'b, T>
where
    T: FromStr + fmt::Debug,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    pipeline: Option<&'b mut Pipeline<'a>>,
    current_reponse: Option<Response<'a, 'b, T>>,
}

impl<T> Responses<'_, '_, T>
where
    T: FromStr + fmt::Debug,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    fn consume(&mut self) {
        for item in self {
            log::debug!("consuming unused response item {:?}", item);
        }
    }
}

impl<'a, 'b, T> Iterator for Responses<'a, 'b, T>
where
    'a: 'b,
    T: FromStr + fmt::Debug,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    type Item = Result<ResponseItem<T>, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(ref mut current) = self.current_reponse {
                match current.next_or_yield() {
                    Some(ItemOrYield::Item(item)) => return Some(item),
                    Some(ItemOrYield::Yield(pipeline)) => {
                        self.pipeline = Some(pipeline);
                        self.current_reponse = None;
                    }
                    None => unreachable!(),
                }
            }
            if let Some(pipeline) = self.pipeline.take() {
                if let Some(next_response) = pipeline.pop_wrapped() {
                    match next_response {
                        Ok(response) => {
                            self.current_reponse = Some(response);
                        }
                        Err(err) => {
                            log::warn!("query failed: {}", err.inner());
                            self.pipeline = Some(err.take_pipeline());
                        }
                    }
                }
            } else {
                return None;
            }
        }
    }
}

/// A successful query response.
///
/// If the query returned data, this can be accessed by iteration over
/// [`Response<T>`].
///
/// Constructed by [`Pipeline::pop()`]. See the method documentation for
/// details.
#[derive(Debug)]
pub struct Response<'a, 'b, T>
where
    T: FromStr + fmt::Debug,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    query: Query,
    pipeline: Option<&'b mut Pipeline<'a>>,
    expect: usize,
    seen: usize,
    content_type: PhantomData<T>,
}

impl<'a, 'b, T> Response<'a, 'b, T>
where
    'a: 'b,
    T: FromStr + fmt::Debug,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    pub(crate) fn new(query: Query, pipeline: &'b mut Pipeline<'a>, expect: usize) -> Self {
        Self {
            query,
            pipeline: Some(pipeline),
            expect,
            seen: 0,
            content_type: PhantomData,
        }
    }

    /// The [`Query`] which this was a response to.
    #[must_use]
    pub const fn query(&self) -> &Query {
        &self.query
    }

    fn next_or_yield(&mut self) -> Option<ItemOrYield<'a, 'b, T>> {
        if let Some(pipeline) = self.pipeline.take() {
            if self.query.expect_data() {
                if self.expect == 0 {
                    Some(ItemOrYield::Yield(pipeline))
                } else {
                    loop {
                        if let Ok((_, consumed)) = parse::end_of_response(pipeline.buf.data()) {
                            _ = pipeline.buf.consume(consumed);
                            // TODO: this should be a real error
                            if self.expect != self.seen + 1 {
                                log::error!(
                                    "premature end of response after {} bytes: expected {} bytes",
                                    self.seen,
                                    self.expect
                                );
                            }
                            break Some(ItemOrYield::Yield(pipeline));
                        }
                        match self.query.parse_item(pipeline.buf.data()) {
                            // TODO: check for overrun of response length
                            Ok((consumed, item)) => {
                                let item_result = Ok(ResponseItem(item, self.query.clone()));
                                _ = pipeline.buf.consume(consumed);
                                self.seen += consumed;
                                self.pipeline = Some(pipeline);
                                break Some(ItemOrYield::Item(item_result));
                            }
                            Err(Error::Incomplete | Error::ParseErr) => {
                                if let Err(err) = pipeline.fetch() {
                                    break Some(ItemOrYield::Item(Err(err)));
                                }
                            }
                            Err(err @ Error::ParseItem(_, _)) => {
                                log::error!("error parsing content from response item: {}", err);
                                if let Error::ParseItem(_, consumed) = err {
                                    _ = pipeline.buf.consume(consumed);
                                    self.seen += consumed;
                                }
                                self.pipeline = Some(pipeline);
                                break Some(ItemOrYield::Item(Err(err)));
                            }
                            Err(err) => {
                                log::error!("error parsing word from buffer: {}", err);
                                break Some(ItemOrYield::Item(Err(err)));
                            }
                        }
                    }
                }
            } else {
                Some(ItemOrYield::Yield(pipeline))
            }
        } else {
            None
        }
    }

    fn consume(&mut self) {
        for item in self {
            log::debug!("consuming unused response item {:?}", item);
        }
    }
}

impl<T> Drop for Response<'_, '_, T>
where
    T: FromStr + fmt::Debug,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    fn drop(&mut self) {
        self.consume();
    }
}

impl<T> Iterator for Response<'_, '_, T>
where
    T: FromStr + fmt::Debug,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    type Item = Result<ResponseItem<T>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_or_yield() {
            Some(ItemOrYield::Item(item)) => Some(item),
            _ => None,
        }
    }
}

enum ItemOrYield<'a, 'b, T>
where
    T: FromStr + fmt::Debug,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    Item(Result<ResponseItem<T>, Error>),
    Yield(&'b mut Pipeline<'a>),
}

/// An individual data element contained within the query response.
///
/// The nature of each element is dependent on the corresponding [`Query`]
/// variant.
#[derive(Debug)]
pub struct ResponseItem<T>(ResponseContent<T>, Query)
where
    T: FromStr + fmt::Debug,
    T::Err: std::error::Error + Send + Sync + 'static;

impl<T> ResponseItem<T>
where
    T: FromStr + fmt::Debug,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    /// Borrow the content of [`ResponseItem`].
    pub const fn content(&self) -> &T {
        self.0.content()
    }

    /// Take ownership of the content of [`ResponseItem`].
    pub fn into_content(self) -> T {
        self.0.into_content()
    }

    /// The [`Query`] which this element was provided in response to.
    pub const fn query(&self) -> &Query {
        &self.1
    }
}

#[derive(Debug)]
pub(crate) struct ResponseContent<T>(T)
where
    T: FromStr + fmt::Debug,
    T::Err: std::error::Error + Send + Sync + 'static;

impl<T> ResponseContent<T>
where
    T: FromStr + fmt::Debug,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    const fn content(&self) -> &T {
        &self.0
    }

    #[allow(clippy::missing_const_for_fn)]
    fn into_content(self) -> T {
        self.0
    }
}

impl<T> TryFrom<&[u8]> for ResponseContent<T>
where
    T: FromStr + fmt::Debug,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn try_from(buf: &[u8]) -> Result<Self, Self::Error> {
        Ok(Self(from_utf8(buf)?.parse()?))
    }
}
