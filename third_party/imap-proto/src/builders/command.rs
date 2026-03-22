use crate::quoted_string;
use crate::types::{AttrMacro, Attribute, State};

pub struct CommandBuilder {}

impl CommandBuilder {
    pub fn check() -> Command {
        let args = b"CHECK".to_vec();
        Command {
            args,
            next_state: None,
        }
    }

    pub fn close() -> Command {
        let args = b"CLOSE".to_vec();
        Command {
            args,
            next_state: Some(State::Authenticated),
        }
    }

    pub fn examine(mailbox: &str) -> Command {
        let args = format!("EXAMINE \"{}\"", quoted_string(mailbox).unwrap()).into_bytes();
        Command {
            args,
            next_state: Some(State::Selected),
        }
    }

    pub fn fetch() -> FetchCommandEmpty {
        let args = b"FETCH ".to_vec();
        FetchCommandEmpty { args }
    }

    pub fn list(reference: &str, glob: &str) -> Command {
        let args = format!(
            "LIST \"{}\" \"{}\"",
            quoted_string(reference).unwrap(),
            quoted_string(glob).unwrap()
        )
        .into_bytes();
        Command {
            args,
            next_state: None,
        }
    }

    pub fn login(user_name: &str, password: &str) -> Command {
        let args = format!(
            "LOGIN \"{}\" \"{}\"",
            quoted_string(user_name).unwrap(),
            quoted_string(password).unwrap()
        )
        .into_bytes();
        Command {
            args,
            next_state: Some(State::Authenticated),
        }
    }

    pub fn select(mailbox: &str) -> Command {
        let args = format!("SELECT \"{}\"", quoted_string(mailbox).unwrap()).into_bytes();
        Command {
            args,
            next_state: Some(State::Selected),
        }
    }

    pub fn uid_fetch() -> FetchCommandEmpty {
        let args = b"UID FETCH ".to_vec();
        FetchCommandEmpty { args }
    }
}

pub struct Command {
    args: Vec<u8>,
    next_state: Option<State>,
}

impl Command {
    pub fn into_parts(self) -> (Vec<u8>, Option<State>) {
        let Command { args, next_state } = self;
        (args, next_state)
    }
}

pub struct FetchCommandEmpty {
    args: Vec<u8>,
}

impl FetchBuilderMessages for FetchCommandEmpty {
    fn prepare(self) -> FetchCommandMessages {
        FetchCommandMessages { args: self.args }
    }
}

pub struct FetchCommandMessages {
    args: Vec<u8>,
}

impl FetchBuilderMessages for FetchCommandMessages {
    fn prepare(self) -> FetchCommandMessages {
        let FetchCommandMessages { mut args } = self;
        args.push(b',');
        FetchCommandMessages { args }
    }
}

impl FetchCommandMessages {
    pub fn attr_macro(self, named: AttrMacro) -> FetchCommand {
        let FetchCommandMessages { mut args } = self;
        args.push(b' ');
        args.extend(
            match named {
                AttrMacro::All => "ALL",
                AttrMacro::Fast => "FAST",
                AttrMacro::Full => "FULL",
            }
            .as_bytes(),
        );
        FetchCommand { args }
    }
}

pub trait FetchBuilderMessages
where
    Self: Sized,
{
    fn prepare(self) -> FetchCommandMessages;

    fn num(self, num: u32) -> FetchCommandMessages {
        let FetchCommandMessages { mut args } = self.prepare();
        args.extend(num.to_string().as_bytes());
        FetchCommandMessages { args }
    }

    fn range(self, start: u32, stop: u32) -> FetchCommandMessages {
        let FetchCommandMessages { mut args } = self.prepare();
        args.extend(start.to_string().as_bytes());
        args.push(b':');
        args.extend(stop.to_string().as_bytes());
        FetchCommandMessages { args }
    }

    fn all_after(self, start: u32) -> FetchCommandMessages {
        let FetchCommandMessages { mut args } = self.prepare();
        args.extend(start.to_string().as_bytes());
        args.extend(b":*");
        FetchCommandMessages { args }
    }
}

pub struct FetchCommandAttributes {
    args: Vec<u8>,
}

impl FetchBuilderAttributes for FetchCommandMessages {
    fn prepare(self) -> FetchCommandAttributes {
        let FetchCommandMessages { mut args } = self;
        args.extend(b" (");
        FetchCommandAttributes { args }
    }
}

impl FetchBuilderAttributes for FetchCommandAttributes {
    fn prepare(self) -> FetchCommandAttributes {
        let FetchCommandAttributes { mut args } = self;
        args.push(b' ');
        FetchCommandAttributes { args }
    }
}

pub trait FetchBuilderAttributes
where
    Self: Sized,
{
    fn prepare(self) -> FetchCommandAttributes;
    fn attr(self, attr: Attribute) -> FetchCommandAttributes {
        let FetchCommandAttributes { mut args } = self.prepare();
        args.extend(
            match attr {
                Attribute::Body => "BODY",
                Attribute::Envelope => "ENVELOPE",
                Attribute::Flags => "FLAGS",
                Attribute::InternalDate => "INTERNALDATE",
                Attribute::ModSeq => "MODSEQ",
                Attribute::Rfc822 => "RFC822",
                Attribute::Rfc822Size => "RFC822.SIZE",
                Attribute::Rfc822Text => "RFC822.TEXT",
                Attribute::Uid => "UID",
            }
            .as_bytes(),
        );
        FetchCommandAttributes { args }
    }
}

pub struct FetchCommand {
    args: Vec<u8>,
}

pub trait FetchBuilderModifiers
where
    Self: Sized,
{
    fn prepare(self) -> FetchCommand;
    fn build(self) -> Command {
        let FetchCommand { args } = self.prepare();
        Command {
            args,
            next_state: None,
        }
    }
    fn changed_since(self, seq: u64) -> FetchCommand {
        let FetchCommand { mut args } = self.prepare();
        args.extend(b" (CHANGEDSINCE ");
        args.extend(seq.to_string().as_bytes());
        args.push(b')');
        FetchCommand { args }
    }
}

impl FetchBuilderModifiers for FetchCommandAttributes {
    fn prepare(self) -> FetchCommand {
        let FetchCommandAttributes { mut args, .. } = self;
        args.push(b')');
        FetchCommand { args }
    }
}

impl FetchBuilderModifiers for FetchCommand {
    fn prepare(self) -> FetchCommand {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::CommandBuilder;
    #[test]
    fn login() {
        assert_eq!(
            CommandBuilder::login("djc", "s3cr3t").into_parts().0,
            b"LOGIN \"djc\" \"s3cr3t\""
        );
        assert_eq!(
            CommandBuilder::login("djc", "domain\\password")
                .into_parts()
                .0,
            b"LOGIN \"djc\" \"domain\\\\password\""
        );
    }
}
