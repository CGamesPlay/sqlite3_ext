use convert_case::{Case, Casing};
use ext_attr::*;
use fn_attr::*;
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote, ToTokens};
use regex::Regex;
use std::mem::replace;
use syn::{punctuated::Punctuated, *};
use vtab_attr::*;

mod ext_attr;
mod fn_attr;
mod vtab_attr;

mod kw {
    syn::custom_keyword!(DirectOnly);
    syn::custom_keyword!(EponymousModule);
    syn::custom_keyword!(EponymousOnlyModule);
    syn::custom_keyword!(FindFunctionVTab);
    syn::custom_keyword!(Innocuous);
    syn::custom_keyword!(RenameVTab);
    syn::custom_keyword!(StandardModule);
    syn::custom_keyword!(TransactionVTab);
    syn::custom_keyword!(UpdateVTab);
    syn::custom_keyword!(deterministic);
    syn::custom_keyword!(export);
    syn::custom_keyword!(n_args);
    syn::custom_keyword!(persistent);
    syn::custom_keyword!(risk_level);
}

/// Declare the primary extension entry point for the crate.
///
/// This is equivalent to [macro@sqlite3_ext_init], but it will automatically name the export
/// according to the name of the crate (e.g. `sqlite3_myextension_init`).
///
/// # Examples
///
/// Specify a persistent extension:
///
/// ```no_run
/// # use sqlite3_ext_macro::*;
/// use sqlite3_ext::*;
///
/// #[sqlite3_ext_main(persistent)]
/// fn init(db: &Connection) -> Result<()> {
///     Ok(())
/// }
/// ```
#[proc_macro_attribute]
pub fn sqlite3_ext_main(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = proc_macro2::TokenStream::from(attr);
    let item = parse_macro_input!(item as ItemFn);
    let crate_name = std::env::var("CARGO_CRATE_NAME").unwrap();
    let export_base = crate_name.to_lowercase();
    let export_base = Regex::new("[^a-z]").unwrap().replace_all(&export_base, "");
    let init_ident = format_ident!("sqlite3_{}_init", export_base);
    let expanded = quote! {
        #[::sqlite3_ext::sqlite3_ext_init(export = #init_ident, #attr)]
        #item
    };
    TokenStream::from(expanded)
}

/// Declare the entry point to an extension.
///
/// This method generates an `extern "C"` function suitable for use by SQLite's loadable
/// extension feature. An export name can optionally be provided. Consult [the SQLite
/// documentation](https://www.sqlite.org/loadext.html#loading_an_extension) for information
/// about naming the exported method, but generally you can use [macro@sqlite3_ext_main] to
/// automatically name the export correctly.
///
/// If the persistent keyword is included in the attribute, the extension will be loaded
/// permanently. See [the SQLite
/// documentation](https://www.sqlite.org/loadext.html#persistent_loadable_extensions) for more
/// information.
///
/// # Example
///
/// Specifying a nonstandard entry point name:
///
/// ```no_run
/// # use sqlite3_ext_macro::*;
/// use sqlite3_ext::*;
///
/// #[sqlite3_ext_init(export = nonstandard_entry_point, persistent)]
/// fn init(db: &Connection) -> Result<()> {
///     Ok(())
/// }
/// ```
///
/// This extension could be loaded from SQLite:
///
/// ```sql
/// SELECT load_extension('path/to/extension', 'nonstandard_entry_point');
/// ```
///
/// # Implementation
///
/// This macro renames the original Rust function and instead creates an
/// `sqlite3_ext::Extension` object in its place. Because `Extension` dereferences to the
/// original function, you generally won't notice this change. This behavior allows you to use
/// the original identifier to pass the auto extension methods.
#[proc_macro_attribute]
pub fn sqlite3_ext_init(attr: TokenStream, item: TokenStream) -> TokenStream {
    let directives =
        parse_macro_input!(attr with Punctuated::<ExtAttr, Token![,]>::parse_terminated);
    let mut export: Option<Ident> = None;
    let mut persistent: Option<kw::persistent> = None;
    for d in directives {
        match d {
            ExtAttr::Export(ExtAttrExport { value }) => {
                if let Some(_) = export {
                    return Error::new(value.span(), "export specified multiple times")
                        .into_compile_error()
                        .into();
                } else {
                    export = Some(value)
                }
            }
            ExtAttr::Persistent(tok) => {
                persistent = Some(tok);
            }
        }
    }
    let mut item = parse_macro_input!(item as ItemFn);
    let extension_vis = replace(&mut item.vis, Visibility::Inherited);
    let name = item.sig.ident.clone();
    let load_result = match persistent {
        None => quote!(::sqlite3_ext::ffi::SQLITE_OK),
        Some(tok) => {
            if let Some(_) = export {
                // Persistent loadable extensions were added in SQLite 3.14.0. If
                // we were to return SQLITE_OK_LOAD_PERSISTENT, then the load
                // would fail. We want the load to complete: any API which
                // requires persistent extensions would return an error, but
                // ignored errors imply that the persistent loading requirement
                // is optional.
                quote!(::sqlite3_ext::sqlite3_match_version!(
                    3_014_000 => ::sqlite3_ext::ffi::SQLITE_OK_LOAD_PERMANENTLY,
                    _ => ::sqlite3_ext::ffi::SQLITE_OK,
                ))
            } else {
                return Error::new(tok.span, "unexported extension cannot be persistent")
                    .into_compile_error()
                    .into();
            }
        }
    };

    let c_export = export.as_ref().map(|_| quote!(#[no_mangle] pub));
    let c_name = match export {
        None => format_ident!("{}_entry", item.sig.ident),
        Some(x) => x,
    };

    let expanded = quote! {
        #[allow(non_upper_case_globals)]
        #extension_vis static #name: ::sqlite3_ext::Extension = {
            #c_export
            unsafe extern "C" fn #c_name(
                db: *mut ::sqlite3_ext::ffi::sqlite3,
                err_msg: *mut *mut ::std::os::raw::c_char,
                api: *mut ::sqlite3_ext::ffi::sqlite3_api_routines,
            ) -> ::std::os::raw::c_int {
                if let Err(e) = ::sqlite3_ext::ffi::init_api_routines(api) {
                    return ::sqlite3_ext::ffi::handle_error(e, err_msg);
                }
                match #name(::sqlite3_ext::Connection::from_ptr(db)) {
                    Ok(_) => #load_result,
                    Err(e) => ::sqlite3_ext::ffi::handle_error(e, err_msg),
                }
            }

            #item

            ::sqlite3_ext::Extension::new(#c_name, #name)
        };
    };
    TokenStream::from(expanded)
}

/// Declare a virtual table module.
///
/// This attribute is intended to be applied to the struct which implements VTab and related
/// traits. The first parameter to the attribute is the type of module to create, which is one
/// of StandardModule, EponymousModule, EponymousOnlyModule. The subsequent parameters refer to
/// traits in sqlite3_ext::vtab, and describe the functionality which the virtual table
/// supports. See the corresponding structs and traits in sqlite3_ext::vtab for more details.
///
/// The resulting struct will have an associated method `module` which returns the concrete
/// type of module specified in the first parameter, or a Result containing it.
///
/// # Examples
///
/// Declare a table-valued function:
///
/// ```no_run
/// # use sqlite3_ext_macro::*;
/// use sqlite3_ext::*;
///
/// #[sqlite3_ext_vtab(EponymousModule)]
/// struct MyTableFunction {}
/// # sqlite3_ext_doctest_impl!(MyTableFunction);
///
/// #[sqlite3_ext_main]
/// fn init(db: &Connection) -> Result<()> {
///     db.create_module("my_table_function", MyTableFunction::module(), ())?;
///     Ok(())
/// }
/// ```
///
/// Declare a standard virtual table that supports updates:
///
/// ```no_run
/// # use sqlite3_ext_macro::*;
/// use sqlite3_ext::*;
///
/// #[sqlite3_ext_vtab(StandardModule, UpdateVTab)]
/// struct MyTable {}
/// # sqlite3_ext_doctest_impl!(MyTable);
///
/// #[sqlite3_ext_main]
/// fn init(db: &Connection) -> Result<()> {
///     db.create_module("my_table", MyTable::module(), ())?;
///     Ok(())
/// }
/// ```
///
/// Declare an eponymous-only table that supports updates:
///
/// ```no_run
/// # use sqlite3_ext_macro::*;
/// use sqlite3_ext::*;
///
/// #[sqlite3_ext_vtab(EponymousOnlyModule, UpdateVTab)]
/// struct MyTable {}
/// # sqlite3_ext_doctest_impl!(MyTable);
///
/// #[sqlite3_ext_main]
/// fn init(db: &Connection) -> Result<()> {
///     db.create_module("my_table", MyTable::module()?, ())?;
///     Ok(())
/// }
/// ```
#[proc_macro_attribute]
pub fn sqlite3_ext_vtab(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = match parse::<VTabAttr>(attr) {
        Ok(syntax_tree) => syntax_tree,
        Err(err) => {
            let mut ret = TokenStream::from(err.to_compile_error());
            ret.extend(item);
            return ret;
        }
    };
    let item = parse_macro_input!(item as ItemStruct);
    let struct_generics = &item.generics;
    let impl_arguments = if struct_generics.params.is_empty() {
        None
    } else {
        Some(AngleBracketedGenericArguments {
            colon2_token: None,
            lt_token: token::Lt::default(),
            args: struct_generics
                .params
                .iter()
                .map(|gp| match gp {
                    GenericParam::Type(t) => {
                        GenericArgument::Type(Type::Verbatim(t.ident.to_token_stream()))
                    }
                    GenericParam::Lifetime(l) => GenericArgument::Lifetime(l.lifetime.clone()),
                    GenericParam::Const(c) => {
                        GenericArgument::Const(Expr::Verbatim(c.ident.to_token_stream()))
                    }
                })
                .collect(),
            gt_token: token::Gt::default(),
        })
    };
    let struct_generic_def = {
        let mut segments = Punctuated::default();
        segments.push_value(PathSegment {
            ident: item.ident.clone(),
            arguments: impl_arguments
                .map(PathArguments::AngleBracketed)
                .unwrap_or(PathArguments::None),
        });
        Type::Path(TypePath {
            qself: None,
            path: Path {
                leading_colon: None,
                segments,
            },
        })
    };
    let lifetime = quote!('sqlite3_ext_vtab);
    let lifetime_bounds: Punctuated<_, Token![+]> = struct_generics
        .params
        .iter()
        .filter_map(|gp| {
            if let GenericParam::Lifetime(LifetimeDef { lifetime, .. }) = gp {
                Some(lifetime)
            } else {
                None
            }
        })
        .collect();
    let lifetime_bounds = if lifetime_bounds.is_empty() {
        quote!()
    } else {
        quote!(: #lifetime_bounds)
    };
    let base = match attr.base {
        VTabBase::Standard(_) => quote!(::sqlite3_ext::vtab::StandardModule),
        VTabBase::Eponymous(_) => quote!(::sqlite3_ext::vtab::EponymousModule),
        VTabBase::EponymousOnly(_) => quote!(::sqlite3_ext::vtab::EponymousOnlyModule),
    };
    let mut expr = quote!(#base::<Self>::new());
    let ret = if let VTabBase::EponymousOnly(_) = attr.base {
        expr.extend(quote!(?));
        quote!(::sqlite3_ext::Result<#base<#lifetime, Self>>)
    } else {
        quote!(#base<#lifetime, Self>)
    };
    for t in attr.additional {
        match t {
            VTabTrait::UpdateVTab(_) => expr.extend(quote!(.with_update())),
            VTabTrait::TransactionVTab(_) => expr.extend(quote!(.with_transactions())),
            VTabTrait::FindFunctionVTab(_) => expr.extend(quote!(.with_find_function())),
            VTabTrait::RenameVTab(_) => expr.extend(quote!(.with_rename())),
        }
    }
    if let VTabBase::EponymousOnly(_) = attr.base {
        expr = quote!(Ok(#expr));
    };
    let expanded = quote! {
        #item

        #[automatically_derived]
        impl #struct_generics #struct_generic_def {
            /// Return the [Module](::sqlite3_ext::vtab::Module) associated with
            /// this virtual table.
            pub fn module<#lifetime #lifetime_bounds> () -> #ret {
                use ::sqlite3_ext::vtab::*;
                #expr
            }
        }
    };
    TokenStream::from(expanded)
}

/// Create a FunctionOptions for an application-defined function.
///
/// This macro declares a FunctionOptions constant with the provided values. The constant will
/// take on a name based on the function, so for example applying this attribute to a function
/// named "count_horses" or a trait named "CountHorses" will create a constant named
/// "COUNT_HORSES_OPTS".
///
/// # Syntax
///
/// Arguments passed to the macro are comma-separated. The following are supported:
///
/// - `n_args=N` corresponds to set_n_args.
/// - `risk_level=X` corresponds to set_risk_level.
/// - `deterministic` corresponds to set_desterministic with true.
///
/// # Example
///
/// ```no_run
/// use sqlite3_ext::{function::*, *};
///
/// #[sqlite3_ext_fn(n_args=0, risk_level=Innocuous)]
/// pub fn random_number(ctx: &Context, args: &mut [&mut ValueRef]) -> Result<()> {
///     ctx.set_result(4) // chosen by fair dice roll.
/// }
///
/// pub fn init(db: &Connection) -> Result<()> {
///     db.create_scalar_function("random_number", &RANDOM_NUMBER_OPTS, random_number)
/// }
/// ```
#[proc_macro_attribute]
pub fn sqlite3_ext_fn(attr: TokenStream, item: TokenStream) -> TokenStream {
    let directives =
        parse_macro_input!(attr with Punctuated::<FnAttr, Token![,]>::parse_terminated);
    let item = parse_macro_input!(item as Item);
    let (ident, vis) = match &item {
        Item::Fn(item) => (&item.sig.ident, &item.vis),
        Item::Struct(item) => (&item.ident, &item.vis),
        _ => {
            return TokenStream::from(
                Error::new(Span::call_site(), "only applies to fn or struct").into_compile_error(),
            )
        }
    };
    let opts_name = Ident::new(
        &format!("{}_opts", ident).to_case(Case::UpperSnake),
        Span::call_site(),
    );
    let mut opts = quote! {
        #[automatically_derived]
        #vis const #opts_name: ::sqlite3_ext::function::FunctionOptions = ::sqlite3_ext::function::FunctionOptions::default()
    };
    for d in directives {
        match d {
            FnAttr::NumArgs(x) => opts.extend(quote!(.set_n_args(#x))),
            FnAttr::RiskLevel(FnAttrRiskLevel::Innocuous) => {
                opts.extend(quote!(.set_risk_level(::sqlite3_ext::RiskLevel::Innocuous)))
            }
            FnAttr::RiskLevel(FnAttrRiskLevel::DirectOnly) => {
                opts.extend(quote!(.set_risk_level(::sqlite3_ext::RiskLevel::DirectOnly)))
            }
            FnAttr::Deterministic => opts.extend(quote!(.set_deterministic(true))),
        }
    }
    let expanded = quote! {
        #opts;
        #item
    };
    TokenStream::from(expanded)
}

#[doc(hidden)]
#[proc_macro]
pub fn sqlite3_ext_doctest_impl(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as Type);
    let expanded = quote! {
        impl<'vtab> ::sqlite3_ext::vtab::VTab<'vtab> for #item {
            type Aux = ();
            type Cursor = Cursor;

            fn connect(_: &::sqlite3_ext::vtab::VTabConnection, _: &Self::Aux, _: &[&str]) -> std::result::Result<(String, Self), ::sqlite3_ext::Error> { todo!() }
            fn best_index(&self, _: &mut ::sqlite3_ext::vtab::IndexInfo) -> std::result::Result<(), ::sqlite3_ext::Error> { todo!() }
            fn open(&self) -> std::result::Result<Self::Cursor, ::sqlite3_ext::Error> { todo!() }
        }

        impl<'vtab> ::sqlite3_ext::vtab::CreateVTab<'vtab> for #item {
            fn create(_: &::sqlite3_ext::vtab::VTabConnection, _: &Self::Aux, _: &[&str]) -> std::result::Result<(String, Self), ::sqlite3_ext::Error> { todo!() }
            fn destroy(&mut self) -> std::result::Result<(), ::sqlite3_ext::Error> { todo!() }
        }

        impl<'vtab> ::sqlite3_ext::vtab::UpdateVTab<'vtab> for #item {
            fn update(&self, _: &mut ::sqlite3_ext::vtab::ChangeInfo) -> ::sqlite3_ext::Result<i64> { todo!() }
        }

        struct Cursor {}
        impl ::sqlite3_ext::vtab::VTabCursor<'_> for Cursor {
            fn filter(&mut self, _: i32, _: Option<&str>, _: &mut [&mut ValueRef]) -> std::result::Result<(), ::sqlite3_ext::Error> { todo!() }
            fn next(&mut self) -> std::result::Result<(), ::sqlite3_ext::Error> { todo!() }
            fn eof(&mut self) -> bool { todo!() }
            fn column(&mut self, _: usize, _: &::sqlite3_ext::vtab::ColumnContext) -> Result<()> { todo!() }
            fn rowid(&mut self) -> std::result::Result<i64, ::sqlite3_ext::Error> { todo!() }
        }
    };
    TokenStream::from(expanded)
}
