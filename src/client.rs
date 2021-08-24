use std::error::Error;
use std::fmt;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::str::FromStr;
use std::time::Duration;

use crate::{
    pipeline::{Pipeline, ResponseItem},
    query::{Query, QueryResult},
    types::{AsSet, AutNum},
};

/// Builder for IRR query protocol connections.
///
/// This is the entrypoint for most query operations.
///
/// # Example
///
/// ``` no_run
/// use irrc::{IrrClient, QueryResult};
///
/// fn main() -> QueryResult<()> {
///     let mut irr = IrrClient::new("whois.radb.net:43")
///         .connect()?;
///     println!("{}", irr.version()?);
///     Ok(())
/// }
/// ```
///
/// [IRRd]: https://irrd.readthedocs.io/en/stable/
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
    pub fn new(addr: A) -> Self {
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
        self.client_id = id.map(|id| id.as_ref().to_string())
    }

    /// Set a non-default server-side timeout.
    ///
    /// The default if not set is server configuration dependent.
    pub fn server_timeout(&mut self, duration: Option<Duration>) {
        self.server_timeout = duration
    }

    /// Initiate a new connection to an IRRd server.
    pub fn connect(&self) -> io::Result<Connection> {
        Connection::connect(self)
    }

    fn effective_client_id(&self) -> &str {
        self.client_id
            .as_ref()
            .map(|id| id.as_ref())
            .unwrap_or(Self::DEFAULT_CLIENT_ID)
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

    fn connect<A>(builder: &IrrClient<A>) -> io::Result<Self>
    where
        A: ToSocketAddrs + fmt::Display,
    {
        log::info!("trying to connect to {}", builder.addr);
        let mut conn = TcpStream::connect(&builder.addr)?;
        log::debug!("disabling Nagle's algorithm");
        conn.set_nodelay(true)?;
        log::debug!("requesting multiple command mode");
        conn.write_all(b"!!\n")?;
        conn.flush()?;
        log::info!("connected to {}", builder.addr);
        let mut this = Self { conn };
        {
            let mut init_pipeline = this.pipeline_with_capacity(8);
            init_pipeline.push(Query::SetClientId(builder.effective_client_id().to_owned()))?;
            if let Some(server_timeout) = builder.server_timeout {
                init_pipeline.push(Query::SetTimeout(server_timeout))?;
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
    pub fn pipeline(&mut self) -> Pipeline {
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
    pub fn pipeline_from_initial<T, F, I>(&mut self, initial: Query, f: F) -> QueryResult<Pipeline>
    where
        T: FromStr + fmt::Debug,
        T::Err: Error + Send + Sync + 'static,
        F: Fn(QueryResult<ResponseItem<T>>) -> Option<I>,
        I: IntoIterator<Item = Query>,
    {
        Pipeline::from_initial(self, initial, f)
    }

    /// Create a new query [`Pipeline`] with a non-default read buffer size.
    pub fn pipeline_with_capacity(&mut self, capacity: usize) -> Pipeline {
        Pipeline::new(self, capacity)
    }

    /// Get the server's version identification string.
    pub fn version(&mut self) -> QueryResult<String> {
        Ok(self
            .pipeline()
            .push(Query::Version)?
            .pop::<String>()
            .unwrap()?
            .next()
            .unwrap()?
            .content()
            .to_owned())
    }

    /// Convenience function to execute a [`Query::AsSetMembers`] query in a
    /// new [`Pipeline`].
    pub fn as_set_members(&mut self, as_set: AsSet) -> QueryResult<Vec<ResponseItem<String>>> {
        self.pipeline()
            .push(Query::AsSetMembers(as_set))?
            .pop()
            .unwrap()?
            .collect()
    }

    /// Convenience function to execute a [`Query::Ipv4Routes`] query in a
    /// new [`Pipeline`].
    pub fn ipv4_routes(&mut self, autnum: AutNum) -> QueryResult<Vec<ResponseItem<String>>> {
        self.pipeline()
            .push(Query::Ipv4Routes(autnum))?
            .pop()
            .unwrap()?
            .collect()
    }

    /// Convenience function to execute a [`Query::Ipv6Routes`] query in a
    /// new [`Pipeline`].
    pub fn ipv6_routes(&mut self, autnum: AutNum) -> QueryResult<Vec<ResponseItem<String>>> {
        self.pipeline()
            .push(Query::Ipv6Routes(autnum))?
            .pop()
            .unwrap()?
            .collect()
    }

    pub(crate) fn send(&mut self, query: &str) -> io::Result<()> {
        log::debug!("sending query {:?}", query);
        self.conn.write_all(query.as_bytes())?;
        self.conn.flush()
    }

    pub(crate) fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.conn.read(buf)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        log::info!("closing connection");
        if let Err(err) = self.conn.write(b"!q\n") {
            log::warn!("failed to send quit command: {}", err);
        }
        if let Err(err) = self.conn.shutdown(Shutdown::Both) {
            log::warn!("failed to close connection: {}", err);
        }
    }
}
