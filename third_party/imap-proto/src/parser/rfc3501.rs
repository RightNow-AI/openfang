//!
//! https://tools.ietf.org/html/rfc3501
//!
//! INTERNET MESSAGE ACCESS PROTOCOL
//!

// rustfmt doesn't do a very good job on nom parser invocations.
#![cfg_attr(rustfmt, rustfmt_skip)]

use nom::IResult;

use std::str;

use crate::parser::rfc4551;
use crate::parser::rfc5464::resp_metadata;
use crate::types::*;
use crate::core::*;
use crate::body::*;
use crate::body_structure::*;


fn tag_char(c: u8) -> bool {
    c != b'+' && astring_char(c)
}

named!(status_ok<Status>, map!(tag_no_case!("OK"),
    |_s| Status::Ok
));
named!(status_no<Status>, map!(tag_no_case!("NO"),
    |_s| Status::No
));
named!(status_bad<Status>, map!(tag_no_case!("BAD"),
    |_s| Status::Bad
));
named!(status_preauth<Status>, map!(tag_no_case!("PREAUTH"),
    |_s| Status::PreAuth
));
named!(status_bye<Status>, map!(tag_no_case!("BYE"),
    |_s| Status::Bye
));

named!(status<Status>, alt!(
    status_ok |
    status_no |
    status_bad |
    status_preauth |
    status_bye
));

named!(mailbox<&str>, map!(
    astring_utf8,
    |s| {
        if s.eq_ignore_ascii_case("INBOX") {
            "INBOX"
        } else {
            s
        }
    }
));

named!(flag_extension<&str>, map_res!(
    recognize!(pair!(tag!("\\"), take_while!(atom_char))),
    str::from_utf8
));

named!(flag<&str>, alt!(flag_extension | atom));

named!(flag_list<Vec<&str>>, parenthesized_list!(flag));

named!(flag_perm<&str>, alt!(
    map_res!(tag!("\\*"), str::from_utf8) |
    flag
));

named!(resp_text_code_alert<ResponseCode>, do_parse!(
    tag_no_case!("ALERT") >>
    (ResponseCode::Alert)
));

named!(resp_text_code_badcharset<ResponseCode>, do_parse!(
    tag_no_case!("BADCHARSET") >>
    ch: opt!(do_parse!(
        tag!(" ") >>
        charsets: parenthesized_nonempty_list!(astring_utf8) >>
        (charsets)
    )) >>
    (ResponseCode::BadCharset(ch))
));

named!(resp_text_code_capability<ResponseCode>, map!(
    capability_data,
    |c| ResponseCode::Capabilities(c)
));

named!(resp_text_code_parse<ResponseCode>, do_parse!(
    tag_no_case!("PARSE") >>
    (ResponseCode::Parse)
));

named!(resp_text_code_permanent_flags<ResponseCode>, do_parse!(
    tag_no_case!("PERMANENTFLAGS ") >>
    flags: parenthesized_list!(flag_perm) >>
    (ResponseCode::PermanentFlags(flags))
));

named!(resp_text_code_read_only<ResponseCode>, do_parse!(
    tag_no_case!("READ-ONLY") >>
    (ResponseCode::ReadOnly)
));

named!(resp_text_code_read_write<ResponseCode>, do_parse!(
    tag_no_case!("READ-WRITE") >>
    (ResponseCode::ReadWrite)
));

named!(resp_text_code_try_create<ResponseCode>, do_parse!(
    tag_no_case!("TRYCREATE") >>
    (ResponseCode::TryCreate)
));

named!(resp_text_code_uid_validity<ResponseCode>, do_parse!(
    tag_no_case!("UIDVALIDITY ") >>
    num: number >>
    (ResponseCode::UidValidity(num))
));

named!(resp_text_code_uid_next<ResponseCode>, do_parse!(
    tag_no_case!("UIDNEXT ") >>
    num: number >>
    (ResponseCode::UidNext(num))
));

named!(resp_text_code_unseen<ResponseCode>, do_parse!(
    tag_no_case!("UNSEEN ") >>
    num: number >>
    (ResponseCode::Unseen(num))
));

named!(resp_text_code<ResponseCode>, do_parse!(
    tag!("[") >>
    coded: alt!(
        resp_text_code_alert |
        resp_text_code_badcharset |
        resp_text_code_capability |
        resp_text_code_parse |
        resp_text_code_permanent_flags |
        resp_text_code_uid_validity |
        resp_text_code_uid_next |
        resp_text_code_unseen |
        resp_text_code_read_only |
        resp_text_code_read_write |
        resp_text_code_try_create |
        rfc4551::resp_text_code_highest_mod_seq
    ) >>
    // Per the spec, the closing tag should be "] ".
    // See `resp_text` for more on why this is done differently.
    tag!("]") >>
    (coded)
));

named!(capability<Capability>, alt!(
    map!(tag_no_case!("IMAP4rev1"), |_| Capability::Imap4rev1) |
    map!(preceded!(tag_no_case!("AUTH="), atom), |a| Capability::Auth(a)) |
    map!(atom, |a| Capability::Atom(a))
));

fn ensure_capabilities_contains_imap4rev<'a>(capabilities: Vec<Capability<'a>>) -> Result<Vec<Capability<'a>>, ()> {
    if capabilities.contains(&Capability::Imap4rev1) {
        Ok(capabilities)
    } else {
        Err(())
    }
}

named!(capability_data<Vec<Capability>>, map_res!(
    do_parse!(
        tag_no_case!("CAPABILITY") >>
        capabilities: many0!(preceded!(char!(' '), capability)) >>
        (capabilities)
    ),
    ensure_capabilities_contains_imap4rev
));

named!(resp_capability<Response>, map!(
    capability_data,
    |c| Response::Capabilities(c)
));

named!(mailbox_data_search<Response>, do_parse!(
    tag_no_case!("SEARCH") >>
    ids: many0!(do_parse!(
        tag!(" ") >>
        id: number >>
        (id)
    )) >>
    (Response::IDs(ids))
));

named!(mailbox_data_flags<Response>, do_parse!(
    tag_no_case!("FLAGS ") >>
    flags: flag_list >>
    (Response::MailboxData(MailboxDatum::Flags(flags)))
));

named!(mailbox_data_exists<Response>, do_parse!(
    num: number >>
    tag_no_case!(" EXISTS") >>
    (Response::MailboxData(MailboxDatum::Exists(num)))
));

named!(mailbox_list<(Vec<&str>, Option<&str>, &str)>, do_parse!(
    flags: flag_list >>
    tag!(" ") >>
    delimiter: alt!(
        map!(quoted_utf8, |v| Some(v)) |
        map!(nil, |_| None)
    ) >>
    tag!(" ") >>
    name: mailbox >>
    ((flags, delimiter, name))
));

named!(mailbox_data_list<Response>, do_parse!(
    tag_no_case!("LIST ") >>
    data: mailbox_list >>
    (Response::MailboxData(MailboxDatum::List {
        flags: data.0,
        delimiter: data.1,
        name: data.2,
    }))
));

named!(mailbox_data_lsub<Response>, do_parse!(
    tag_no_case!("LSUB ") >>
    data: mailbox_list >>
    (Response::MailboxData(MailboxDatum::List {
        flags: data.0,
        delimiter: data.1,
        name: data.2,
    }))
));

// Unlike `status_att` in the RFC syntax, this includes the value,
// so that it can return a valid enum object instead of just a key.
named!(status_att<StatusAttribute>, alt!(
    rfc4551::status_att_val_highest_mod_seq |
    do_parse!(
        tag_no_case!("MESSAGES ") >>
        val: number >>
        (StatusAttribute::Messages(val))
    ) |
    do_parse!(
        tag_no_case!("RECENT ") >>
        val: number >>
        (StatusAttribute::Recent(val))
    ) |
    do_parse!(
        tag_no_case!("UIDNEXT ") >>
        val: number >>
        (StatusAttribute::UidNext(val))
    ) |
    do_parse!(
        tag_no_case!("UIDVALIDITY ") >>
        val: number >>
        (StatusAttribute::UidValidity(val))
    ) |
    do_parse!(
        tag_no_case!("UNSEEN ") >>
        val: number >>
        (StatusAttribute::Unseen(val))
    )
));

named!(status_att_list<Vec<StatusAttribute>>, parenthesized_nonempty_list!(status_att));

named!(mailbox_data_status<Response>, do_parse!(
    tag_no_case!("STATUS ") >>
    mailbox: mailbox >>
    tag!(" ") >>
    status: status_att_list >>
    (Response::MailboxData(MailboxDatum::Status {
        mailbox,
        status,
    }))
));

named!(mailbox_data_recent<Response>, do_parse!(
    num: number >>
    tag_no_case!(" RECENT") >>
    (Response::MailboxData(MailboxDatum::Recent(num)))
));

named!(mailbox_data<Response>, alt!(
    mailbox_data_flags |
    mailbox_data_exists |
    mailbox_data_list |
    mailbox_data_lsub |
    mailbox_data_status |
    mailbox_data_recent |
    mailbox_data_search
));

// An address structure is a parenthesized list that describes an
// electronic mail address.
named!(address<Address>, paren_delimited!(
    do_parse!(
        name: nstring >>
        tag!(" ") >>
        adl: nstring >>
        tag!(" ") >>
        mailbox: nstring >>
        tag!(" ") >>
        host: nstring >>
        (Address {
            name,
            adl,
            mailbox,
            host,
        })
    )
));

named!(opt_addresses<Option<Vec<Address>>>, alt!(
    map!(nil, |_s| None) |
    map!(paren_delimited!(
        many1!(do_parse!(
            addr: address >>
            opt!(char!(' ')) >>
            (addr)
        ))
    ), |v| Some(v))
));

named!(pub(crate) envelope<Envelope>, paren_delimited!(
    do_parse!(
        date: nstring >>
        tag!(" ") >>
        subject: nstring >>
        tag!(" ") >>
        from: opt_addresses >>
        tag!(" ") >>
        sender: opt_addresses >>
        tag!(" ") >>
        reply_to: opt_addresses >>
        tag!(" ") >>
        to: opt_addresses >>
        tag!(" ") >>
        cc: opt_addresses >>
        tag!(" ") >>
        bcc: opt_addresses >>
        tag!(" ") >>
        in_reply_to: nstring >>
        tag!(" ") >>
        message_id: nstring >>
        (Envelope {
            date,
            subject,
            from,
            sender,
            reply_to,
            to,
            cc,
            bcc,
            in_reply_to,
            message_id,
        })
    )
));

named!(msg_att_envelope<AttributeValue>, do_parse!(
    tag_no_case!("ENVELOPE ") >>
    envelope: envelope >>
    (AttributeValue::Envelope(Box::new(envelope)))
));

named!(msg_att_internal_date<AttributeValue>, do_parse!(
    tag_no_case!("INTERNALDATE ") >>
    date: nstring_utf8 >>
    (AttributeValue::InternalDate(date.unwrap()))
));

named!(msg_att_flags<AttributeValue>, do_parse!(
    tag_no_case!("FLAGS ") >>
    flags: flag_list >>
    (AttributeValue::Flags(flags))
));

named!(msg_att_rfc822<AttributeValue>, do_parse!(
    tag_no_case!("RFC822 ") >>
    raw: nstring >>
    (AttributeValue::Rfc822(raw))
));

named!(msg_att_rfc822_header<AttributeValue>, do_parse!(
    tag_no_case!("RFC822.HEADER ") >>
    opt!(tag!(" ")) >> // extra space workaround for DavMail
    raw: nstring >>
    (AttributeValue::Rfc822Header(raw))
));

named!(msg_att_rfc822_size<AttributeValue>, do_parse!(
    tag_no_case!("RFC822.SIZE ") >>
    num: number >>
    (AttributeValue::Rfc822Size(num))
));

named!(msg_att_rfc822_text<AttributeValue>, do_parse!(
    tag_no_case!("RFC822.TEXT ") >>
    raw: nstring >>
    (AttributeValue::Rfc822Text(raw))
));

named!(msg_att_uid<AttributeValue>, do_parse!(
    tag_no_case!("UID ") >>
    num: number >>
    (AttributeValue::Uid(num))
));

named!(msg_att<AttributeValue>, alt!(
    msg_att_body_section |
    msg_att_body_structure |
    msg_att_envelope |
    msg_att_internal_date |
    msg_att_flags |
    rfc4551::msg_att_mod_seq |
    msg_att_rfc822 |
    msg_att_rfc822_header |
    msg_att_rfc822_size |
    msg_att_rfc822_text |
    msg_att_uid
));

named!(msg_att_list<Vec<AttributeValue>>, parenthesized_nonempty_list!(msg_att));

named!(message_data_fetch<Response>, do_parse!(
    num: number >>
    tag_no_case!(" FETCH ") >>
    attrs: msg_att_list >>
    (Response::Fetch(num, attrs))
));

named!(message_data_expunge<Response>, do_parse!(
    num: number >>
    tag_no_case!(" EXPUNGE") >>
    (Response::Expunge(num))
));

named!(tag<RequestId>, map!(
    map_res!(take_while1!(tag_char), str::from_utf8),
    |s| RequestId(s.to_string())
));

// This is not quite according to spec, which mandates the following:
//     ["[" resp-text-code "]" SP] text
// However, examples in RFC 4551 (Conditional STORE) counteract this by giving
// examples of `resp-text` that do not include the trailing space and text.
named!(resp_text<(Option<ResponseCode>, Option<&str>)>, do_parse!(
    code: opt!(resp_text_code) >>
    text: text >>
    ({
        let res = if text.is_empty() {
            None
        } else if code.is_some() {
            Some(&text[1..])
        } else {
            Some(text)
        };
        (code, res)
    })
));

named!(continue_req<Response>, do_parse!(
    tag!("+") >>
    opt!(tag!(" ")) >> // Some servers do not send the space :/
    text: resp_text >> // TODO: base64
    tag!("\r\n") >>
    (Response::Continue {
        code: text.0,
        information: text.1,
    })
));

named!(response_tagged<Response>, do_parse!(
    tag: tag >>
    tag!(" ") >>
    status: status >>
    tag!(" ") >>
    text: resp_text >>
    tag!("\r\n") >>
    (Response::Done {
        tag,
        status,
        code: text.0,
        information: text.1,
    })
));

named!(resp_cond<Response>, do_parse!(
    status: status >>
    tag!(" ") >>
    text: resp_text >>
    (Response::Data {
        status,
        code: text.0,
        information: text.1,
    })
));

named!(response_data<Response>, do_parse!(
    tag!("* ") >>
    contents: alt!(
        resp_cond |
        mailbox_data |
        message_data_expunge |
        message_data_fetch |
        resp_capability |
        resp_metadata
    ) >>
    tag!("\r\n") >>
    (contents)
));

named!(response<Response>, alt!(
    continue_req |
    response_data |
    response_tagged
));

pub type ParseResult<'a> = IResult<&'a [u8], Response<'a>>;

pub fn parse_response(msg: &[u8]) -> ParseResult<'_> {
    response(msg)
}


#[cfg(test)]
mod tests {
    use nom;
    use super::parse_response;
    use crate::types::*;

    #[test]
    fn test_number_overflow() {
        match parse_response(b"* 2222222222222222222222222222222222222222222C\r\n") {
            Err(_) => {},
            _ => panic!("error required for integer overflow"),
        }
    }

    #[test]
    fn test_unseen() {
        match parse_response(b"* OK [UNSEEN 3] Message 3 is first unseen\r\n").unwrap() {
            (_, Response::Data {
                status: Status::Ok,
                code: Some(ResponseCode::Unseen(3)),
                information: Some("Message 3 is first unseen"),
            }) => {},
            rsp => panic!("unexpected response {:?}", rsp),
        }
    }

    #[test]
    fn test_body_text() {
        match parse_response(b"* 2 FETCH (BODY[TEXT] {3}\r\nfoo)\r\n") {
            Ok((_, Response::Fetch(_, attrs))) => {
                let body = &attrs[0];
                assert_eq!(body, &AttributeValue::BodySection {
                    section: Some(SectionPath::Full(MessageSection::Text)),
                    index: None,
                    data: Some(b"foo"),
                }, "body = {:?}", body);
            },
            rsp => panic!("unexpected response {:?}", rsp),
        }
    }

    #[test]
    fn test_body_structure() {
        const RESPONSE: &[u8] = b"* 15 FETCH (BODYSTRUCTURE (\"TEXT\" \"PLAIN\" (\"CHARSET\" \"iso-8859-1\") NIL NIL \"QUOTED-PRINTABLE\" 1315 42 NIL NIL NIL NIL))\r\n";
        match parse_response(RESPONSE) {
            Ok((_, Response::Fetch(_, attrs))) => {
                let body = &attrs[0];
                assert!(if let AttributeValue::BodyStructure(_) = *body { true } else { false }, "body = {:?}", body);
            },
            rsp => panic!("unexpected response {:?}", rsp),
        }
    }

    #[test]
    fn test_status() {
        match parse_response(b"* STATUS blurdybloop (MESSAGES 231 UIDNEXT 44292)\r\n") {
            Ok((_, Response::MailboxData(MailboxDatum::Status { mailbox, status }))) => {
                assert_eq!(mailbox, "blurdybloop");
                assert_eq!(status, [
                    StatusAttribute::Messages(231),
                    StatusAttribute::UidNext(44292),
                ]);
            },
            rsp => panic!("unexpected response {:?}", rsp),
        }
    }

    #[test]
    fn test_notify() {
        match parse_response(b"* 3501 EXPUNGE\r\n") {
            Ok((_, Response::Expunge(3501))) => {},
            rsp => panic!("unexpected response {:?}", rsp),
        }
        match parse_response(b"* 3501 EXISTS\r\n") {
            Ok((_, Response::MailboxData(MailboxDatum::Exists(3501)))) => {},
            rsp => panic!("unexpected response {:?}", rsp),
        }
        match parse_response(b"+ idling\r\n") {
            Ok((_, Response::Continue { code: None, information: Some("idling") })) => {},
            rsp => panic!("unexpected response {:?}", rsp),
        }
    }

    #[test]
    fn test_search() {
        match parse_response(b"* SEARCH\r\n") {
            Ok((_, Response::IDs(ids))) => {
                assert!(ids.is_empty());
            },
            rsp => panic!("unexpected response {:?}", rsp),
        }
        match parse_response(b"* SEARCH 12345 67890\r\n") {
            Ok((_, Response::IDs(ids))) => {
                assert_eq!(ids[0], 12345);
                assert_eq!(ids[1], 67890);
            },
            rsp => panic!("unexpected response {:?}", rsp),
        }
    }

    #[test]
    fn test_uid_fetch() {
        match parse_response(b"* 4 FETCH (UID 71372 RFC822.HEADER {10275}\r\n") {
            Err(nom::Err::Incomplete(nom::Needed::Size(10275))) => {},
            rsp => panic!("unexpected response {:?}", rsp),
        }
    }

    #[test]
    fn test_list() {
        match crate::parser::rfc3501::mailbox(b"iNboX ") {
            Ok((_, mb)) => {
                assert_eq!(mb, "INBOX");
            },
            rsp => panic!("unexpected response {:?}", rsp),
        }

        match parse_response(b"* LIST (\\HasNoChildren) \".\" INBOX.Tests\r\n") {
            Ok((_, Response::MailboxData(_))) => {},
            rsp => panic!("unexpected response {:?}", rsp),
        }
    }

    #[test]
    fn test_uid_fetch_extra_space() {
        // DavMail inserts an extra space after RFC822.HEADER
        match parse_response(b"* 4 FETCH (UID 71372 RFC822.HEADER  {10275}\r\n") {
            Err(nom::Err::Incomplete(nom::Needed::Size(10275))) => {},
            rsp => panic!("unexpected response {:?}", rsp),
        }
    }

    #[test]
    fn test_envelope() {
        let env = br#"ENVELOPE ("Wed, 17 Jul 1996 02:23:25 -0700 (PDT)" "IMAP4rev1 WG mtg summary and minutes" (("Terry Gray" NIL "gray" "cac.washington.edu")) (("Terry Gray" NIL "gray" "cac.washington.edu")) (("Terry Gray" NIL "gray" "cac.washington.edu")) ((NIL NIL "imap" "cac.washington.edu")) ((NIL NIL "minutes" "CNRI.Reston.VA.US") ("John Klensin" NIL "KLENSIN" "MIT.EDU")) NIL NIL "<B27397-0100000@cac.washington.edu>") "#;
        match crate::parser::rfc3501::msg_att_envelope(env) {
            Ok((_, AttributeValue::Envelope(_))) => {},
            rsp => panic!("unexpected response {:?}", rsp)
        }
    }

    #[test]
    fn test_header_fields() {
        const RESPONSE: &[u8] = b"* 1 FETCH (UID 1 BODY[HEADER.FIELDS (CHAT-VERSION)] {21}\r\nChat-Version: 1.0\r\n\r\n)\r\n";

        match parse_response(RESPONSE) {
            Ok((_, Response::Fetch(_, _))) => {},
            rsp => panic!("unexpected response {:?}", rsp),
        }
    }

    #[test]
    fn test_opt_addresses() {
        let addr = b"((NIL NIL \"minutes\" \"CNRI.Reston.VA.US\") (\"John Klensin\" NIL \"KLENSIN\" \"MIT.EDU\")) ";
            match crate::parser::rfc3501::opt_addresses(addr) {
            Ok((_, _addresses)) => {},
            rsp => panic!("unexpected response {:?}", rsp)
        }
    }

    #[test]
    fn test_opt_addresses_no_space() {
        let addr = br#"((NIL NIL "test" "example@example.com")(NIL NIL "test" "example@example.com"))"#;
            match super::opt_addresses(addr) {
            Ok((_, _addresses)) => {},
            rsp => panic!("unexpected response {:?}", rsp)
        }
    }

    #[test]
    fn test_addresses() {
        match crate::parser::rfc3501::address(b"(\"John Klensin\" NIL \"KLENSIN\" \"MIT.EDU\") ") {
            Ok((_, _address)) => {},
            rsp => panic!("unexpected response {:?}", rsp)
        }

        // Literal non-UTF8 address
        match crate::parser::rfc3501::address(b"({12}\r\nJoh\xff Klensin NIL \"KLENSIN\" \"MIT.EDU\") ") {
            Ok((_, _address)) => {},
            rsp => panic!("unexpected response {:?}", rsp)
        }
    }

    #[test]
    fn test_response_codes() {
        match parse_response(b"* OK [ALERT] Alert!\r\n") {
            Ok((_, Response::Data { status: Status::Ok, code: Some(ResponseCode::Alert), information: Some("Alert!") })) => {}
            rsp => panic!("unexpected response {:?}", rsp)
        }

        match parse_response(b"* NO [PARSE] Something\r\n") {
            Ok((_, Response::Data { status: Status::No, code: Some(ResponseCode::Parse), information: Some("Something") })) => {}
            rsp => panic!("unexpected response {:?}", rsp)
        }

        match parse_response(b"* OK [CAPABILITY IMAP4rev1 IDLE] Logged in\r\n") {
            Ok((_, Response::Data {
                status: Status::Ok,
                code: Some(ResponseCode::Capabilities(c)),
                information: Some("Logged in")
            })) => {
                assert_eq!(c.len(), 2);
                assert_eq!(c[0], Capability::Imap4rev1);
                assert_eq!(c[1], Capability::Atom("IDLE"));
            }
            rsp => panic!("unexpected response {:?}", rsp)
        }

        match parse_response(b"* OK [CAPABILITY UIDPLUS IMAP4rev1 IDLE] Logged in\r\n") {
            Ok((_, Response::Data {
                status: Status::Ok,
                code: Some(ResponseCode::Capabilities(c)),
                information: Some("Logged in")
            })) => {
                assert_eq!(c.len(), 3);
                assert_eq!(c[0], Capability::Atom("UIDPLUS"));
                assert_eq!(c[1], Capability::Imap4rev1);
                assert_eq!(c[2], Capability::Atom("IDLE"));
            }
            rsp => panic!("unexpected response {:?}", rsp)
        }

        // Missing IMAP4rev1
        match parse_response(b"* OK [CAPABILITY UIDPLUS IDLE] Logged in\r\n") {
            Ok((_, Response::Data {
                status: Status::Ok,
                code: None,
                information: Some("[CAPABILITY UIDPLUS IDLE] Logged in")
            })) => {}
            rsp => panic!("unexpected response {:?}", rsp)
        }

        match parse_response(b"* NO [BADCHARSET] error\r\n") {
            Ok((_, Response::Data {
                status: Status::No,
                code: Some(ResponseCode::BadCharset(None)),
                information: Some("error")
            })) => {},
            rsp => panic!("unexpected response {:?}", rsp)
        }

        match parse_response(b"* NO [BADCHARSET (utf-8 latin1)] error\r\n") {
            Ok((_, Response::Data {
                status: Status::No,
                code: Some(ResponseCode::BadCharset(Some(v))),
                information: Some("error")
            })) => {
                assert_eq!(v.len(), 2);
                assert_eq!(v[0], "utf-8");
                assert_eq!(v[1], "latin1");
            },
            rsp => panic!("unexpected response {:?}", rsp)
        }

        match parse_response(b"* NO [BADCHARSET ()] error\r\n") {
            Ok((_, Response::Data {
                status: Status::No,
                code: None,
                information: Some("[BADCHARSET ()] error")
            })) => {}
            rsp => panic!("unexpected response {:?}", rsp)
        }
    }

    #[test]
    fn test_capability_data() {
        // Minimal capabilities
        assert_matches!(
            super::capability_data(b"CAPABILITY IMAP4rev1\r\n"),
            Ok((_, capabilities)) => {
                assert_eq!(capabilities, vec![Capability::Imap4rev1])
            }
        );

        assert_matches!(
            super::capability_data(b"CAPABILITY IMAP4REV1\r\n"),
            Ok((_, capabilities)) => {
                assert_eq!(capabilities, vec![Capability::Imap4rev1])
            }
        );

        assert_matches!(
            super::capability_data(b"CAPABILITY XPIG-LATIN IMAP4rev1 STARTTLS AUTH=GSSAPI\r\n"),
            Ok((_, capabilities)) => {
                assert_eq!(capabilities, vec![
                    Capability::Atom("XPIG-LATIN"), Capability::Imap4rev1,
                    Capability::Atom("STARTTLS"), Capability::Auth("GSSAPI")
                ])
            }
        );

        assert_matches!(
            super::capability_data(b"CAPABILITY IMAP4rev1 AUTH=GSSAPI AUTH=PLAIN\r\n"),
            Ok((_, capabilities)) => {
                assert_eq!(capabilities, vec![
                    Capability::Imap4rev1, Capability::Auth("GSSAPI"),  Capability::Auth("PLAIN")
                ])
            }
        );

        // Capability command must contain IMAP4rev1
        assert_matches!(
            super::capability_data(b"CAPABILITY AUTH=GSSAPI AUTH=PLAIN\r\n"),
            Err(_)
        );
    }

    #[test]
    fn test_incomplete_fetch() {
        match parse_response(b"* 4644 FETCH (UID ") {
            Err(nom::Err::Incomplete(_)) => {},
            rsp => panic!("should be incomplete: {:?}", rsp),
        }
    }

    #[test]
    fn test_continuation() {
        // regular RFC compliant
        match parse_response(b"+ \r\n") {
            Ok((_, Response::Continue {
                code: None,
                information: None,
            })) => {}
            rsp => panic!("unexpected response {:?}", rsp)
        }

        // short version, sent by yandex
        match parse_response(b"+\r\n") {
            Ok((_, Response::Continue {
                code: None,
                information: None,
            })) => {}
            rsp => panic!("unexpected response {:?}", rsp)
        }
    }
}
