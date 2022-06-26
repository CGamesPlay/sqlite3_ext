use ext_attr::*;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use regex::Regex;
use std::mem::replace;
use syn::{punctuated::Punctuated, *};
use vtab_attr::*;

mod ext_attr;
mod vtab_attr;

mod kw {
    syn::custom_keyword!(export);
    syn::custom_keyword!(persistent);
    syn::custom_keyword!(StandardModule);
    syn::custom_keyword!(EponymousModule);
    syn::custom_keyword!(EponymousOnlyModule);
    syn::custom_keyword!(UpdateVTab);
    syn::custom_keyword!(TransactionVTab);
    syn::custom_keyword!(FindFunctionVTab);
    syn::custom_keyword!(RenameVTab);
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
    let item = parse_macro_input!(item as syn::ItemFn);
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
    let mut item = parse_macro_input!(item as syn::ItemFn);
    let extension_vis = replace(&mut item.vis, syn::Visibility::Inherited);
    let name = item.sig.ident.clone();
    let load_result = match persistent {
        None => quote!(::sqlite3_ext::ffi::SQLITE_OK),
        Some(tok) => {
            if let Some(_) = export {
                // Persistent loadable extensions were added in SQLite 3.14.0. If
                // the entry point for the extension returns
                // SQLITE_OK_LOAD_PERSISTENT, then the load fails. We want to
                // detect this situation and allow the load to complete anyways:
                // any API which requires persistent extensions would return an
                // error, but ignored errors imply that the persistent loading
                // requirement is optional.
                quote!(::sqlite3_ext::sqlite3_require_version!(
                    3_014_000,
                    ::sqlite3_ext::ffi::SQLITE_OK_LOAD_PERMANENTLY,
                    ::sqlite3_ext::ffi::SQLITE_OK
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
                ::sqlite3_ext::ffi::init_api_routines(api);
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
    let item = parse_macro_input!(item as syn::ItemStruct);
    let struct_ident = &item.ident;
    let struct_generics = &item.generics;
    let base = match attr.base {
        VTabBase::Standard(_) => quote!(::sqlite3_ext::vtab::StandardModule),
        VTabBase::Eponymous(_) => quote!(::sqlite3_ext::vtab::EponymousModule),
        VTabBase::EponymousOnly(_) => quote!(::sqlite3_ext::vtab::EponymousOnlyModule),
    };
    let mut expr = quote!(#base::<Self>::new());
    let ret = if let VTabBase::EponymousOnly(_) = attr.base {
        expr.extend(quote!(?));
        quote!(::sqlite3_ext::Result<#base<'a, Self>>)
    } else {
        quote!(#base<'a, Self>)
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
        impl #struct_ident #struct_generics {
            /// Return the [Module](::sqlite3_ext::vtab::Module) associated with
            /// this virtual table.
            pub fn module<'a>() -> #ret {
                use ::sqlite3_ext::vtab::*;
                #expr
            }
        }
    };
    TokenStream::from(expanded)
}

#[doc(hidden)]
#[proc_macro]
pub fn sqlite3_ext_doctest_impl(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as syn::Ident);
    let expanded = quote! {
        impl<'vtab> ::sqlite3_ext::vtab::VTab<'vtab> for #item {
            type Aux = ();
            type Cursor = Cursor;

            fn connect(_: &'vtab mut ::sqlite3_ext::vtab::VTabConnection, _: &'vtab Self::Aux, _: &[&str]) -> std::result::Result<(String, Self), ::sqlite3_ext::Error> { todo!() }
            fn best_index(&self, _: &mut ::sqlite3_ext::vtab::IndexInfo) -> std::result::Result<(), ::sqlite3_ext::Error> { todo!() }
            fn open(&'vtab mut self) -> std::result::Result<Self::Cursor, ::sqlite3_ext::Error> { todo!() }
        }

        impl<'vtab> ::sqlite3_ext::vtab::CreateVTab<'vtab> for #item {
            fn create(_: &'vtab mut ::sqlite3_ext::vtab::VTabConnection, _: &'vtab Self::Aux, _: &[&str]) -> std::result::Result<(String, Self), ::sqlite3_ext::Error> { todo!() }
            fn destroy(&mut self) -> std::result::Result<(), ::sqlite3_ext::Error> { todo!() }
        }

        impl<'vtab> ::sqlite3_ext::vtab::UpdateVTab<'vtab> for #item {
            fn insert(&mut self, _: &[&::sqlite3_ext::ValueRef]) -> std::result::Result<i64, ::sqlite3_ext::Error> { todo!() }
            fn update(&mut self, _: &::sqlite3_ext::ValueRef, _: &[&::sqlite3_ext::ValueRef]) -> std::result::Result<(), ::sqlite3_ext::Error> { todo!() }
            fn delete(&mut self, _: &::sqlite3_ext::ValueRef) -> std::result::Result<(), ::sqlite3_ext::Error> { todo!() }
        }

        struct Cursor {}
        impl ::sqlite3_ext::vtab::VTabCursor for Cursor {
            type ColumnType = ();
            fn filter(&mut self, _: usize, _: Option<&str>, _: &[&ValueRef]) -> std::result::Result<(), ::sqlite3_ext::Error> { todo!() }
            fn next(&mut self) -> std::result::Result<(), ::sqlite3_ext::Error> { todo!() }
            fn eof(&self) -> bool { todo!() }
            fn column(&self, _: usize) { todo!() }
            fn rowid(&self) -> std::result::Result<i64, ::sqlite3_ext::Error> { todo!() }
        }
    };
    TokenStream::from(expanded)
}
