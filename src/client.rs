use std::fmt;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::str::FromStr;
use std::time::Duration;

use crate::{
    error::Error,
    pipeline::{Pipeline, ResponseItem},
    query::Query,
};

/// Builder for IRR query protocol connections.
///
/// This is the entrypoint for most query operations.
///
/// # Example
///
/// ``` no_run
/// use irrc::{IrrClient, Error};
///
/// fn main() -> Result<(), Error> {
///     let mut irr = IrrClient::new("whois.radb.net:43")
///         .connect()?;
///     println!("{}", irr.version()?);
///     Ok(())
/// }
/// ```
///
/// [IRRd]: https://irrd.readthedocs.io/en/stable/
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct IrrClient<A> {
    addr: A,
    client_id: Option<String>,
    server_timeout: Option<Duration>,
}

impl<A> IrrClient<A>
where
    A: ToSocketAddrs + fmt::Display,
{
    /// Default client identification string sent to the server at connection
    /// startup.
    pub const DEFAULT_CLIENT_ID: &'static str =
        concat!(env!("CARGO_PKG_NAME"), "-", env!("CARGO_PKG_VERSION"));

    /// Initialize a new [`IrrClient`].
    ///
    /// The connection is established by calling [`connect()`][Self::connect()]
    /// on the returned object.
    ///
    /// # Example
    ///
    /// ``` no_run
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use irrc::IrrClient;
    ///
    /// if let Ok(conn) = IrrClient::new("whois.radb.net:43").connect() {
    ///     println!("connected!");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub const fn new(addr: A) -> Self {
        Self {
            addr,
            client_id: None,
            server_timeout: None,
        }
    }

    /// Set a client identification string to send to the server upon
    /// connection.
    ///
    /// Default if not set is [`DEFAULT_CLIENT_ID`][Self::DEFAULT_CLIENT_ID].
    pub fn client_id<S: AsRef<str>>(&mut self, id: Option<S>) {
        self.client_id = id.map(|id| id.as_ref().to_string());
    }

    /// Set a non-default server-side timeout.
    ///
    /// The default if not set is server configuration dependent.
    pub fn server_timeout(&mut self, duration: Option<Duration>) {
        self.server_timeout = duration;
    }

    /// Initiate a new connection to an IRRd server.
    ///
    /// # Errors
    ///
    /// Returns an error if the TCP connection to the IRRd server cannot be established.
    pub fn connect(&self) -> Result<Connection, Error> {
        Connection::connect(self)
    }

    fn effective_client_id(&self) -> &str {
        self.client_id
            .as_ref()
            .map_or(Self::DEFAULT_CLIENT_ID, String::as_ref)
    }
}

/// A connection to an [IRRd] server.
///
/// Constructed by [`connect()`][IrrClient::connect()]. See the method
/// documentation for details.
///
/// [IRRd]: https://irrd.readthedocs.io/en/stable/
#[derive(Debug)]
pub struct Connection {
    conn: TcpStream,
}

impl Connection {
    /// Default read buffer size allocated for new [`Pipeline`]s.
    pub const DEFAULT_CAPACITY: usize = 1 << 20;

    #[tracing::instrument(skip(builder), fields(%builder.addr), level = "debug")]
    fn connect<A>(builder: &IrrClient<A>) -> Result<Self, Error>
    where
        A: ToSocketAddrs + fmt::Display,
    {
        tracing::info!("trying to connect to {}", builder.addr);
        let mut conn = TcpStream::connect(&builder.addr)?;
        tracing::debug!("disabling Nagle's algorithm");
        conn.set_nodelay(true)?;
        tracing::debug!("requesting multiple command mode");
        conn.write_all(b"!!\n")?;
        conn.flush()?;
        tracing::info!("connected to {}", builder.addr);
        let mut this = Self { conn };
        {
            let mut init_pipeline = this.pipeline_with_capacity(8);
            _ = init_pipeline.push(Query::SetClientId(builder.effective_client_id().to_owned()))?;
            if let Some(server_timeout) = builder.server_timeout {
                _ = init_pipeline.push(Query::SetTimeout(server_timeout))?;
            }
        }
        Ok(this)
    }

    /// Create a new query [`Pipeline`] using this connection.
    ///
    /// Only a single [`Pipeline`] can exist for a given [`Connection`] at any
    /// one time, to ensure that responses are handled in the correct order.
    ///
    /// The returned [`Pipeline`] is created with a read buffer of
    /// [`DEFAULT_CAPACITY`][Self::DEFAULT_CAPACITY] bytes. The
    /// [`pipeline_with_capacity()`][Self::pipeline_with_capacity()] method
    /// can be used to specify an alternate size.
    pub fn pipeline(&mut self) -> Pipeline<'_> {
        self.pipeline_with_capacity(Self::DEFAULT_CAPACITY)
    }

    /// Create a new pipeline, passing an initial [`Query`] and a closure
    /// that creates additional queries from the response data of the first.
    ///
    /// Rust's ownership rules prevent new [`Query`]s being
    /// [`push()`][Pipeline::push]ed whilst the response of another is still
    /// being read from the TCP socket. As as result, using only
    /// [`push()`][Pipeline::push] and [`pop()`][Pipeline::pop], it is not
    /// possible to enqueue follow-up queries until the initial query response
    /// has been fully consumed.
    ///
    /// This method saves substantial end-to-end query latency be enqueing
    /// follow up queries as soon as the [`ResponseItem`] they are constructed
    /// from has been read.
    ///
    /// It also avoids the necessity to `collect()` the initial `ResponseItem`s
    /// into a temporary data structure, saving allocations.
    ///
    /// See `examples/pipelined.rs` for example usage.
    ///
    /// # Errors
    ///
    /// An [`Error`] is returned if a connection error is encountered during
    /// the processing of the `initial` query.
    pub fn pipeline_from_initial<T, F, I>(
        &mut self,
        initial: Query,
        f: F,
    ) -> Result<Pipeline<'_>, Error>
    where
        T: FromStr + fmt::Debug,
        T::Err: std::error::Error + Send + Sync + 'static,
        F: FnMut(Result<ResponseItem<T>, Error>) -> Option<I>,
        I: IntoIterator<Item = Query>,
    {
        Pipeline::from_initial(self, initial, f)
    }

    /// Create a new query [`Pipeline`] from an iterator of [`Query`] items.
    pub fn pipeline_from_iter<I>(&mut self, iter: I) -> Pipeline<'_>
    where
        I: IntoIterator<Item = Query>,
    {
        let mut pipeline = self.pipeline();
        pipeline.extend(iter);
        pipeline
    }

    /// Create a new query [`Pipeline`] with a non-default read buffer size.
    pub fn pipeline_with_capacity(&mut self, capacity: usize) -> Pipeline<'_> {
        Pipeline::new(self, capacity)
    }

    /// Get the server's version identification string.
    ///
    /// # Errors
    ///
    /// An error is returned if a failure occurs on the underlying TCP
    /// connection, the response contains no data, or if the response
    /// bytes cannot be parsed as UTF-8.
    pub fn version(&mut self) -> Result<String, Error> {
        Ok(self
            .pipeline()
            .push(Query::Version)?
            .pop::<String>()
            .unwrap_or_else(|| Err(Error::Dequeue))?
            .next()
            .unwrap_or_else(|| Err(Error::EmptyResponse(Query::Version)))?
            .content()
            .clone())
    }

    #[tracing::instrument(skip(self), level = "debug")]
    pub(crate) fn send(&mut self, query: &str) -> Result<(), Error> {
        tracing::debug!("sending query");
        self.conn.write_all(query.as_bytes())?;
        self.conn.flush().map_err(Error::from)
    }

    pub(crate) fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.conn.read(buf).map_err(Error::from)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        tracing::info!("closing connection");
        if let Err(err) = self.conn.write(b"!q\n") {
            tracing::error!("failed to send quit command: {err}");
        }
        if let Err(err) = self.conn.shutdown(Shutdown::Both) {
            tracing::error!("failed to close connection: {err}");
        }
    }
}
