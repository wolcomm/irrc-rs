use std::collections::VecDeque;
use std::io;

use circular::Buffer;

use crate::{
    client::Connection,
    error::{QueryError, WrappingQueryError},
    parse,
    query::{Query, QueryResult},
};

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

    pub fn push(&mut self, query: Query) -> io::Result<&mut Self> {
        self.conn.send(&query.cmd())?;
        self.queue.push_back(query);
        Ok(self)
    }

    unsafe fn push_raw(pipeline: *mut Pipeline, query: Query) -> io::Result<()> {
        (*pipeline).push(query)?;
        Ok(())
    }

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

    pub fn clear(&mut self) -> &mut Connection {
        self.responses().consume();
        self.conn
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

#[derive(Debug)]
pub struct Responses<'a, 'b>
where
    'a: 'b,
{
    pipeline: Option<&'b mut Pipeline<'a>>,
    current_reponse: Option<Response<'a, 'b>>,
}

impl Responses<'_, '_> {
    pub fn consume(&mut self) {
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

    pub fn consume(&mut self) {
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

#[derive(Debug)]
pub struct ResponseItem(String, Query);

impl ResponseItem {
    pub fn content(&self) -> &str {
        &self.0
    }

    pub fn query(&self) -> &Query {
        &self.1
    }
}
