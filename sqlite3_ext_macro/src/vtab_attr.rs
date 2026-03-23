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
    Standard,
    Eponymous,
    EponymousOnly,
}

pub enum VTabTrait {
    Update,
    Transaction,
    FindFunction,
    Rename,
}

impl Parse for VTabAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let base = input.parse()?;
        let additional = if input.parse::<Token![,]>().is_ok() {
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
            input
                .parse::<kw::StandardModule>()
                .map(|_| VTabBase::Standard)
        } else if lookahead.peek(kw::EponymousModule) {
            input
                .parse::<kw::EponymousModule>()
                .map(|_| VTabBase::Eponymous)
        } else if lookahead.peek(kw::EponymousOnlyModule) {
            input
                .parse::<kw::EponymousOnlyModule>()
                .map(|_| VTabBase::EponymousOnly)
        } else {
            Err(lookahead.error())
        }
    }
}

impl Parse for VTabTrait {
    fn parse(input: ParseStream) -> Result<Self> {
        let lookahead = input.lookahead1();
        if lookahead.peek(kw::UpdateVTab) {
            input.parse::<kw::UpdateVTab>().map(|_| VTabTrait::Update)
        } else if lookahead.peek(kw::TransactionVTab) {
            input
                .parse::<kw::TransactionVTab>()
                .map(|_| VTabTrait::Transaction)
        } else if lookahead.peek(kw::FindFunctionVTab) {
            input
                .parse::<kw::FindFunctionVTab>()
                .map(|_| VTabTrait::FindFunction)
        } else if lookahead.peek(kw::RenameVTab) {
            input.parse::<kw::RenameVTab>().map(|_| VTabTrait::Rename)
        } else {
            Err(lookahead.error())
        }
    }
}
