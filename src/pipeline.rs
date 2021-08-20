use std::collections::VecDeque;
use std::io;

use circular::Buffer;

use crate::{
    client::Connection,
    error::{QueryError, WrappingQueryError},
    parse,
    query::{Query, QueryResult},
};

/// A sequence of queries to be executed sequentially using pipelining.
///
/// See [`Connection::pipeline()`] for details.
///
#[derive(Debug)]
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

    pub(crate) fn from_initial<'b, F, I>(
        conn: &'a mut Connection,
        initial: Query,
        f: F,
    ) -> QueryResult<Self>
    where
        'a: 'b,
        F: Fn(QueryResult<ResponseItem>) -> Option<I>,
        I: IntoIterator<Item = Query>,
    {
        let mut pipeline = conn.pipeline();
        pipeline.push(initial)?;
        let raw_self = &mut pipeline as *mut Pipeline;
        pipeline
            .pop()
            // safe to unwrap because there is exactly one query in the queue
            .unwrap()?
            .filter_map(f)
            .flatten()
            .for_each(move |query| {
                let result = unsafe { Self::push_raw(raw_self, query) };
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
    /// # Example
    ///
    /// ``` no_run
    /// # use irrc::{IrrClient, QueryResult};
    /// # fn main() -> QueryResult<()> {
    /// # let mut conn = IrrClient::new("whois.radb.net:43").connect()?;
    /// use irrc::Query;
    ///
    /// let mut pipeline = conn.pipeline();
    /// let query = Query::Ipv6Routes("AS65000".to_string());
    /// pipeline.push(query)?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    pub fn push(&mut self, query: Query) -> io::Result<&mut Self> {
        self.conn.send(&query.cmd())?;
        self.queue.push_back(query);
        Ok(self)
    }

    unsafe fn push_raw(pipeline: *mut Pipeline, query: Query) -> io::Result<()> {
        (*pipeline).push(query)?;
        Ok(())
    }

    /// Get the next query response from this [`Pipeline`].
    ///
    /// This method will block until enough data has been read from the underlying
    /// TCP socket to determine the response status and length.
    ///
    /// The [`Response`] contained in the returned result provides methods for
    /// reading any data returned by the server.
    ///
    /// # Example
    ///
    /// ``` no_run
    /// # use irrc::{IrrClient, Query, QueryResult};
    /// # fn main() -> QueryResult<()> {
    /// # let mut conn = IrrClient::new("whois.radb.net:43").connect()?;
    /// let mut pipeline = conn.pipeline();
    /// pipeline.push(Query::Version)?;
    ///
    /// assert!(pipeline.pop().is_some());
    /// assert!(pipeline.pop().is_none());
    /// # Ok(())
    /// # }
    /// ```
    ///
    pub fn pop<'b>(&'b mut self) -> Option<QueryResult<Response<'a, 'b>>> {
        self.pop_wrapped()
            .map(|wrapped| wrapped.map_err(|err| err.take_inner()))
    }

    fn pop_wrapped<'b>(&'b mut self) -> Option<Result<Response<'a, 'b>, WrappingQueryError<'a, 'b>>>
    where
        'a: 'b,
    {
        self.queue.pop_front().map(move |query| {
            let expect = loop {
                match parse::response_status(self.buf.data()) {
                    Ok((_, (consumed, response_result))) => {
                        self.buf.consume(consumed);
                        match response_result {
                            Ok(Some(len)) => break len,
                            Ok(None) => break 0,
                            Err(err) => return Err(WrappingQueryError::new(self, err.into())),
                        }
                    }
                    Err(nom::Err::Incomplete(_)) => {
                        if let Err(err) = self.fetch() {
                            return Err(WrappingQueryError::new(self, err.into()));
                        };
                    }
                    Err(err) => {
                        log::error!("query {:?} failed: {}", query, err);
                        let inner_err = err.into();
                        return Err(WrappingQueryError::new(self, inner_err));
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
    /// Error responses received from the server will be logged at the `WARNING`
    /// level and then skipped. If some other error handling is required, use
    /// [`pop()`][Self::pop] instead.
    ///
    /// # Example
    ///
    /// ``` no_run
    /// # use irrc::{IrrClient, Query, QueryResult};
    /// # fn main() -> QueryResult<()> {
    /// let autnum = "AS65000".to_string();
    /// IrrClient::new("whois.radb.net:43")
    ///     .connect()?
    ///     .pipeline()
    ///     .push(Query::Ipv4Routes(autnum.clone()))?
    ///     .push(Query::Ipv6Routes(autnum.clone()))?
    ///     .responses()
    ///     .filter_map(Result::ok)
    ///     .for_each(|route| println!("{:?}", route));
    /// # Ok(())
    /// # }
    /// ```
    ///
    pub fn responses<'b>(&'b mut self) -> Responses<'a, 'b>
    where
        'a: 'b,
    {
        Responses {
            pipeline: Some(self),
            current_reponse: None,
        }
    }

    fn fetch(&mut self) -> io::Result<usize> {
        self.buf.shift();
        let space = self.buf.space();
        log::debug!("trying to fetch up to {} bytes", space.len());
        let fetched = self.conn.read(space)?;
        log::debug!("fetched {} bytes", fetched);
        Ok(self.buf.fill(fetched))
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
    /// # use irrc::{IrrClient, Query, QueryResult};
    /// # fn main() -> QueryResult<()> {
    /// let mut irr = IrrClient::new("whois.radb.net:43")
    ///     .connect()?;
    /// let mut pipeline = irr.pipeline();
    /// if let Some(autnum) = pipeline
    ///     .push(Query::Origins("192.0.2.0/24".to_string()))?
    ///     .responses()
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
    ///
    pub fn clear(&mut self) -> &mut Self {
        self.responses().consume();
        self
    }
}

impl Drop for Pipeline<'_> {
    fn drop(&mut self) {
        self.clear();
    }
}

impl Extend<Query> for Pipeline<'_> {
    fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = Query>,
    {
        iter.into_iter().for_each(|q| {
            if let Err(err) = self.push(q) {
                log::error!("error enqueuing query: {}", err)
            }
        })
    }
}

/// Iterator returned by [`responses()`][Pipeline::responses] method.
///
/// See [`Pipeline::responses`] for details.
///
#[derive(Debug)]
pub struct Responses<'a, 'b>
where
    'a: 'b,
{
    pipeline: Option<&'b mut Pipeline<'a>>,
    current_reponse: Option<Response<'a, 'b>>,
}

impl Responses<'_, '_> {
    fn consume(&mut self) {
        for item in self {
            log::debug!("consuming unused response item {:?}", item);
        }
    }
}

impl<'a, 'b> Iterator for Responses<'a, 'b>
where
    'a: 'b,
{
    type Item = QueryResult<ResponseItem>;
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
                            self.pipeline = Some(err.take_pipeline())
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
/// If the query returned data, this can be accessed by iteration over [`Response`].
///
/// Constructed by [`Pipeline::pop()`]. See the method documentation for details.
#[derive(Debug)]
pub struct Response<'a, 'b>
where
    'a: 'b,
{
    query: Query,
    pipeline: Option<&'b mut Pipeline<'a>>,
    expect: usize,
    seen: usize,
}

impl<'a, 'b> Response<'a, 'b>
where
    'a: 'b,
{
    pub(crate) fn new(query: Query, pipeline: &'b mut Pipeline<'a>, expect: usize) -> Self {
        Self {
            query,
            pipeline: Some(pipeline),
            expect,
            seen: 0,
        }
    }

    /// The [`Query`] which this was a response to.
    pub fn query(&self) -> &Query {
        &self.query
    }

    fn next_or_yield(&mut self) -> Option<ItemOrYield<'a, 'b>> {
        if let Some(pipeline) = self.pipeline.take() {
            if self.query.expect_data() {
                if self.expect == 0 {
                    Some(ItemOrYield::Yield(pipeline))
                } else {
                    loop {
                        if let Ok((_, consumed)) = parse::end_of_response(pipeline.buf.data()) {
                            pipeline.buf.consume(consumed);
                            // TODO: this should be a real error
                            if !self.expect == self.seen + 1 {
                                log::error!(
                                    "premature end of response after {} bytes: expected {} bytes",
                                    self.seen,
                                    self.expect
                                );
                            }
                            break Some(ItemOrYield::Yield(pipeline));
                        }
                        match self.query.parse_item(pipeline.buf.data()) {
                            // TODO: check for overrun of respnse length
                            Ok((consumed, item)) => {
                                let item_result = Ok(ResponseItem(item, self.query.clone()));
                                pipeline.buf.consume(consumed);
                                self.seen += consumed;
                                self.pipeline = Some(pipeline);
                                break Some(ItemOrYield::Item(item_result));
                            }
                            Err(QueryError::Incomplete | QueryError::ParseErr) => {
                                if let Err(err) = pipeline.fetch() {
                                    break Some(ItemOrYield::Item(Err(err.into())));
                                }
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

impl Drop for Response<'_, '_> {
    fn drop(&mut self) {
        self.consume();
    }
}

impl Iterator for Response<'_, '_> {
    type Item = QueryResult<ResponseItem>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_or_yield() {
            Some(ItemOrYield::Item(item)) => Some(item),
            _ => None,
        }
    }
}

enum ItemOrYield<'a, 'b> {
    Item(QueryResult<ResponseItem>),
    Yield(&'b mut Pipeline<'a>),
}

/// An individual data element contained within the query response.
///
/// The nature of each element is dependent on the corresponding [`Query`]
/// variant.
///
#[derive(Debug)]
pub struct ResponseItem(String, Query);

impl ResponseItem {
    /// The content of the [`ResponseItem`].
    pub fn content(&self) -> &str {
        &self.0
    }

    /// The [`Query`] which this element was provided in response to.
    pub fn query(&self) -> &Query {
        &self.1
    }
}
