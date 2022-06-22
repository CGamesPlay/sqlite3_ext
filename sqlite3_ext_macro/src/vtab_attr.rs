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
    Standard(kw::StandardModule),
    Eponymous(kw::EponymousModule),
    EponymousOnly(kw::EponymousOnlyModule),
}

pub enum VTabTrait {
    UpdateVTab(kw::UpdateVTab),
    TransactionVTab(kw::TransactionVTab),
    FindFunctionVTab(kw::FindFunctionVTab),
    RenameVTab(kw::RenameVTab),
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
        if lookahead.peek(kw::StandardModule) {
            input.parse().map(VTabBase::Standard)
        } else if lookahead.peek(kw::EponymousModule) {
            input.parse().map(VTabBase::Eponymous)
        } else if lookahead.peek(kw::EponymousOnlyModule) {
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
        } else if lookahead.peek(kw::ShadowNameVTab) {
            input.parse().map(VTabTrait::ShadowNameVTab)
        } else {
            Err(lookahead.error())
        }
    }
}
