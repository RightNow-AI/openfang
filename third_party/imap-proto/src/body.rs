use crate::core::*;
use crate::types::*;

named!(pub section_part<Vec<u32>>, do_parse!(
    part: number >>
    rest: many0!(do_parse!(
        tag!(".") >>
        part: number >>
        (part)
    ))  >> ({
        let mut res = vec![part];
        res.extend(rest);
        res
    })
));

named!(pub section_msgtext<MessageSection>, alt!(
    do_parse!(
        tag_no_case!("HEADER.FIELDS") >>
        opt!(tag_no_case!(".NOT")) >>
        tag!(" ") >>
        parenthesized_list!(astring) >>
        (MessageSection::Header)) |
    do_parse!(tag_no_case!("HEADER") >> (MessageSection::Header)) |
    do_parse!(tag_no_case!("TEXT") >> (MessageSection::Text))
));

named!(pub section_text<MessageSection>, alt!(
    section_msgtext |
    do_parse!(tag_no_case!("MIME") >> (MessageSection::Mime))
));

named!(pub section_spec<SectionPath>, alt!(
    map!(section_msgtext, |val| SectionPath::Full(val)) |
    do_parse!(
        part: section_part >>
        text: opt!(do_parse!(
            tag!(".") >>
            text: section_text >>
            (text)
        )) >>
        (SectionPath::Part(part, text))
    )
));

named!(pub section<Option<SectionPath>>, do_parse!(
    tag!("[") >>
    spec: opt!(section_spec) >>
    tag!("]") >>
    (spec)
));

named!(pub msg_att_body_section<AttributeValue>, do_parse!(
    tag_no_case!("BODY") >>
    section: section >>
    index: opt!(do_parse!(
        tag!("<") >>
        num: number >>
        tag!(">") >>
        (num)
    )) >>
    tag!(" ") >>
    data: nstring >>
    (AttributeValue::BodySection { section, index, data })
));
