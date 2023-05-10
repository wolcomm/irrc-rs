//! This crate provides an client implementation for the [IRRd query
//! protocol][irrd].
//!
//! The implementation provides pipelined query execution for maximal
//! performance over a single TCP connection.
//!
//! # Quickstart
//!
//! ``` no_run
//! use irrc::{IrrClient, Query, QueryResult};
//! use rpsl::names::AutNum;
//!
//! fn main() -> QueryResult<()> {
//!
//!     let mut irr = IrrClient::new("whois.radb.net:43")
//!         .connect()?;
//!
//!     println!("connected to {}", irr.version()?);
//!
//!     let as_set = "AS-FOO".parse().unwrap();
//!     println!("getting members of {}", as_set);
//!     irr.pipeline()
//!         .push(Query::AsSetMembersRecursive(as_set))?
//!         .responses::<AutNum>()
//!         .filter_map(|result| {
//!             result.map_err(|err| {
//!                 println!("error parsing member: {}", err);
//!                 err
//!             })
//!             .ok()
//!         })
//!         .for_each(|autnum| println!("{}", autnum.content()));
//!
//!     Ok(())
//! }
//! ```
//!
//! [irrd]: https://irrd.readthedocs.io/en/stable/users/queries/#irrd-style-queries
#![doc(html_root_url = "https://docs.rs/irrc/0.1.0-rc.4")]
#![warn(missing_docs)]

mod client;
mod parse;
mod pipeline;
mod query;

/// Error types returned during query execution
pub mod error;

pub use client::{Connection, IrrClient};
pub use pipeline::{Pipeline, Response, ResponseItem, Responses};
pub use query::{Query, QueryResult};
