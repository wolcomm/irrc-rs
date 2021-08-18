use std::str::from_utf8;

use nom::{
    branch::alt,
    bytes::streaming::{tag, take_till, take_till1, take_until},
    character::{
        is_newline,
        streaming::{char, digit1, newline, space0},
    },
    combinator::{consumed, map, map_res},
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

pub fn paragraph(_: &[u8]) -> IResult<&[u8], (usize, &[u8])> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use nom::Finish;
    use paste::paste;

    use super::*;

    macro_rules! assert_incomplete {
        ( $( $desc:ident: $input:literal ),* $(,)? ) => {
            paste! {
                $(
                    #[test]
                    fn [<$desc _is_incomplete>]() {
                        let input = dbg!($input);
                        assert!(response_status(input).unwrap_err().is_incomplete())
                    }
                )*
            }
        }
    }

    macro_rules! assert_error_kind {
        ( $( $desc:ident: $input:literal => $kind:ident ),* $(,)? ) => {
            paste! {
                $(
                    #[test]
                    #[allow(non_snake_case)]
                    fn [<$desc _is_ $kind _error>]() {
                        let input = dbg!($input);
                        let err = response_status(input).finish().unwrap_err();
                        assert_eq!(err.code, nom::error::ErrorKind::$kind)
                    }
                )*
            }
        }
    }

    mod status {
        use super::*;

        macro_rules! assert_response_result {
            ( $( $desc:ident: $input:literal => $result:expr ),* $(,)? ) => {
                paste! {
                    $(
                        #[test]
                        fn [<$desc _is_valid_result>]() {
                            let input = dbg!($input);
                            let (_, (consumed, result)) = response_status(input).unwrap();
                            assert_eq!(input.len(), consumed);
                            assert_eq!(result, $result);
                        }
                    )*
                }
            }
        }

        assert_incomplete!(
            empty: b"",
            unterminated_ok_none: b"C",
            unterminated_err_not_found: b"D",
            unterminated_err_not_unique: b"E",
            ok_data_no_length: b"A",
            unterminated_ok_data: b"A1",
            err_other_no_msg: b"F",
            unterminated_err_other: b"F foo",
        );

        assert_error_kind!(
            null: b"\n" => Char,
            unknown_status: b"Z" => Char,
            missing_length: b"A\n" => Char,
            invalid_length: b"Afoo" => Char,
            unexpected_length: b"C1" => Char,
            missing_err_msg: b"F\n" => Char,
            missing_err_msg_delimiter: b"Fmsg" => Char,
            non_utf8_err_msg: b"F \xc0\n" => MapRes,
        );

        assert_response_result!(
            ok_data_nil_length: b"A0\n" => Ok(Some(0)),
            ok_data_with_length: b"A101\n" => Ok(Some(101)),
            ok_none: b"C\n" => Ok(None),
            err_not_found: b"D\n" => Err(ResponseError::KeyNotFound),
            err_not_unique: b"E\n" => Err(ResponseError::KeyNotUnique),
            err_other: b"F foo\n" => Err(ResponseError::Other("foo".to_string())),
        );
    }
}
