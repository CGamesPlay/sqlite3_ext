use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    *,
};

mod kw {
    syn::custom_keyword!(export);
}

/// Declare the primary extension entry point for the crate.
///
/// This is equivalent to [macro@sqlite3_ext_init], but it will automatically name the export
/// according to the name of the crate (e.g. `sqlite3_myextension_init`).
#[proc_macro_attribute]
pub fn sqlite3_ext_main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as syn::ItemFn);
    let crate_name = std::env::var("CARGO_CRATE_NAME").unwrap();
    let init_ident = format_ident!("sqlite3_{}_init", crate_name);
    let expanded = quote! {
        #[sqlite3_ext_init(export = #init_ident)]
        #item
    };
    TokenStream::from(expanded)
}

struct ExtAttrName {
    value: Ident,
}

impl Parse for ExtAttrName {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<kw::export>()?;
        input.parse::<token::Eq>()?;
        Ok(ExtAttrName {
            value: input.parse()?,
        })
    }
}

/// Declare the entry point to an extension.
///
/// This method generates an `extern "C"` function suitable for use by SQLite's loadable
/// extension feature. An export name can optionally be provided. Consult [the SQLite
/// documentation](https://www.sqlite.org/loadext.html#loading_an_extension) for information
/// about naming the exported method, but generally you can use [macro@sqlite3_ext_main] to
/// automatically name the export correctly.
///
/// The extension entry point must return a `Result<bool>`. Returning `Ok(true)` is equivalent
/// to returning `SQLITE_OK_LOAD_PERMANENTLY`, meaning that the extension will not be unloaded
/// when the connection is closed. Returning `Ok(false)` is equivalent to returning
/// `SQLITE_OK`. See [the SQLite
/// documentation](https://www.sqlite.org/loadext.html#persistent_loadable_extensions) for more
/// information.
///
/// # Example
///
/// Specifying a nonstandard entry point name:
///
/// ```
/// #[sqlite3_ext_init(export = "nonstandard_entry_point")]
/// fn init(db: &Connection) -> Result<bool> {
///     Ok(false)
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
/// renamed function, you generally won't notice this change. This behavior allows you to use
/// the original identifier to pass the auto extension methods.
#[proc_macro_attribute]
pub fn sqlite3_ext_init(attr: TokenStream, item: TokenStream) -> TokenStream {
    let directives =
        parse_macro_input!(attr with Punctuated::<ExtAttrName, Token![,]>::parse_terminated);
    if directives.len() > 1 {
        return Error::new(directives[1].value.span(), "multiple names")
            .into_compile_error()
            .into();
    }
    let export_vis = directives.first().map(|_| quote!(#[no_mangle] pub));
    let mut item = parse_macro_input!(item as syn::ItemFn);
    let name = item.sig.ident.clone();
    let c_name = match directives.first() {
        None => format_ident!("{}_cfunc", item.sig.ident),
        Some(x) => x.value.clone(),
    };
    let rust_name = format_ident!("{}_rust", item.sig.ident);
    item.sig.ident = rust_name.clone();
    let expanded = quote! {
        #[allow(non_upper_case_globals)]
        static #name: ::sqlite3_ext::Extension = ::sqlite3_ext::Extension::new(#c_name, #rust_name);

        #export_vis
        unsafe extern "C" fn #c_name(
            db: *mut ::sqlite3_ext::ffi::sqlite3,
            err_msg: *mut *mut ::std::os::raw::c_char,
            api: *mut ::sqlite3_ext::ffi::sqlite3_api_routines,
        ) -> ::std::os::raw::c_int {
            ::sqlite3_ext::ffi::init_api_routines(api);
            match #name(&::sqlite3_ext::Connection::from(db)) {
                Ok(true) => ::sqlite3_ext::ffi::SQLITE_OK_LOAD_PERMANENTLY,
                Ok(false) => ::sqlite3_ext::ffi::SQLITE_OK,
                Err(e) => ::sqlite3_ext::ffi::handle_error(e, err_msg),
            }
        }

        #item
    };
    TokenStream::from(expanded)
}
