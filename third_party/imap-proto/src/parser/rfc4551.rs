//!
//! https://tools.ietf.org/html/rfc4551
//!
//! IMAP Extension for Conditional STORE Operation
//! or Quick Flag Changes Resynchronization
//!

// rustfmt doesn't do a very good job on nom parser invocations.
#![cfg_attr(rustfmt, rustfmt_skip)]

use crate::core::number_64;
use crate::types::*;

// The highest mod-sequence value of all messages in the mailbox.
// Extends resp-test-code defined in rfc3501.
// [RFC4551 - 3.6 HIGHESTMODSEQ Status Data Items](https://tools.ietf.org/html/rfc4551#section-3.6)
// [RFC4551 - 4. Formal Syntax - resp-text-code](https://tools.ietf.org/html/rfc4551#section-4)
named!(pub (crate) resp_text_code_highest_mod_seq<ResponseCode>, do_parse!(
    tag_no_case!("HIGHESTMODSEQ ") >>
    num: number_64 >>
    (ResponseCode::HighestModSeq(num))
));

// Extends status-att/status-att-list defined in rfc3501
// [RFC4551 - 3.6 - HIGHESTMODSEQ Status Data Items](https://tools.ietf.org/html/rfc4551#section-3.6)
// [RFC4551 - 4. Formal Syntax - status-att-val](https://tools.ietf.org/html/rfc4551#section-4)
named!(pub (crate) status_att_val_highest_mod_seq<StatusAttribute>, do_parse!(
    tag_no_case!("HIGHESTMODSEQ ") >>
    mod_sequence_valzer: number_64 >>
    (StatusAttribute::HighestModSeq(mod_sequence_valzer))
));

// [RFC4551 - 4. Formal Syntax - fetch-mod-resp](https://tools.ietf.org/html/rfc4551#section-4)
named!(pub (crate) msg_att_mod_seq<AttributeValue>, do_parse!(
    tag_no_case!("MODSEQ ") >>
    num: paren_delimited!(number_64) >>
    (AttributeValue::ModSeq(num))
));
