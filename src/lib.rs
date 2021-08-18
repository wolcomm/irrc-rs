//! This crate provides an client implementation for the [IRRd query protocol][irrd].
//!
//! The implementation provides pipelined query execution for maximal
//! performance over a single TCP connection.
//!
//! [irrd]: https://irrd.readthedocs.io/en/stable/users/queries/#irrd-style-queries
//!
#![doc(html_root_url = "https://docs.rs/irrc/0.1.0-alpha.1")]
// #![warn(missing_docs)]

mod client;
mod parse;
mod pipeline;
mod query;

/// Error types returned during query execution
pub mod error;

pub use client::IrrClient;
pub use pipeline::{Pipeline, Response, ResponseItem, Responses};
pub use query::{Query, QueryResult};
