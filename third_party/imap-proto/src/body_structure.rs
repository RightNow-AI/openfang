// rustfmt doesn't do a very good job on nom parser invocations.
#![cfg_attr(rustfmt, rustfmt_skip)]

use crate::core::*;
use crate::types::*;

use crate::parser::envelope;

struct BodyFields<'a> {
    pub param: BodyParams<'a>,
    pub id: Option<&'a str>,
    pub description: Option<&'a str>,
    pub transfer_encoding: ContentEncoding<'a>,
    pub octets: u32,
}

// body-fields     = body-fld-param SP body-fld-id SP body-fld-desc SP
//                   body-fld-enc SP body-fld-octets
named!(body_fields<BodyFields>, do_parse!(
    param: body_param >>
    tag!(" ") >>
    // body id seems to refer to the Message-ID or possibly Content-ID header, which
    // by the definition in RFC 2822 seems to resolve to all ASCII characters (through
    // a large amount of indirection which I did not have the patience to fully explore)
    id: nstring_utf8 >>
    tag!(" ") >>
    // Per https://tools.ietf.org/html/rfc2045#section-8, description should be all ASCII
    description: nstring_utf8 >>
    tag!(" ") >>
    transfer_encoding: body_encoding >>
    tag!(" ") >>
    octets: number >>
    (BodyFields { param, id, description, transfer_encoding, octets })
));

struct BodyExt1Part<'a> {
    pub md5: Option<&'a str>,
    pub disposition: Option<ContentDisposition<'a>>,
    pub language: Option<Vec<&'a str>>,
    pub location: Option<&'a str>,
    pub extension: Option<BodyExtension<'a>>,
}

// body-ext-1part  = body-fld-md5 [SP body-fld-dsp [SP body-fld-lang
//                   [SP body-fld-loc *(SP body-extension)]]]
//                     ; MUST NOT be returned on non-extensible
//                     ; "BODY" fetch
named!(body_ext_1part<BodyExt1Part>, do_parse!(
    // Per RFC 1864, MD5 values are base64-encoded
    md5: opt_opt!(preceded!(tag!(" "), nstring_utf8)) >>
    disposition: opt_opt!(preceded!(tag!(" "), body_disposition)) >>
    language: opt_opt!(preceded!(tag!(" "), body_lang)) >>
    // Location appears to reference a URL, which by RFC 1738 (section 2.2) should be ASCII
    location: opt_opt!(preceded!(tag!(" "), nstring_utf8)) >>
    extension: opt!(preceded!(tag!(" "), body_extension)) >>
    (BodyExt1Part { md5, disposition, language, location, extension })
));

struct BodyExtMPart<'a> {
    pub param: BodyParams<'a>,
    pub disposition: Option<ContentDisposition<'a>>,
    pub language: Option<Vec<&'a str>>,
    pub location: Option<&'a str>,
    pub extension: Option<BodyExtension<'a>>,
}

// body-ext-mpart  = body-fld-param [SP body-fld-dsp [SP body-fld-lang
//                   [SP body-fld-loc *(SP body-extension)]]]
//                     ; MUST NOT be returned on non-extensible
//                     ; "BODY" fetch
named!(body_ext_mpart<BodyExtMPart>, do_parse!(
    param: opt_opt!(preceded!(tag!(" "), body_param)) >>
    disposition: opt_opt!(preceded!(tag!(" "), body_disposition)) >>
    language: opt_opt!(preceded!(tag!(" "), body_lang)) >>
    // Location appears to reference a URL, which by RFC 1738 (section 2.2) should be ASCII
    location: opt_opt!(preceded!(tag!(" "), nstring_utf8)) >>
    extension: opt!(preceded!(tag!(" "), body_extension)) >>
    (BodyExtMPart { param, disposition, language, location, extension })
));

named!(body_encoding<ContentEncoding>, alt!(
    delimited!(char!('"'), alt!(
        map!(tag_no_case!("7BIT"), |_| ContentEncoding::SevenBit) |
        map!(tag_no_case!("8BIT"), |_| ContentEncoding::EightBit) |
        map!(tag_no_case!("BINARY"), |_| ContentEncoding::Binary) |
        map!(tag_no_case!("BASE64"), |_| ContentEncoding::Base64) |
        map!(tag_no_case!("QUOTED-PRINTABLE"), |_| ContentEncoding::QuotedPrintable)
    ), char!('"')) |
    map!(string_utf8, |enc| ContentEncoding::Other(enc))
));

named!(body_lang<Option<Vec<&str>>>, alt!(
    // body language seems to refer to RFC 3066 language tags, which should be ASCII-only
    map!(nstring_utf8, |v| v.map(|s| vec![s])) |
    map!(parenthesized_nonempty_list!(string_utf8), Option::from)
));

named!(body_param<BodyParams>, alt!(
    map!(nil, |_| None) |
    map!(parenthesized_nonempty_list!(do_parse!(
        key: string_utf8 >>
        tag!(" ") >>
        val: string_utf8 >>
        ((key, val))
    )), Option::from)
));

named!(body_extension<BodyExtension>, alt!(
    map!(number, |n| BodyExtension::Num(n)) |
    // Cannot find documentation on character encoding for body extension values.
    // So far, assuming UTF-8 seems fine, please report if you run into issues here.
    map!(nstring_utf8, |s| BodyExtension::Str(s)) |
    map!(parenthesized_nonempty_list!(body_extension), |ext| BodyExtension::List(ext))
));

named!(body_disposition<Option<ContentDisposition>>, alt!(
    map!(nil, |_| None) |
    paren_delimited!(do_parse!(
        ty: string_utf8 >>
        tag!(" ") >>
        params: body_param >>
        (Some(ContentDisposition {
            ty,
            params
        }))
    ))
));

named!(body_type_basic<BodyStructure>, do_parse!(
    media_type: string_utf8 >>
    tag!(" ") >>
    media_subtype: string_utf8 >>
    tag!(" ") >>
    fields: body_fields >>
    ext: body_ext_1part >>
    (BodyStructure::Basic {
        common: BodyContentCommon {
            ty: ContentType {
                ty: media_type,
                subtype: media_subtype,
                params: fields.param,
            },
            disposition: ext.disposition,
            language: ext.language,
            location: ext.location,
        },
        other: BodyContentSinglePart {
            id: fields.id,
            md5: ext.md5,
            octets: fields.octets,
            description: fields.description,
            transfer_encoding: fields.transfer_encoding,
        },
        extension: ext.extension,
    })
));

named!(body_type_text<BodyStructure>, do_parse!(
    tag_no_case!("\"TEXT\"") >>
    tag!(" ") >>
    media_subtype: string_utf8 >>
    tag!(" ") >>
    fields: body_fields >>
    tag!(" ") >>
    lines: number >>
    ext: body_ext_1part >>
    (BodyStructure::Text {
        common: BodyContentCommon {
            ty: ContentType {
                ty: "TEXT",
                subtype: media_subtype,
                params: fields.param,
            },
            disposition: ext.disposition,
            language: ext.language,
            location: ext.location,
        },
        other: BodyContentSinglePart {
            id: fields.id,
            md5: ext.md5,
            octets: fields.octets,
            description: fields.description,
            transfer_encoding: fields.transfer_encoding,
        },
        lines,
        extension: ext.extension,
    })
));

named!(body_type_message<BodyStructure>, do_parse!(
    tag_no_case!("\"MESSAGE\" \"RFC822\"") >>
    tag!(" ") >>
    fields: body_fields >>
    tag!(" ") >>
    envelope: envelope >>
    tag!(" ") >>
    body: body >>
    tag!(" ") >>
    lines: number >>
    ext: body_ext_1part >>
    (BodyStructure::Message {
        common: BodyContentCommon {
            ty: ContentType {
                ty: "MESSAGE",
                subtype: "RFC822",
                params: fields.param,
            },
            disposition: ext.disposition,
            language: ext.language,
            location: ext.location,
        },
        other: BodyContentSinglePart {
            id: fields.id,
            md5: ext.md5,
            octets: fields.octets,
            description: fields.description,
            transfer_encoding: fields.transfer_encoding,
        },
        envelope,
        body: Box::new(body),
        lines,
        extension: ext.extension,
    })
));

named!(body_type_multipart<BodyStructure>, do_parse!(
    bodies: many1!(body) >>
    tag!(" ") >>
    media_subtype: string_utf8 >>
    ext: body_ext_mpart >>
    (BodyStructure::Multipart {
        common: BodyContentCommon {
            ty: ContentType {
                ty: "MULTIPART",
                subtype: media_subtype,
                params: ext.param,
            },
            disposition: ext.disposition,
            language: ext.language,
            location: ext.location,
        },
        bodies,
        extension: ext.extension,
    })
));

named!(pub(crate) body<BodyStructure>, paren_delimited!(
    alt!(body_type_text | body_type_message | body_type_basic | body_type_multipart)
));

named!(pub(crate) msg_att_body_structure<AttributeValue>, do_parse!(
    tag_no_case!("BODYSTRUCTURE ") >>
    body: body >>
    (AttributeValue::BodyStructure(body))
));

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY: &[u8] = &[];

    // body-fld-param SP body-fld-id SP body-fld-desc SP body-fld-enc SP body-fld-octets
    const BODY_FIELDS: &str = r#"("foo" "bar") "id" "desc" "7BIT" 1337"#;
    const BODY_FIELD_PARAM_PAIR: (&str, &str) = ("foo", "bar");
    const BODY_FIELD_ID: Option<&str> = Some("id");
    const BODY_FIELD_DESC: Option<&str> = Some("desc");
    const BODY_FIELD_ENC: ContentEncoding = ContentEncoding::SevenBit;
    const BODY_FIELD_OCTETS: u32 = 1337;

    fn mock_body_text() -> (String, BodyStructure<'static>) {
        (
            format!(r#"("TEXT" "PLAIN" {} 42)"#, BODY_FIELDS),
            BodyStructure::Text {
                common: BodyContentCommon {
                    ty: ContentType {
                        ty: "TEXT",
                        subtype: "PLAIN",
                        params: Some(vec![BODY_FIELD_PARAM_PAIR]),
                    },
                    disposition: None,
                    language: None,
                    location: None,
                },
                other: BodyContentSinglePart {
                    md5: None,
                    transfer_encoding: BODY_FIELD_ENC,
                    octets: BODY_FIELD_OCTETS,
                    id: BODY_FIELD_ID,
                    description: BODY_FIELD_DESC,
                },
                lines: 42,
                extension: None,
            }
        )
    }

    #[test]
    fn test_body_param_data() {
        assert_matches!(
            body_param(br#"NIL"#),
            Ok((EMPTY, None))
        );

        assert_matches!(
            body_param(br#"("foo" "bar")"#),
            Ok((EMPTY, Some(param))) => {
                assert_eq!(param, vec![("foo", "bar")]);
            }
        );
    }

    #[test]
    fn test_body_lang_data() {
        assert_matches!(
            body_lang(br#""bob""#),
            Ok((EMPTY, Some(langs))) => {
                assert_eq!(langs, vec!["bob"]);
            }
        );

        assert_matches!(
            body_lang(br#"("one" "two")"#),
            Ok((EMPTY, Some(langs))) => {
                assert_eq!(langs, vec!["one", "two"]);
            }
        );

        assert_matches!(
            body_lang(br#"NIL"#),
            Ok((EMPTY, None))
        );
    }

    #[test]
    fn test_body_extension_data() {
        assert_matches!(
            body_extension(br#""blah""#),
            Ok((EMPTY, BodyExtension::Str(Some("blah"))))
        );

        assert_matches!(
            body_extension(br#"NIL"#),
            Ok((EMPTY, BodyExtension::Str(None)))
        );

        assert_matches!(
            body_extension(br#"("hello")"#),
            Ok((EMPTY, BodyExtension::List(list))) => {
                assert_eq!(list, vec![BodyExtension::Str(Some("hello"))]);
            }
        );

        assert_matches!(
            body_extension(br#"(1337)"#),
            Ok((EMPTY, BodyExtension::List(list))) => {
                assert_eq!(list, vec![BodyExtension::Num(1337)]);
            }
        );
    }

    #[test]
    fn test_body_disposition_data() {
        assert_matches!(
            body_disposition(br#"NIL"#),
            Ok((EMPTY, None))
        );

        assert_matches!(
            body_disposition(br#"("attachment" ("FILENAME" "pages.pdf"))"#),
            Ok((EMPTY, Some(disposition))) => {
                assert_eq!(disposition, ContentDisposition {
                    ty: "attachment",
                    params: Some(vec![
                        ("FILENAME", "pages.pdf")
                    ])
                });
            }
        );
    }

    #[test]
    fn test_body_structure_text() {
        let (body_str, body_struct) = mock_body_text();

        assert_matches!(
            body(body_str.as_bytes()),
            Ok((_, text)) => {
                assert_eq!(text, body_struct);
            }
        );
    }

    #[test]
    fn test_body_structure_text_with_ext() {
        let body_str = format!(r#"("TEXT" "PLAIN" {} 42 NIL NIL NIL NIL)"#, BODY_FIELDS);
        let (_, text_body_struct) = mock_body_text();

        assert_matches!(
            body(body_str.as_bytes()),
            Ok((_, text)) => {
                assert_eq!(text, text_body_struct)
            }
        );
    }

    #[test]
    fn test_body_structure_basic() {
        const BODY: &[u8] = br#"("APPLICATION" "PDF" ("NAME" "pages.pdf") NIL NIL "BASE64" 38838 NIL ("attachment" ("FILENAME" "pages.pdf")) NIL NIL)"#;

        assert_matches!(
            body(BODY),
            Ok((_, basic)) => {
                assert_eq!(basic, BodyStructure::Basic {
                    common: BodyContentCommon {
                        ty: ContentType {
                            ty: "APPLICATION",
                            subtype: "PDF",
                            params: Some(vec![("NAME", "pages.pdf")])
                        },
                        disposition: Some(ContentDisposition {
                            ty: "attachment",
                            params: Some(vec![("FILENAME", "pages.pdf")])
                        }),
                        language: None,
                        location: None,
                    },
                    other: BodyContentSinglePart {
                        transfer_encoding: ContentEncoding::Base64,
                        octets: 38838,
                        id: None,
                        md5: None,
                        description: None,
                    },
                    extension: None,
                })
            }
        );
    }

    #[test]
    fn test_body_structure_message() {
        let (text_body_str, _) = mock_body_text();
        let envelope_str = r#"("Wed, 17 Jul 1996 02:23:25 -0700 (PDT)" "IMAP4rev1 WG mtg summary and minutes" (("Terry Gray" NIL "gray" "cac.washington.edu")) (("Terry Gray" NIL "gray" "cac.washington.edu")) (("Terry Gray" NIL "gray" "cac.washington.edu")) ((NIL NIL "imap" "cac.washington.edu")) ((NIL NIL "minutes" "CNRI.Reston.VA.US") ("John Klensin" NIL "KLENSIN" "MIT.EDU")) NIL NIL "<B27397-0100000@cac.washington.edu>")"#;
        let body_str = format!(r#"("MESSAGE" "RFC822" {} {} {} 42)"#, BODY_FIELDS, envelope_str, text_body_str);

        assert_matches!(
            body(body_str.as_bytes()),
            Ok((_, BodyStructure::Message { .. }))
        );
    }

    #[test]
    fn test_body_structure_multipart() {
        let (text_body_str1, text_body_struct1) = mock_body_text();
        let (text_body_str2, text_body_struct2) = mock_body_text();
        let body_str = format!(
            r#"({}{} "ALTERNATIVE" NIL NIL NIL NIL)"#,
            text_body_str1, text_body_str2
        );

        assert_matches!(
            body(body_str.as_bytes()),
            Ok((_, multipart)) => {
                assert_eq!(multipart, BodyStructure::Multipart {
                    common: BodyContentCommon {
                        ty: ContentType {
                            ty: "MULTIPART",
                            subtype: "ALTERNATIVE",
                            params: None
                        },
                        language: None,
                        location: None,
                        disposition: None,
                    },
                    bodies: vec![
                        text_body_struct1,
                        text_body_struct2,
                    ],
                    extension: None
                });
            }
        );
    }
}
