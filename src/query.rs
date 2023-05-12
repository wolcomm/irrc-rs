use std::convert::TryInto;
use std::error::Error;
use std::fmt;
use std::iter::{once, Once};
use std::str::FromStr;
use std::time::Duration;

use rpsl::names::{AsSet, AutNum, Mntner, RouteSet};

use crate::{error::QueryError, parse, pipeline::ResponseContent};

/// Alias for [`Result<T, error::QueryError>`].
#[allow(clippy::module_name_repetitions)]
pub type QueryResult<T> = Result<T, QueryError>;

/// IRRd query variants.
// TODO: !a, !j, maybe !J
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Query {
    /// Returns the current version of the server.
    Version,
    /// Identifies the client to the server.
    ///
    /// This should usually be used via
    /// [`client_id()`][crate::IrrClient::client_id], rather than being issued
    /// directly.
    SetClientId(String),
    /// Sets the server-side timeout of the connection.
    ///
    /// This should usually be used via
    /// [`server_timeout()`][crate::IrrClient::server_timeout],
    /// rather than being issued directly.
    SetTimeout(Duration),
    /// Returns the list of sources currently selected for query resolution.
    GetSources,
    /// Sets the list of sources to be used for subsequent query resolution.
    SetSources(Vec<String>),
    /// Re-sets the list of sources to all those available on the server.
    UnsetSources,
    /// Returns all (direct) members of an `as-set`.
    AsSetMembers(AsSet),
    /// Returns all members of an `as-set`, recursively expanding `as-set`
    /// members as necessary.
    AsSetMembersRecursive(AsSet),
    /// Returns all (direct) members of a `route-set`.
    RouteSetMembers(RouteSet),
    /// Returns all members of an `route-set`, recursively expanding members
    /// as necessary.
    RouteSetMembersRecursive(RouteSet),
    /// Returns all IPv4 prefixes corresponding to a `route` object having
    /// `origin:` set to the provided AS.
    Ipv4Routes(AutNum),
    /// Returns all IPv6 prefixes corresponding to a `route6` object having
    /// `origin:` set to the provided AS.
    Ipv6Routes(AutNum),
    /// Returns an RPSL object exactly matching the provided key, of the
    /// specified RPSL object class.
    RpslObject(RpslObjectClass, String),
    /// Returns all RPSL objects with the specified maintainer in their
    /// `mnt-by:` attribute.
    MntBy(Mntner),
    /// Returns the unique `origin:`s of `route` or `route6` objects exactly
    /// matching the provided prefix.
    Origins(String),
    /// Returns all RPSL `route` or `route6` objects exactly matching the
    /// provided prefix.
    RoutesExact(String),
    /// Returns all RPSL `route` or `route6` objects one level less-specific
    /// (excluding exeact matches) than the provided prefix.
    RoutesLess(String),
    /// Returns all RPSL `route` or `route6` objects one level less-specific
    /// (including exeact matches) than the provided prefix.
    RoutesLessEqual(String),
    /// Returns all RPSL `route` or `route6` objects one level more-specific
    /// (excluding exeact matches) than the provided prefix.
    RoutesMore(String),
}

impl Query {
    pub(crate) fn cmd(&self) -> String {
        match self {
            Self::Version => "!v\n".to_owned(),
            Self::SetClientId(id) => format!("!n{id}\n"),
            Self::SetTimeout(dur) => format!("!t{}\n", dur.as_secs()),
            Self::GetSources => "!s-lc\n".to_owned(),
            Self::SetSources(sources) => format!("!s{}\n", sources.join(",")),
            Self::UnsetSources => "!s-*\n".to_owned(),
            Self::AsSetMembers(q) => format!("!i{q}\n"),
            Self::AsSetMembersRecursive(q) => format!("!i{q},1\n"),
            Self::RouteSetMembers(q) => format!("!i{q}\n"),
            Self::RouteSetMembersRecursive(q) => format!("!i{q},1\n"),
            Self::Ipv4Routes(q) => format!("!g{q}\n"),
            Self::Ipv6Routes(q) => format!("!6{q}\n"),
            Self::RpslObject(class, q) => format!("!m{class},{q}\n"),
            Self::MntBy(q) => format!("!o{q}\n"),
            Self::Origins(q) => format!("!r{q},o\n"),
            Self::RoutesExact(q) => format!("!r{q}\n"),
            Self::RoutesLess(q) => format!("!r{q},l\n"),
            Self::RoutesLessEqual(q) => format!("!r{q},L\n"),
            Self::RoutesMore(q) => format!("!r{q},M\n"),
        }
    }

    pub(crate) const fn expect_data(&self) -> bool {
        matches!(
            self,
            Self::Version
                | Self::GetSources
                | Self::AsSetMembers(_)
                | Self::RouteSetMembers(_)
                | Self::AsSetMembersRecursive(_)
                | Self::RouteSetMembersRecursive(_)
                | Self::Ipv4Routes(_)
                | Self::Ipv6Routes(_)
                | Self::RpslObject(..)
                | Self::MntBy(_)
                | Self::Origins(_)
                | Self::RoutesExact(_)
                | Self::RoutesLess(_)
                | Self::RoutesLessEqual(_)
                | Self::RoutesMore(_)
        )
    }

    pub(crate) fn parse_item<T>(&self, input: &[u8]) -> QueryResult<(usize, ResponseContent<T>)>
    where
        T: FromStr + fmt::Debug,
        T::Err: Error + Send + Sync + 'static,
    {
        let (_, (consumed, item)) = match self {
            _ if !self.expect_data() => parse::noop(input)?,
            Self::Version => parse::all(input)?,
            Self::RpslObject(..)
            | Self::MntBy(_)
            | Self::RoutesExact(_)
            | Self::RoutesLess(_)
            | Self::RoutesLessEqual(_)
            | Self::RoutesMore(_) => parse::paragraph(input)?,
            _ => parse::word(input)?,
        };
        let content = item
            .try_into()
            .map_err(|err: QueryError| err.into_sized(consumed))?;
        Ok((consumed, content))
    }
}

impl IntoIterator for Query {
    type Item = Self;
    type IntoIter = Once<Self>;
    fn into_iter(self) -> Self::IntoIter {
        once(self)
    }
}

/// RPSL object classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::Display)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum RpslObjectClass {
    /// `mntner` object class.
    #[strum(to_string = "mntner")]
    Mntner,
    /// `person` object class.
    #[strum(to_string = "person")]
    Person,
    /// `role` object class.
    #[strum(to_string = "role")]
    Role,
    /// `route` object class.
    #[strum(to_string = "route")]
    Route,
    /// `route6` object class.
    #[strum(to_string = "route6")]
    Route6,
    /// `aut-num` object class.
    #[strum(to_string = "aut-num")]
    AutNum,
    /// `inet-rtr` object class.
    #[strum(to_string = "inet-rtr")]
    InetRtr,
    /// `as-set` object class.
    #[strum(to_string = "as-set")]
    AsSet,
    /// `route-set` object class.
    #[strum(to_string = "route-set")]
    RouteSet,
    /// `filter-set` object class.
    #[strum(to_string = "filter-set")]
    FilterSet,
    /// `rtr-set` object class.
    #[strum(to_string = "rtr-set")]
    RtrSet,
    /// `peering-set` object class.
    #[strum(to_string = "peering-set")]
    PeeringSet,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_is_singleton_iterator() {
        let q = Query::Version;
        let mut iter = q.clone().into_iter();
        assert_eq!(iter.next(), Some(q));
        assert_eq!(iter.next(), None);
    }

    mod proptests {
        use proptest::{prelude::*, strategy::Union};
        use strum::IntoEnumIterator;

        use super::*;

        impl Arbitrary for RpslObjectClass {
            type Parameters = ();
            type Strategy = Union<Just<Self>>;
            fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
                Union::new(Self::iter().map(Just))
            }
        }

        impl Arbitrary for Query {
            type Parameters = ();
            type Strategy = BoxedStrategy<Self>;

            fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
                prop_oneof![
                    Just(Self::Version),
                    any::<String>().prop_map(Self::SetClientId),
                    any::<Duration>().prop_map(Self::SetTimeout),
                    Just(Self::GetSources),
                    any::<Vec<String>>().prop_map(Self::SetSources),
                    Just(Self::UnsetSources),
                    any::<AsSet>().prop_map(Self::AsSetMembers),
                    any::<AsSet>().prop_map(Self::AsSetMembersRecursive),
                    any::<RouteSet>().prop_map(Self::RouteSetMembers),
                    any::<RouteSet>().prop_map(Self::RouteSetMembersRecursive),
                    any::<AutNum>().prop_map(Self::Ipv4Routes),
                    any::<AutNum>().prop_map(Self::Ipv6Routes),
                    any::<(RpslObjectClass, String)>()
                        .prop_map(|(class, object)| Self::RpslObject(class, object)),
                    any::<Mntner>().prop_map(Self::MntBy),
                    any::<String>().prop_map(Self::Origins),
                    any::<String>().prop_map(Self::RoutesExact),
                    any::<String>().prop_map(Self::RoutesLess),
                    any::<String>().prop_map(Self::RoutesLessEqual),
                    any::<String>().prop_map(Self::RoutesMore),
                ]
                .boxed()
            }
        }

        proptest! {
            #[test]
            fn cmd_begins_with_bang(q in any::<Query>()) {
                assert!(q.cmd().starts_with('!'));
            }

            #[test]
            fn cmd_ends_with_newline(q in any::<Query>()) {
                assert!(q.cmd().ends_with('\n'));
            }

            #[test]
            #[allow(unused_must_use)]
            fn parse_item_never_panics(q in any::<Query>(), input in any::<Vec<u8>>()) {
                q.parse_item::<String>(&input);
            }
        }
    }
}
