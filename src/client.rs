use std::fmt;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::{
    pipeline::{Pipeline, ResponseItem},
    query::{Query, QueryResult},
};

/// Builder for IRR query protocol connections.
///
/// This is the entrypoint for most query operations.
///
/// # Example
///
/// ``` rust
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
///
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

    pub fn new(addr: A) -> Self {
        Self {
            addr,
            client_id: None,
            server_timeout: None,
        }
    }

    pub fn client_id<S: AsRef<str>>(&mut self, id: Option<S>) {
        self.client_id = id.map(|id| id.as_ref().to_string())
    }

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
    ///
    pub fn pipeline(&mut self) -> Pipeline {
        self.pipeline_with_capacity(Self::DEFAULT_CAPACITY)
    }

    pub fn pipeline_from_initial<F, I>(&mut self, initial: Query, f: F) -> QueryResult<Pipeline>
    where
        F: Fn(QueryResult<ResponseItem>) -> Option<I>,
        I: IntoIterator<Item = Query>,
    {
        Pipeline::from_initial(self, initial, f)
    }

    /// Create a new query [`Pipeline`] with a non-default read buffer size.
    pub fn pipeline_with_capacity(&mut self, capacity: usize) -> Pipeline {
        Pipeline::new(self, capacity)
    }

    // FIXME
    /// Get the servers version identification `String`.
    pub fn version(&mut self) -> QueryResult<String> {
        Ok(self
            .pipeline()
            .push(Query::Version)?
            .pop()
            .unwrap()?
            .next()
            .unwrap()?
            .content()
            .to_owned())
    }

    pub fn as_set_members(&mut self, s: &str) -> QueryResult<Vec<ResponseItem>> {
        self.pipeline()
            .push(Query::AsSetMembers(s.to_owned()))?
            .pop()
            .unwrap()?
            .collect()
    }

    pub fn ipv4_routes(&mut self, s: &str) -> QueryResult<Vec<ResponseItem>> {
        self.pipeline()
            .push(Query::Ipv4Routes(s.to_owned()))?
            .pop()
            .unwrap()?
            .collect()
    }

    pub fn ipv6_routes(&mut self, s: &str) -> QueryResult<Vec<ResponseItem>> {
        self.pipeline()
            .push(Query::Ipv6Routes(s.to_owned()))?
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
