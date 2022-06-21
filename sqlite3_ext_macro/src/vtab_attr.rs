use super::kw;
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    *,
};

pub struct VTabAttr {
    pub base: VTabBase,
    pub additional: Punctuated<VTabTrait, Token![,]>,
}

pub enum VTabBase {
    Standard(kw::standard),
    Eponymous(kw::eponymous),
    EponymousOnly(kw::eponymous_only),
}

pub enum VTabTrait {
    UpdateVTab(kw::UpdateVTab),
    RenameVTab(kw::RenameVTab),
}

impl Parse for VTabAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let base = input.parse()?;
        let additional = if let Ok(_) = input.parse::<Token![,]>() {
            Punctuated::parse_terminated(input)?
        } else {
            Punctuated::new()
        };
        Ok(VTabAttr { base, additional })
    }
}

impl Parse for VTabBase {
    fn parse(input: ParseStream) -> Result<Self> {
        let lookahead = input.lookahead1();
        if lookahead.peek(kw::standard) {
            input.parse().map(VTabBase::Standard)
        } else if lookahead.peek(kw::eponymous) {
            input.parse().map(VTabBase::Eponymous)
        } else if lookahead.peek(kw::eponymous_only) {
            input.parse().map(VTabBase::EponymousOnly)
        } else {
            Err(lookahead.error())
        }
    }
}

impl Parse for VTabTrait {
    fn parse(input: ParseStream) -> Result<Self> {
        let lookahead = input.lookahead1();
        if lookahead.peek(kw::UpdateVTab) {
            input.parse().map(VTabTrait::UpdateVTab)
        } else if lookahead.peek(kw::RenameVTab) {
            input.parse().map(VTabTrait::RenameVTab)
        } else {
            Err(lookahead.error())
        }
    }
}
