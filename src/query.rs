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
            Self::RpslObject(class, q) => format!("!m{},{}\n", class.class_name(), q),
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RpslObjectClass {
    Mntner,
    Person,
    Role,
    Route,
    Route6,
    AutNum,
    InetRtr,
    AsSet,
    RouteSet,
    FilterSet,
    RtrSet,
    PeeringSet,
}

impl RpslObjectClass {
    fn class_name(&self) -> &str {
        match self {
            Self::Mntner => "mntner",
            Self::Person => "person",
            Self::Role => "role",
            Self::Route => "route",
            Self::Route6 => "route6",
            Self::AutNum => "aut-num",
            Self::InetRtr => "inet-rtr",
            Self::AsSet => "as-set",
            Self::RouteSet => "route-set",
            Self::FilterSet => "filter-set",
            Self::RtrSet => "rtr-set",
            Self::PeeringSet => "peering-set",
        }
    }
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
}
