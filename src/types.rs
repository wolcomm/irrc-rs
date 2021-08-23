use std::fmt;
use std::num::ParseIntError;
use std::str::FromStr;

use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case, take_while1},
    character::complete::digit1,
    combinator::{all_consuming, map, map_res, verify},
    multi::separated_list1,
    sequence::preceded,
    Finish, IResult, Parser,
};

type ParseError = nom::error::Error<String>;
type ParseResult<'a, T> = IResult<&'a str, T>;

trait Parsable: Sized {
    fn parse(input: &str) -> ParseResult<'_, Self>;

    fn parse_all(input: &str) -> ParseResult<'_, Self> {
        all_consuming(Self::parse)(input)
    }
}

macro_rules! impl_from_str {
    ( $this:ident ) => {
        impl FromStr for $this {
            type Err = ParseError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::parse_all(s)
                    .map_err(|err| err.to_owned())
                    .finish()
                    .map(|(_, this)| this)
            }
        }
    };
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// RPSL `aut-num` name: a representation of an autonomous system number. See
/// [RFC2622].
///
/// [RFC2622]: https://datatracker.ietf.org/doc/html/rfc2622#section-6
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct AutNum(u32);

impl_from_str!(AutNum);

impl AutNum {
    /// Get the ASN as a `u32`.
    pub fn asn(&self) -> u32 {
        self.0
    }
}

impl Parsable for AutNum {
    fn parse(input: &str) -> ParseResult<Self> {
        map_res(
            preceded(tag_no_case("AS"), digit1),
            |asn: &str| -> Result<_, ParseIntError> { Ok(Self(asn.parse()?)) },
        )(input)
    }
}

impl fmt::Display for AutNum {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "AS{}", self.0)
    }
}

impl From<u32> for AutNum {
    fn from(asn: u32) -> Self {
        Self(asn)
    }
}

/// RPSL `as-set` name. See [RFC2622].
///
/// [RFC2622]: https://datatracker.ietf.org/doc/html/rfc2622#section-5.1
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct AsSet(Vec<AsSetComponent>);

impl_from_str!(AsSet);

impl Parsable for AsSet {
    fn parse(input: &str) -> ParseResult<Self> {
        verify(
            map(separated_list1(tag(":"), AsSetComponent::parse), Self),
            |as_set| as_set.0.iter().any(AsSetComponent::is_named),
        )(input)
    }
}

impl fmt::Display for AsSet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            self.0
                .iter()
                .map(|comp| comp.to_string())
                .reduce(|lhs, rhs| format!("{}:{}", lhs, rhs))
                .unwrap()
        )
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum AsSetComponent {
    AutNum(AutNum),
    Named(AsSetName),
}

impl AsSetComponent {
    fn is_named(&self) -> bool {
        matches!(self, Self::Named(_))
    }
}

impl Parsable for AsSetComponent {
    fn parse(input: &str) -> ParseResult<Self> {
        alt((
            map(AutNum::parse, Self::AutNum),
            map(AsSetName::parse, Self::Named),
        ))(input)
    }
}

impl fmt::Display for AsSetComponent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::AutNum(autnum) => autnum.fmt(f),
            Self::Named(name) => name.fmt(f),
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct AsSetName(String);

impl Parsable for AsSetName {
    fn parse(input: &str) -> ParseResult<Self> {
        map(
            tag_no_case("AS-").and(take_while1(is_name_char)),
            |(prefix, name): (&str, &str)| Self(format!("{}{}", prefix, name)),
        )(input)
    }
}

impl fmt::Display for AsSetName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// RPSL `route-set` name. See [RFC2622].
///
/// [RFC2622]: https://datatracker.ietf.org/doc/html/rfc2622#section-5.2
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct RouteSet(Vec<RouteSetComponent>);

impl_from_str!(RouteSet);

impl Parsable for RouteSet {
    fn parse(input: &str) -> ParseResult<Self> {
        verify(
            map(separated_list1(tag(":"), RouteSetComponent::parse), Self),
            |route_set| route_set.0.iter().any(RouteSetComponent::is_named),
        )(input)
    }
}

impl fmt::Display for RouteSet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            self.0
                .iter()
                .map(|comp| comp.to_string())
                .reduce(|lhs, rhs| format!("{}:{}", lhs, rhs))
                .unwrap()
        )
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum RouteSetComponent {
    AutNum(AutNum),
    Named(RouteSetName),
}

impl RouteSetComponent {
    fn is_named(&self) -> bool {
        matches!(self, Self::Named(_))
    }
}

impl Parsable for RouteSetComponent {
    fn parse(input: &str) -> ParseResult<Self> {
        alt((
            map(AutNum::parse, Self::AutNum),
            map(RouteSetName::parse, Self::Named),
        ))(input)
    }
}

impl fmt::Display for RouteSetComponent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::AutNum(autnum) => autnum.fmt(f),
            Self::Named(name) => name.fmt(f),
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct RouteSetName(String);

impl Parsable for RouteSetName {
    fn parse(input: &str) -> ParseResult<Self> {
        map(
            tag_no_case("RS-").and(take_while1(is_name_char)),
            |(prefix, name): (&str, &str)| Self(format!("{}{}", prefix, name)),
        )(input)
    }
}

impl fmt::Display for RouteSetName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

// TODO
/// Enumeration of the RPSL object class names permitted to appear in the
/// `members:` attribute of an `as-set`.
///
/// See [RFC2622](https://datatracker.ietf.org/doc/html/rfc2622#section-5.1).
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum AsSetMember {
    /// The name of an `aut-num`.
    AutNum(AutNum),
    /// The name of an `as-set`.
    AsSet(AsSet),
}

// TODO (refer RFC4012 S4.2)
/// Enumeration of the RPSL object class names permitted to appear in the
/// `members:` attribute of a `route-set`.
///
/// See [RFC2622](https://datatracker.ietf.org/doc/html/rfc2622#section-5.2).
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum RouteSetMember {
    /// An IPv4 prefix with optional [`RangeOperator`].
    Route(String, Option<RangeOperator>),
    /// The name of a `route-set` with optional [`RangeOperator`].
    RouteSet(RouteSet, Option<RangeOperator>),
    /// The name of an `aut-num` with optional [`RangeOperator`].
    AutNum(AutNum, Option<RangeOperator>),
    /// The name of an `as-set` with optional [`RangeOperator`].
    AsSet(AsSet, Option<RangeOperator>),
}

// TODO
/// Operators that may be applied to RPSL object class names when evaluated
/// in a `route-set` context.
///
/// See [RFC2622](https://datatracker.ietf.org/doc/html/rfc2622#section-2).
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum RangeOperator {
    /// Exclusive more-specifics operator (`^-`).
    MoreExclusive,
    /// Inclusive more-specifics operator (`^+`).
    MoreInclusive,
    /// More specifics of length `n` operator (`^n`).
    SubprefixExact(u8),
    /// More specifics of lengths `m` to `n` operator (`^m-n`).
    SubprefixRange(u8, u8),
}

/// RPSL `mntner` name. See [RFC2622].
///
/// [RFC2622]: https://datatracker.ietf.org/doc/html/rfc2622#section-3.1
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Mntner(String);

impl_from_str!(Mntner);

impl Parsable for Mntner {
    fn parse(input: &str) -> ParseResult<Self> {
        map(take_while1(is_name_char), |name: &str| {
            Self(name.to_string())
        })(input)
    }
}

impl fmt::Display for Mntner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use proptest::{arbitrary::ParamsFor, collection::size_range, prelude::*};

    use super::*;

    macro_rules! display_fmt_parses {
        ( $t:ty ) => {
            proptest! {
                #[test]
                fn display_fmt_parses(autnum in any::<$t>()) {
                    assert_eq!(autnum, autnum.to_string().parse().unwrap())
                }
            }
        };
    }

    mod autnum {
        use super::*;

        impl Arbitrary for AutNum {
            type Parameters = ParamsFor<u32>;
            type Strategy = BoxedStrategy<Self>;
            fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
                any_with::<u32>(args).prop_map(Self::from).boxed()
            }
        }

        display_fmt_parses!(AutNum);
    }

    mod as_set {
        use super::*;

        const AS_SET_NAME: &str = "[Aa][Ss]-[A-Za-z0-9_-]+";

        impl Arbitrary for AsSetComponent {
            type Parameters = ();
            type Strategy = BoxedStrategy<Self>;
            fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
                prop_oneof![
                    any::<AutNum>().prop_map(Self::AutNum),
                    AS_SET_NAME.prop_map(|s| Self::Named(AsSetName(s)))
                ]
                .boxed()
            }
        }

        impl Arbitrary for AsSet {
            type Parameters = ();
            type Strategy = BoxedStrategy<Self>;
            fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
                (
                    AS_SET_NAME.prop_map(|s| AsSetComponent::Named(AsSetName(s))),
                    any_with::<Vec<AsSetComponent>>(size_range(0..9).lift()),
                )
                    .prop_map(|(named, mut components)| {
                        components.push(named);
                        components
                    })
                    .prop_shuffle()
                    .prop_map(Self)
                    .boxed()
            }
        }

        display_fmt_parses!(AsSet);

        #[test]
        fn cannot_be_empty() {
            assert!("".parse::<AsSet>().is_err())
        }

        #[test]
        fn simple_set_parses() {
            assert_eq!(
                "AS-FOO".parse::<AsSet>().unwrap(),
                AsSet(vec![AsSetComponent::Named(AsSetName("AS-FOO".into()))])
            )
        }

        #[test]
        fn hierarchical_set_parses() {
            assert_eq!(
                "AS65000:AS-FOO".parse::<AsSet>().unwrap(),
                AsSet(vec![
                    AsSetComponent::AutNum(AutNum(65000)),
                    AsSetComponent::Named(AsSetName("AS-FOO".into()))
                ])
            )
        }

        #[test]
        fn cannot_be_single_autnnum() {
            assert!("AS65000".parse::<AsSet>().is_err())
        }

        #[test]
        fn must_have_named_component() {
            assert!("AS65000:AS65001".parse::<AsSet>().is_err())
        }
    }

    mod route_set {
        use super::*;

        const ROUTE_SET_NAME: &str = "[Rr][Ss]-[A-Za-z0-9_-]+";

        impl Arbitrary for RouteSetComponent {
            type Parameters = ();
            type Strategy = BoxedStrategy<Self>;
            fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
                prop_oneof![
                    any::<AutNum>().prop_map(Self::AutNum),
                    ROUTE_SET_NAME.prop_map(|s| Self::Named(RouteSetName(s)))
                ]
                .boxed()
            }
        }

        impl Arbitrary for RouteSet {
            type Parameters = ();
            type Strategy = BoxedStrategy<Self>;
            fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
                (
                    ROUTE_SET_NAME.prop_map(|s| RouteSetComponent::Named(RouteSetName(s))),
                    any_with::<Vec<RouteSetComponent>>(size_range(1..9).lift()),
                )
                    .prop_map(|(named, mut components)| {
                        components.push(named);
                        components
                    })
                    .prop_shuffle()
                    .prop_map(Self)
                    .boxed()
            }
        }

        display_fmt_parses!(RouteSet);

        #[test]
        fn cannot_be_empty() {
            assert!("".parse::<RouteSet>().is_err())
        }

        #[test]
        fn simple_set_parses() {
            assert_eq!(
                "RS-FOO".parse::<RouteSet>().unwrap(),
                RouteSet(vec![RouteSetComponent::Named(RouteSetName(
                    "RS-FOO".into()
                ))])
            )
        }

        #[test]
        fn hierarchical_set_parses() {
            assert_eq!(
                "AS65000:RS-FOO".parse::<RouteSet>().unwrap(),
                RouteSet(vec![
                    RouteSetComponent::AutNum(AutNum(65000)),
                    RouteSetComponent::Named(RouteSetName("RS-FOO".into()))
                ])
            )
        }

        #[test]
        fn cannot_be_single_autnnum() {
            assert!("AS65000".parse::<RouteSet>().is_err())
        }

        #[test]
        fn must_have_named_component() {
            assert!("AS65000:AS65001".parse::<RouteSet>().is_err())
        }
    }

    mod mntner {
        use super::*;

        impl Arbitrary for Mntner {
            type Parameters = ();
            type Strategy = BoxedStrategy<Self>;
            fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
                "[A-Za-z0-9_-]+".prop_map(Self).boxed()
            }
        }

        display_fmt_parses!(Mntner);
    }
}
