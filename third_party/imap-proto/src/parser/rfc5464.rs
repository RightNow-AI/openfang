//!
//! https://tools.ietf.org/html/rfc5464
//!
//! IMAP METADATA extension
//!

// rustfmt doesn't do a very good job on nom parser invocations.
#![cfg_attr(rustfmt, rustfmt_skip)]

use nom::IResult;

use crate::core::*;
use crate::types::*;
use std::str;

fn is_entry_component_char(c: u8) -> bool {
    c < 0x80 && c > 0x19 && c != b'*' && c != b'%' && c != b'/'
}

enum EntryParseStage<'a> {
    PrivateShared,
    Admin(usize),
    VendorComment(usize),
    Path(usize),
    Done(usize),
    Fail(nom::Err<&'a [u8]>),
}

fn check_private_shared(i: &[u8]) -> EntryParseStage<'_> {
    if i.starts_with(b"/private") {
        EntryParseStage::VendorComment(8)
    } else if i.starts_with(b"/shared") {
        EntryParseStage::Admin(7)
    } else {
        EntryParseStage::Fail(nom::Err::Error(
            b"Entry Name doesn't start with /private or /shared",
        ))
    }
}

fn check_admin(i: &[u8], l: usize) -> EntryParseStage<'_> {
    if i[l..].starts_with(b"/admin") {
        EntryParseStage::Path(l + 6)
    } else {
        EntryParseStage::VendorComment(l)
    }
}

fn check_vendor_comment(i: &[u8], l: usize) -> EntryParseStage<'_> {
    if i[l..].starts_with(b"/comment") {
        EntryParseStage::Path(l + 8)
    } else if i[l..].starts_with(b"/vendor") {
        //make sure vendor name is present
        if i.len() < l + 9 || i[l + 7] != b'/' || !is_entry_component_char(i[l + 8]) {
            EntryParseStage::Fail(nom::Err::Incomplete(nom::Needed::Unknown))
        } else {
            EntryParseStage::Path(l + 7)
        }
    } else {
        EntryParseStage::Fail(nom::Err::Error(
            b"Entry name is not continued with /admin, /vendor or /comment",
        ))
    }
}

fn check_path(i: &[u8], l: usize) -> EntryParseStage<'_> {
    if i.len() == l || i[l] == b' ' || i[l] == b'\r' {
        return EntryParseStage::Done(l);
    } else if i[l] != b'/' {
        return EntryParseStage::Fail(nom::Err::Error(b"Entry name path is corrupted"));
    }
    for j in 1..(i.len() - l) {
        if !is_entry_component_char(i[l + j]) {
            return EntryParseStage::Path(l + j);
        }
    }
    EntryParseStage::Done(i.len())
}

fn check_entry_name(i: &[u8]) -> IResult<&[u8], &[u8]> {
    let mut stage = EntryParseStage::PrivateShared;
    loop {
        match stage {
            EntryParseStage::PrivateShared => {
                stage = check_private_shared(i);
            }
            EntryParseStage::Admin(l) => {
                stage = check_admin(i, l);
            }
            EntryParseStage::VendorComment(l) => {
                stage = check_vendor_comment(i, l);
            }
            EntryParseStage::Path(l) => {
                stage = check_path(i, l);
            }
            EntryParseStage::Done(l) => {
                return Ok((&i[l..], &i[..l]));
            }
            EntryParseStage::Fail(nom::Err::Error(err_msg)) => {
                return std::result::Result::Err(nom::Err::Error((
                    err_msg,
                    nom::error::ErrorKind::Verify,
                )));
            }
            EntryParseStage::Fail(nom::Err::Incomplete(reason)) => {
                return std::result::Result::Err(nom::Err::Incomplete(reason));
            }
            _ => panic!("Entry name verification failure"),
        }
    }
}

fn entry_name(i: &[u8]) -> IResult<&[u8], &[u8]> {
    let astring_res = astring(i)?;
    check_entry_name(astring_res.1)?;
    Ok(astring_res)
}

fn slice_to_str(i: &[u8]) -> &str {
    str::from_utf8(i).unwrap()
}

named!(nil_value<Option<String>>, do_parse!(
    tag_no_case!("NIL") >>
        (None)
));

named!(string_value<Option<String>>, do_parse!(
    value: map!(alt!(quoted | literal), slice_to_str) >>
        (Some(value.to_string()))
));

named!(keyval_list<Vec<Metadata>>, do_parse!(
    list: parenthesized_nonempty_list!(do_parse!(
        key: map!(entry_name, slice_to_str) >>
            tag!(" ") >>
            value: alt!(nil_value | string_value) >>
            (Metadata{entry: key.to_string(), value})
    )) >>
        (list)
));

named!(entry_list<Vec<&str>>, do_parse!(
    list: separated_list!(tag!(" "), map!(entry_name, slice_to_str)) >>
        (list)
));

named!(metadata_common<&[u8]>, do_parse!(
    tag_no_case!("METADATA ") >>
        mbox: quoted >>
        tag!(" ") >>
        (mbox)
));

// [RFC5464 - 4.4.1 METADATA Response with values]
named!(metadata_solicited<Response>, do_parse!(
    mbox: metadata_common >>
    tail: keyval_list >>
        (Response::MailboxData(MailboxDatum::MetadataSolicited {
            mailbox:slice_to_str(mbox), values:tail
        }))
));

// [RFC5464 - 4.4.2 Unsolicited METADATA Response without values]
named!(metadata_unsolicited<Response>, do_parse!(
    mbox: metadata_common >>
        tail: entry_list >>
        (Response::MailboxData(MailboxDatum::MetadataUnsolicited {
            mailbox:slice_to_str(mbox), values:tail
        }))
));

// Parse solicited or unsolicited METADATA response.
named!(pub resp_metadata<Response>, do_parse!(
    r: alt!(metadata_solicited |
            metadata_unsolicited) >>
        (r)
));

#[cfg(test)]
mod tests {
    use super::{metadata_solicited, metadata_unsolicited};
    use crate::types::*;

    #[test]
    fn test_solicited_fail_1() {
        match metadata_solicited(b"METADATA \"\" (/asdfg \"asdf\")\r\n") {
            Err(_) => {}
            _ => panic!("Error required when entry name is not starting with /private or /shared"),
        }
    }

    #[test]
    fn test_solicited_fail_2() {
        match metadata_solicited(b"METADATA \"\" (/shared/asdfg \"asdf\")\r\n") {
            Err(_) => {}
            _ => panic!(
                "Error required when in entry name /shared \
                 is not continued with /admin, /comment or /vendor"
            ),
        }
    }

    #[test]
    fn test_solicited_fail_3() {
        match metadata_solicited(b"METADATA \"\" (/private/admin \"asdf\")\r\n") {
            Err(_) => {}
            _ => panic!(
                "Error required when in entry name /private \
                 is not continued with /comment or /vendor"
            ),
        }
    }

    #[test]
    fn test_solicited_fail_4() {
        match metadata_solicited(b"METADATA \"\" (/shared/vendor \"asdf\")\r\n") {
            Err(_) => {}
            _ => panic!("Error required when vendor name is not provided."),
        }
    }

    #[test]
    fn test_solicited_success() {
        match metadata_solicited(
            b"METADATA \"mbox\" (/shared/vendor/vendorname \"asdf\" \
              /private/comment/a \"bbb\")\r\n",
        ) {
            Ok((i, Response::MailboxData(MailboxDatum::MetadataSolicited { mailbox, values }))) => {
                assert_eq!(mailbox, "mbox");
                assert_eq!(i, b"\r\n");
                assert_eq!(values.len(), 2);
                assert_eq!(values[0].entry, "/shared/vendor/vendorname");
                assert_eq!(
                    values[0]
                        .value
                        .as_ref()
                        .expect("None value is not expected"),
                    "asdf"
                );
                assert_eq!(values[1].entry, "/private/comment/a");
                assert_eq!(
                    values[1]
                        .value
                        .as_ref()
                        .expect("None value is not expected"),
                    "bbb"
                );
            }
            _ => panic!("Correct METADATA response is not parsed properly."),
        }
    }

    #[test]
    fn test_literal_success() {
        // match metadata_solicited(b"METADATA \"\" (/shared/vendor/vendor.coi/a \"AAA\")\r\n")
        match metadata_solicited(b"METADATA \"\" (/shared/vendor/vendor.coi/a {3}\r\nAAA)\r\n") {
            Ok((i, Response::MailboxData(MailboxDatum::MetadataSolicited { mailbox, values }))) => {
                assert_eq!(mailbox, "");
                assert_eq!(i, b"\r\n");
                assert_eq!(values.len(), 1);
                assert_eq!(values[0].entry, "/shared/vendor/vendor.coi/a");
                assert_eq!(
                    values[0]
                        .value
                        .as_ref()
                        .expect("None value is not expected"),
                    "AAA"
                );
            }
            Err(e) => panic!("ERR: {:?}", e),
            _ => panic!("Strange failure"),
        }
    }

    #[test]
    fn test_unsolicited_success() {
        match metadata_unsolicited(b"METADATA \"theBox\" /shared/admin/qwe /private/comment/a\r\n")
        {
            Ok((
                i,
                Response::MailboxData(MailboxDatum::MetadataUnsolicited { mailbox, values }),
            )) => {
                assert_eq!(i, b"\r\n");
                assert_eq!(mailbox, "theBox");
                assert_eq!(values.len(), 2);
                assert_eq!(values[0], "/shared/admin/qwe");
                assert_eq!(values[1], "/private/comment/a");
            }
            _ => panic!("Correct METADATA response is not parsed properly."),
        }
    }
}
