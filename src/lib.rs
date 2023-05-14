//! This crate provides a client implementation of the [IRRd query protocol][irrd].
//!
//! The implementation provides pipelined query execution for maximal performance over a single TCP
//! connection.
//!
//! # Quickstart
//!
//! ``` no_run
//! use irrc::{IrrClient, Query, Error};
//! use rpsl::names::AutNum;
//!
//! fn main() -> Result<(), Error> {
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
#![doc(html_root_url = "https://docs.rs/irrc/0.1.0-rc.5")]
// clippy lints
#![warn(clippy::pedantic)]
#![warn(clippy::cargo)]
#![warn(clippy::nursery)]
#![allow(clippy::redundant_pub_crate)]
#![allow(clippy::multiple_crate_versions)]
// rustc lints
#![allow(box_pointers)]
#![warn(absolute_paths_not_starting_with_crate)]
#![warn(deprecated_in_future)]
#![warn(elided_lifetimes_in_paths)]
#![warn(explicit_outlives_requirements)]
#![warn(keyword_idents)]
#![warn(macro_use_extern_crate)]
#![warn(meta_variable_misuse)]
#![warn(missing_abi)]
#![warn(missing_copy_implementations)]
#![warn(missing_debug_implementations)]
#![warn(missing_docs)]
#![warn(non_ascii_idents)]
#![warn(noop_method_call)]
#![warn(pointer_structural_match)]
#![warn(rust_2021_incompatible_closure_captures)]
#![warn(rust_2021_incompatible_or_patterns)]
#![warn(rust_2021_prefixes_incompatible_syntax)]
#![warn(rust_2021_prelude_collisions)]
#![warn(single_use_lifetimes)]
#![warn(trivial_casts)]
#![warn(trivial_numeric_casts)]
#![warn(unreachable_pub)]
#![warn(unsafe_code)]
#![warn(unsafe_op_in_unsafe_fn)]
#![warn(unstable_features)]
#![warn(unused_crate_dependencies)]
#![warn(unused_extern_crates)]
#![warn(unused_import_braces)]
#![warn(unused_lifetimes)]
#![warn(unused_qualifications)]
#![warn(unused_results)]
#![warn(variant_size_differences)]
// docs.rs build config
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

// silence unused dev-dependency warnings
#[cfg(test)]
mod deps {
    use ip as _;
    use simple_logger as _;
    use version_sync as _;
}

mod client;
pub use self::client::{Connection, IrrClient};

mod parse;

mod pipeline;
pub use self::pipeline::{Pipeline, Response, ResponseItem, Responses};

mod query;
pub use self::query::{Query, RpslObjectClass};

/// Error types returned during query execution
pub mod error;
pub use self::error::Error;
