use super::kw;
use syn::{
    parse::{Parse, ParseStream},
    *,
};

pub enum ExtAttr {
    Export(ExtAttrExport),
    Persistent(kw::persistent),
}

pub struct ExtAttrExport {
    pub value: Ident,
}

impl Parse for ExtAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let lookahead = input.lookahead1();
        if lookahead.peek(kw::export) {
            input.parse().map(ExtAttr::Export)
        } else if lookahead.peek(kw::persistent) {
            input.parse().map(ExtAttr::Persistent)
        } else {
            Err(lookahead.error())
        }
    }
}

impl Parse for ExtAttrExport {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<kw::export>()?;
        input.parse::<token::Eq>()?;
        Ok(ExtAttrExport {
            value: input.parse()?,
        })
    }
}
