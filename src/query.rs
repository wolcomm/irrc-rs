use std::iter::{once, Once};
use std::str::from_utf8;
use std::time::Duration;

use crate::{error::QueryError, parse};

/// Alias for [`Result<T, error::QueryError>`].
pub type QueryResult<T> = Result<T, QueryError>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Query {
    Version,
    SetClientId(String),
    SetTimeout(Duration),
    GetSources,
    SetSources(Vec<String>),
    UnsetSources,
    AsSetMembers(String),
    AsSetMembersRecursive(String),
    RouteSetMembers(String),
    RouteSetMembersRecursive(String),
    Ipv4Routes(String),
    Ipv6Routes(String),
    RpslObject(RpslObjectClass, String),
    MntBy(String),
    Origins(String),
    RoutesExact(String),
    RoutesLess(String),
    RoutesLessEqual(String),
    RoutesMore(String),
}

impl Query {
    pub(crate) fn cmd(&self) -> String {
        match self {
            Self::Version => "!v\n".to_owned(),
            Self::SetClientId(id) => format!("!n{}\n", id),
            Self::SetTimeout(dur) => format!("!t{}\n", dur.as_secs()),
            Self::GetSources => "!s-lc\n".to_owned(),
            Self::SetSources(sources) => format!("!s{}\n", sources.join(",")),
            Self::UnsetSources => "!s-*\n".to_owned(),
            Self::AsSetMembers(q) | Self::RouteSetMembers(q) => format!("!i{}\n", q),
            Self::AsSetMembersRecursive(q) | Self::RouteSetMembersRecursive(q) => {
                format!("!i{},1\n", q)
            }
            Self::Ipv4Routes(q) => format!("!g{}\n", q),
            Self::Ipv6Routes(q) => format!("!6{}\n", q),
            Self::RpslObject(class, q) => format!("!m{},{}\n", class, q),
            Self::MntBy(q) => format!("!o{}\n", q),
            Self::Origins(q) => format!("!r{},o\n", q),
            Self::RoutesExact(q) => format!("!r{}\n", q),
            Self::RoutesLess(q) => format!("!r{},l\n", q),
            Self::RoutesLessEqual(q) => format!("!r{},L\n", q),
            Self::RoutesMore(q) => format!("!r{},M\n", q),
        }
    }

    pub(crate) fn expect_data(&self) -> bool {
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

    pub(crate) fn parse_item(&self, input: &[u8]) -> QueryResult<(usize, String)> {
        let parse_result = match self {
            _ if !self.expect_data() => parse::noop(input),
            Self::Version => parse::all(input),
            Self::RpslObject(..)
            | Self::MntBy(_)
            | Self::RoutesExact(_)
            | Self::RoutesLess(_)
            | Self::RoutesLessEqual(_)
            | Self::RoutesMore(_) => parse::paragraph(input),
            _ => parse::word(input),
        };
        match parse_result {
            Ok((_, (consumed, item))) => Ok((consumed, from_utf8(item)?.to_owned())),
            Err(err) => Err(err.into()),
        }
    }
}

impl IntoIterator for Query {
    type Item = Self;
    type IntoIter = Once<Self>;
    fn into_iter(self) -> Self::IntoIter {
        once(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, strum::Display)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum RpslObjectClass {
    #[strum(to_string = "mntner")]
    Mntner,
    #[strum(to_string = "person")]
    Person,
    #[strum(to_string = "role")]
    Role,
    #[strum(to_string = "route")]
    Route,
    #[strum(to_string = "route6")]
    Route6,
    #[strum(to_string = "aut-num")]
    AutNum,
    #[strum(to_string = "inet-rtr")]
    InetRtr,
    #[strum(to_string = "as-set")]
    AsSet,
    #[strum(to_string = "route-set")]
    RouteSet,
    #[strum(to_string = "filter-set")]
    FilterSet,
    #[strum(to_string = "rtr-set")]
    RtrSet,
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
                    any::<String>().prop_map(Self::AsSetMembers),
                    any::<String>().prop_map(Self::AsSetMembersRecursive),
                    any::<String>().prop_map(Self::RouteSetMembers),
                    any::<String>().prop_map(Self::RouteSetMembersRecursive),
                    any::<String>().prop_map(Self::Ipv4Routes),
                    any::<String>().prop_map(Self::Ipv6Routes),
                    any::<(RpslObjectClass, String)>()
                        .prop_map(|(class, object)| Self::RpslObject(class, object)),
                    any::<String>().prop_map(Self::MntBy),
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
                q.parse_item(&input);
            }
        }
    }
}
