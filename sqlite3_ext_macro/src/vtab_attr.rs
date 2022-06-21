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
    TransactionVTab(kw::TransactionVTab),
    FindFunctionVTab(kw::FindFunctionVTab),
    RenameVTab(kw::RenameVTab),
    SavepointVTab(kw::SavepointVTab),
    ShadowNameVTab(kw::ShadowNameVTab),
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
        } else if lookahead.peek(kw::TransactionVTab) {
            input.parse().map(VTabTrait::TransactionVTab)
        } else if lookahead.peek(kw::FindFunctionVTab) {
            input.parse().map(VTabTrait::FindFunctionVTab)
        } else if lookahead.peek(kw::RenameVTab) {
            input.parse().map(VTabTrait::RenameVTab)
        } else if lookahead.peek(kw::SavepointVTab) {
            input.parse().map(VTabTrait::SavepointVTab)
        } else if lookahead.peek(kw::ShadowNameVTab) {
            input.parse().map(VTabTrait::ShadowNameVTab)
        } else {
            Err(lookahead.error())
        }
    }
}
