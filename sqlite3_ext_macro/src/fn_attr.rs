use super::kw;
use syn::{
    parse::{Parse, ParseStream},
    *,
};

pub enum FnAttr {
    NumArgs(LitInt),
    RiskLevel(FnAttrRiskLevel),
    Deterministic,
}

impl Parse for FnAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let lookahead = input.lookahead1();
        if lookahead.peek(kw::n_args) {
            input.parse::<kw::n_args>()?;
            input.parse::<Token![=]>()?;
            input.parse().map(FnAttr::NumArgs)
        } else if lookahead.peek(kw::risk_level) {
            input.parse::<kw::risk_level>()?;
            input.parse::<Token![=]>()?;
            input.parse().map(FnAttr::RiskLevel)
        } else if lookahead.peek(kw::deterministic) {
            input.parse::<kw::deterministic>()?;
            Ok(FnAttr::Deterministic)
        } else {
            Err(lookahead.error())
        }
    }
}

pub enum FnAttrRiskLevel {
    Innocuous,
    DirectOnly,
}

impl Parse for FnAttrRiskLevel {
    fn parse(input: ParseStream) -> Result<Self> {
        let lookahead = input.lookahead1();
        if lookahead.peek(kw::Innocuous) {
            input.parse::<kw::Innocuous>()?;
            Ok(FnAttrRiskLevel::Innocuous)
        } else if lookahead.peek(kw::DirectOnly) {
            input.parse::<kw::DirectOnly>()?;
            Ok(FnAttrRiskLevel::DirectOnly)
        } else {
            Err(lookahead.error())
        }
    }
}
