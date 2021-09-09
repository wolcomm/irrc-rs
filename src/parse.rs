use std::str::from_utf8;

use nom::{
    branch::alt,
    bytes::streaming::{tag, take_till, take_till1, take_until},
    character::{
        is_newline,
        streaming::{char, digit1, newline, space0},
    },
    combinator::{consumed, map, map_res, opt},
    sequence::{delimited, terminated},
    IResult,
};

use crate::error::ResponseError;

const EOR: &[u8] = b"\nC\n";

type ResponseResult = Result<Option<usize>, ResponseError>;

fn resp_ok_data(input: &[u8]) -> IResult<&[u8], ResponseResult> {
    let (rem, _) = char('A')(input)?;
    let (rem, len) = terminated(
        map_res(map_res(digit1, from_utf8), |input| input.parse()),
        newline,
    )(rem)?;
    Ok((rem, Ok(Some(len))))
}

fn resp_ok_none(input: &[u8]) -> IResult<&[u8], ResponseResult> {
    let (rem, _) = terminated(char('C'), newline)(input)?;
    Ok((rem, Ok(None)))
}

fn resp_err_not_found(input: &[u8]) -> IResult<&[u8], ResponseResult> {
    let (rem, _) = terminated(char('D'), newline)(input)?;
    Ok((rem, Err(ResponseError::KeyNotFound)))
}

fn resp_err_not_unique(input: &[u8]) -> IResult<&[u8], ResponseResult> {
    let (rem, _) = terminated(char('E'), newline)(input)?;
    Ok((rem, Err(ResponseError::KeyNotUnique)))
}

fn resp_err_other(input: &[u8]) -> IResult<&[u8], ResponseResult> {
    let (rem, _) = char('F')(input)?;
    let (rem, msg) = map_res(
        delimited(char(' '), take_till(is_newline), newline),
        from_utf8,
    )(rem)?;
    Ok((rem, Err(ResponseError::Other(msg.to_owned()))))
}

pub fn response_status(input: &[u8]) -> IResult<&[u8], (usize, ResponseResult)> {
    map(
        consumed(alt((
            resp_ok_data,
            resp_ok_none,
            resp_err_not_found,
            resp_err_not_unique,
            resp_err_other,
        ))),
        |(consumed, result)| (consumed.len(), result),
    )(input)
}

pub fn end_of_response(input: &[u8]) -> IResult<&[u8], usize> {
    map(consumed(tag(EOR)), |(consumed, _): (&[u8], &[u8])| {
        consumed.len()
    })(input)
}

fn till_word_end(input: &[u8]) -> IResult<&[u8], &[u8]> {
    take_till1(|b| b == b' ' || b == b'\n')(input)
}

fn take_word_strip(input: &[u8]) -> IResult<&[u8], &[u8]> {
    let (rem, res) = till_word_end(input)?;
    let (rem, _) = space0(rem)?;
    Ok((rem, res))
}

pub fn word(input: &[u8]) -> IResult<&[u8], (usize, &[u8])> {
    map(consumed(take_word_strip), |(consumed, word)| {
        (consumed.len(), word)
    })(input)
}

pub fn all(input: &[u8]) -> IResult<&[u8], (usize, &[u8])> {
    map(
        consumed(take_until(EOR)),
        |(consumed, data): (&[u8], &[u8])| (consumed.len(), data),
    )(input)
}

pub fn noop(input: &[u8]) -> IResult<&[u8], (usize, &[u8])> {
    Ok((input, (0, &[])))
}

pub fn paragraph(input: &[u8]) -> IResult<&[u8], (usize, &[u8])> {
    map(
        consumed(take_paragraph),
        |(consumed, paragraph): (&[u8], &[u8])| (consumed.len(), paragraph),
    )(input)
}

fn take_paragraph(input: &[u8]) -> IResult<&[u8], &[u8]> {
    let (rem, _) = opt(newline)(input)?;
    let (rem, res) = match take_until("\n\n")(rem) {
        Ok((rem, res)) => {
            let (rem, _) = newline(rem)?;
            (rem, res)
        }
        err @ Err(_) => match take_until::<_, _, (&[u8], _)>(EOR)(rem) {
            Ok((rem, res)) => (rem, res),
            Err(_) => return err,
        },
    };
    Ok((rem, res))
}

#[cfg(test)]
mod tests {
    use nom::Finish;
    use paste::paste;
    use proptest::prelude::*;

    use super::*;

    macro_rules! does_not_panic {
        ( $fn:ident ) => {
            proptest! {
                #[test]
                #[allow(unused_must_use)]
                fn does_not_panic(input in any::<Vec<u8>>()) {
                    $fn(&input);
                }
            }
        };
    }

    macro_rules! assert_incomplete_parse {
        ( $fn:ident { $( $desc:ident: $input:literal ),* $(,)? } ) => {
            paste! {
                $(
                    #[test]
                    fn [<$desc _is_incomplete>]() {
                        let input = dbg!($input);
                        assert!($fn(input).unwrap_err().is_incomplete())
                    }
                )*
            }
        }
    }

    macro_rules! assert_error_kind {
        ( $fn:ident { $( $desc:ident: $input:literal => $kind:ident ),* $(,)? } ) => {
            paste! {
                $(
                    #[test]
                    fn [<$desc _is_ $kind:snake _error>]() {
                        let input = dbg!($input);
                        let err = $fn(input).finish().unwrap_err();
                        assert_eq!(err.code, nom::error::ErrorKind::$kind)
                    }
                )*
            }
        }
    }

    macro_rules! assert_parse_result {
        ( $fn:ident { $( $desc:ident: $input:literal => ( $consumed:literal, $result:expr ) ),* $(,)? } ) => {
            paste! {
                $(
                    #[test]
                    fn [<$desc _is_valid_result>]() {
                        let input = dbg!($input);
                        let (_, (consumed, result)) = $fn(input).unwrap();
                        assert_eq!(consumed, $consumed);
                        assert_eq!(result, $result);
                    }
                )*
            }
        }
    }

    mod status {
        use super::*;

        does_not_panic!(response_status);

        assert_incomplete_parse!(response_status {
            empty: b"",
            unterminated_ok_none: b"C",
            unterminated_err_not_found: b"D",
            unterminated_err_not_unique: b"E",
            ok_data_no_length: b"A",
            unterminated_ok_data: b"A1",
            err_other_no_msg: b"F",
            unterminated_err_other: b"F foo",
        });

        assert_error_kind!(
            response_status {
                null: b"\n" => Char,
                unknown_status: b"Z" => Char,
                missing_length: b"A\n" => Char,
                invalid_length: b"Afoo" => Char,
                unexpected_length: b"C1" => Char,
                missing_err_msg: b"F\n" => Char,
                missing_err_msg_delimiter: b"Fmsg" => Char,
                non_utf8_err_msg: b"F \xc0\n" => MapRes,
            }
        );

        assert_parse_result!(
            response_status {
                ok_data_nil_length: b"A0\n" => (3, Ok(Some(0))),
                ok_data_with_length: b"A101\n" => (5, Ok(Some(101))),
                ok_none: b"C\n" => (2, Ok(None)),
                err_not_found: b"D\n" => (2, Err(ResponseError::KeyNotFound)),
                err_not_unique: b"E\n" => (2, Err(ResponseError::KeyNotUnique)),
                err_other: b"F foo\n" => (6, Err(ResponseError::Other("foo".to_string()))),
            }
        );
    }

    mod end_of_response {
        use super::*;

        does_not_panic!(end_of_response);

        assert_incomplete_parse!(end_of_response {
            empty: b"",
            partial: b"\nC",
        });

        assert_error_kind!(
            end_of_response {
                missing_newline: b"C" => Tag,
                missing_char: b"\n\n" => Tag,
            }
        );

        #[test]
        fn eor() {
            assert_eq!(end_of_response(EOR), Ok((b"" as &[u8], 3)))
        }

        proptest! {
            #[test]
            fn parses_with_arbitary_trailing_input(
                input in proptest::string::bytes_regex("\nC\n.*").unwrap(),
            ) {
                assert_eq!(end_of_response(&input), Ok((&input[3..], 3)))
            }

            #[test]
            fn fails_with_arbitary_leading_input(
                input in proptest::string::bytes_regex("[^\n].*").unwrap()
            ) {
                assert_eq!(
                    end_of_response(&input).finish().unwrap_err().code,
                    nom::error::ErrorKind::Tag,
                )
            }
        }
    }

    mod word {
        use super::*;

        does_not_panic!(word);

        assert_incomplete_parse!(word {
            empty: b"",
            unterminated: b"foo",
        });

        assert_error_kind!(
            word {
                leading_space: b" foo" => TakeTill1,
                leading_newline: b"\nfoo" => TakeTill1,
            }
        );

        assert_parse_result!(
            word {
                trailing_space: b"foo bar" => (4, b"foo"),
                trailing_newline: b"foo\n" => (3, b"foo"),
            }
        );
    }

    mod paragraph {
        use super::*;

        does_not_panic!(paragraph);

        assert_incomplete_parse!(all {
            empty: b"",
            unterminated: b"foo",
        });
    }

    mod all {
        use super::*;

        does_not_panic!(all);

        assert_incomplete_parse!(all {
            empty: b"",
            unterminated: b"foo",
        });

        assert_parse_result!(
            all {
                terminated: b"foo bar baz\nC\n" => (11, b"foo bar baz"),
            }
        );
    }

    mod noop {
        use super::*;

        does_not_panic!(noop);

        assert_parse_result!(noop {
            empty: b"" => (0, b""),
            terminated: b"foo bar baz\nC\n" => (0, b""),
        });
    }
}
