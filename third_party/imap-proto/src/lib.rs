#[macro_use]
extern crate nom;

#[cfg(test)]
#[macro_use]
extern crate assert_matches;

#[macro_use]
mod macros;

pub mod body;
pub mod body_structure;
pub mod builders;
pub mod core;
mod parser;
pub mod types;

pub use crate::parser::{parse_response, resp_metadata, ParseResult};
pub use crate::types::*;

use std::borrow::Cow;

// Returns an escaped string if necessary for use as a "quoted" string per
// the IMAPv4 RFC. Return value does not include surrounding quote characters.
// Will return Err if the argument contains illegal characters.
//
// Relevant definitions from RFC 3501 formal syntax:
//
// string = quoted / literal [literal elided here]
// quoted = DQUOTE *QUOTED-CHAR DQUOTE
// QUOTED-CHAR = <any TEXT-CHAR except quoted-specials> / "\" quoted-specials
// quoted-specials = DQUOTE / "\"
// TEXT-CHAR = <any CHAR except CR and LF>
fn quoted_string(s: &str) -> Result<Cow<'_, str>, &'static str> {
    let bytes = s.as_bytes();
    let (mut start, mut new) = (0, Vec::<u8>::new());
    for (i, b) in bytes.iter().enumerate() {
        match *b {
            b'\r' | b'\n' => {
                return Err("CR and LF not allowed in quoted strings");
            }
            b'\\' | b'"' => {
                if start < i {
                    new.extend(&bytes[start..i]);
                }
                new.push(b'\\');
                new.push(*b);
                start = i + 1;
            }
            _ => {}
        };
    }
    if start == 0 {
        Ok(Cow::Borrowed(s))
    } else {
        if start < bytes.len() {
            new.extend(&bytes[start..]);
        }
        // Since the argument is a str, it must contain valid UTF-8. Since
        // this function's transformation preserves the UTF-8 validity,
        // unwrapping here should be okay.
        Ok(Cow::Owned(String::from_utf8(new).unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::quoted_string;
    #[test]
    fn test_quoted_string() {
        assert_eq!(quoted_string("a").unwrap(), "a");
        assert_eq!(quoted_string("").unwrap(), "");
        assert_eq!(quoted_string("a\"b\\c").unwrap(), "a\\\"b\\\\c");
        assert_eq!(quoted_string("\"foo\\").unwrap(), "\\\"foo\\\\");
        assert!(quoted_string("\n").is_err());
    }
}
