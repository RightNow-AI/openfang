#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request(pub RequestId, pub Vec<u8>);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AttrMacro {
    All,
    Fast,
    Full,
}

impl Copy for AttrMacro {}

#[derive(Debug, Eq, PartialEq)]
pub enum Response<'a> {
    Capabilities(Vec<Capability<'a>>),
    Continue {
        code: Option<ResponseCode<'a>>,
        information: Option<&'a str>,
    },
    Done {
        tag: RequestId,
        status: Status,
        code: Option<ResponseCode<'a>>,
        information: Option<&'a str>,
    },
    Data {
        status: Status,
        code: Option<ResponseCode<'a>>,
        information: Option<&'a str>,
    },
    Expunge(u32),
    Fetch(u32, Vec<AttributeValue<'a>>),
    MailboxData(MailboxDatum<'a>),
    IDs(Vec<u32>),
}

#[derive(Debug, Eq, PartialEq)]
pub enum Status {
    Ok,
    No,
    Bad,
    PreAuth,
    Bye,
}

#[derive(Debug, Eq, PartialEq)]
pub enum ResponseCode<'a> {
    Alert,
    BadCharset(Option<Vec<&'a str>>),
    Capabilities(Vec<Capability<'a>>),
    HighestModSeq(u64), // RFC 4551, section 3.1.1
    Parse,
    PermanentFlags(Vec<&'a str>),
    ReadOnly,
    ReadWrite,
    TryCreate,
    UidNext(u32),
    UidValidity(u32),
    Unseen(u32),
}

#[derive(Debug, Eq, PartialEq)]
pub enum StatusAttribute {
    HighestModSeq(u64), // RFC 4551
    Messages(u32),
    Recent(u32),
    UidNext(u32),
    UidValidity(u32),
    Unseen(u32),
}

#[derive(Debug, Eq, PartialEq)]
pub struct Metadata {
    pub entry: String,
    pub value: Option<String>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum MailboxDatum<'a> {
    Exists(u32),
    Flags(Vec<&'a str>),
    List {
        flags: Vec<&'a str>,
        delimiter: Option<&'a str>,
        name: &'a str,
    },
    Status {
        mailbox: &'a str,
        status: Vec<StatusAttribute>,
    },
    Recent(u32),
    MetadataSolicited {
        mailbox: &'a str,
        values: Vec<Metadata>,
    },
    MetadataUnsolicited {
        mailbox: &'a str,
        values: Vec<&'a str>,
    },
}

#[derive(Debug, Eq, PartialEq, Hash)]
pub enum Capability<'a> {
    Imap4rev1,
    Auth(&'a str),
    Atom(&'a str),
}

#[derive(Debug, Eq, PartialEq)]
pub enum Attribute {
    Body,
    Envelope,
    Flags,
    InternalDate,
    ModSeq, // RFC 4551, section 3.3.2
    Rfc822,
    Rfc822Size,
    Rfc822Text,
    Uid,
}

#[derive(Debug, Eq, PartialEq)]
pub enum MessageSection {
    Header,
    Mime,
    Text,
}

#[derive(Debug, Eq, PartialEq)]
pub enum SectionPath {
    Full(MessageSection),
    Part(Vec<u32>, Option<MessageSection>),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Eq, PartialEq)]
pub enum AttributeValue<'a> {
    BodySection {
        section: Option<SectionPath>,
        index: Option<u32>,
        data: Option<&'a [u8]>,
    },
    BodyStructure(BodyStructure<'a>),
    Envelope(Box<Envelope<'a>>),
    Flags(Vec<&'a str>),
    InternalDate(&'a str),
    ModSeq(u64), // RFC 4551, section 3.3.2
    Rfc822(Option<&'a [u8]>),
    Rfc822Header(Option<&'a [u8]>),
    Rfc822Size(u32),
    Rfc822Text(Option<&'a [u8]>),
    Uid(u32),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Eq, PartialEq)]
pub enum BodyStructure<'a> {
    Basic {
        common: BodyContentCommon<'a>,
        other: BodyContentSinglePart<'a>,
        extension: Option<BodyExtension<'a>>,
    },
    Text {
        common: BodyContentCommon<'a>,
        other: BodyContentSinglePart<'a>,
        lines: u32,
        extension: Option<BodyExtension<'a>>,
    },
    Message {
        common: BodyContentCommon<'a>,
        other: BodyContentSinglePart<'a>,
        envelope: Envelope<'a>,
        body: Box<BodyStructure<'a>>,
        lines: u32,
        extension: Option<BodyExtension<'a>>,
    },
    Multipart {
        common: BodyContentCommon<'a>,
        bodies: Vec<BodyStructure<'a>>,
        extension: Option<BodyExtension<'a>>,
    },
}

#[derive(Debug, Eq, PartialEq)]
pub struct BodyContentCommon<'a> {
    pub ty: ContentType<'a>,
    pub disposition: Option<ContentDisposition<'a>>,
    pub language: Option<Vec<&'a str>>,
    pub location: Option<&'a str>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct BodyContentSinglePart<'a> {
    pub id: Option<&'a str>,
    pub md5: Option<&'a str>,
    pub description: Option<&'a str>,
    pub transfer_encoding: ContentEncoding<'a>,
    pub octets: u32,
}

#[derive(Debug, Eq, PartialEq)]
pub struct ContentType<'a> {
    pub ty: &'a str,
    pub subtype: &'a str,
    pub params: BodyParams<'a>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct ContentDisposition<'a> {
    pub ty: &'a str,
    pub params: BodyParams<'a>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum ContentEncoding<'a> {
    SevenBit,
    EightBit,
    Binary,
    Base64,
    QuotedPrintable,
    Other(&'a str),
}

#[derive(Debug, Eq, PartialEq)]
pub enum BodyExtension<'a> {
    Num(u32),
    Str(Option<&'a str>),
    List(Vec<BodyExtension<'a>>),
}

pub type BodyParams<'a> = Option<Vec<(&'a str, &'a str)>>;

#[derive(Debug, Eq, PartialEq)]
pub struct Envelope<'a> {
    pub date: Option<&'a [u8]>,
    pub subject: Option<&'a [u8]>,
    pub from: Option<Vec<Address<'a>>>,
    pub sender: Option<Vec<Address<'a>>>,
    pub reply_to: Option<Vec<Address<'a>>>,
    pub to: Option<Vec<Address<'a>>>,
    pub cc: Option<Vec<Address<'a>>>,
    pub bcc: Option<Vec<Address<'a>>>,
    pub in_reply_to: Option<&'a [u8]>,
    pub message_id: Option<&'a [u8]>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Address<'a> {
    pub name: Option<&'a [u8]>,
    pub adl: Option<&'a [u8]>,
    pub mailbox: Option<&'a [u8]>,
    pub host: Option<&'a [u8]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestId(pub String);

impl RequestId {
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum State {
    NotAuthenticated,
    Authenticated,
    Selected,
    Logout,
}
