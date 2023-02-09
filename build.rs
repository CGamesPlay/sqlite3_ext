use proc_macro2::{Ident, Span, TokenStream};
use quote::*;
use std::env;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use syn;

const BINDGEN_OUTPUT: &str = "src/ffi/sqlite3types.rs";

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_STATIC");
    let static_link = if let Some(_) = env::var_os("CARGO_FEATURE_STATIC") {
        true
    } else {
        false
    };

    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_STATIC_MODERN");
    let modern_sqlite = if let Some(_) = env::var_os("CARGO_FEATURE_STATIC_MODERN") {
        true
    } else if !static_link {
        true
    } else {
        false
    };

    if modern_sqlite {
        println!("cargo:rustc-cfg=modern_sqlite");
    }

    generate_ffi(static_link, modern_sqlite);
}

fn generate_ffi(static_link: bool, modern_sqlite: bool) {
    println!("cargo:rerun-if-changed={}", BINDGEN_OUTPUT);
    let mut file = File::open(format!("{}", BINDGEN_OUTPUT)).expect(BINDGEN_OUTPUT);
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();

    let ast = syn::parse_file(&content).unwrap();
    let ident = Ident::new("sqlite3_api_routines", Span::call_site());
    let api_routines: Vec<(&syn::Ident, &syn::TypeBareFn)> = ast
        .items
        .iter()
        .filter_map(|i| match i {
            syn::Item::Struct(s) => Some(s),
            _ => None,
        })
        .find(|s| s.ident == ident)
        .map(|s| match &s.fields {
            syn::Fields::Named(n) => Some(n.named.iter()),
            _ => None,
        })
        .unwrap_or(None)
        .expect("sqlite3_api_routines missing")
        .map(|field| {
            let name = field.ident.as_ref().expect("unnamed method");
            let method = extract_method(&field.ty).expect("invalid field");
            (name, method)
        })
        // These methods are conditionally enabled by SQLite, but libsqlite3-sys does
        // not link them, so we need to remove them when not static linking.
        .filter(|(name, _)| !static_link || (*name != "normalized_sql" && *name != "unlock_notify"))
        .take_while(|(name, _)| modern_sqlite || *name != "close_v2")
        .collect();

    let methods: Vec<TokenStream> = api_routines
        .iter()
        .map(|(name, method)| {
            let unsafety = &method.unsafety;
            let abi = &method.abi;
            let arg_names: syn::punctuated::Punctuated<&Ident, syn::token::Comma> = method
                .inputs
                .iter()
                .map(|i| &i.name.as_ref().unwrap().0)
                .collect();
            let args = &method.inputs;
            let varargs = &method.variadic;
            let ty = &method.output;
            // Convert the api_routines field name into the actual sqlite3 method
            // name.
            let sqlite3_name = match name.to_string().as_str() {
                "interruptx" => format_ident!("sqlite3_interrupt"),
                "xsnprintf" => format_ident!("sqlite3_snprintf"),
                "xthreadsafe" => format_ident!("sqlite3_threadsafe"),
                "xvsnprintf" => format_ident!("sqlite3_vsnprintf"),
                _ => format_ident!("sqlite3_{}", name),
            };
            let checks = quote!(
                debug_assert!(!API.is_null(), "SQLite API not initialized");
            );
            if static_link {
                if let Some(_) = varargs {
                    quote! {
                        pub unsafe fn #sqlite3_name() -> #unsafety #abi fn(#args #varargs) #ty {
                            super::sqlite3funcs::#sqlite3_name
                        }
                    }
                } else {
                    quote! {
                        pub unsafe fn #sqlite3_name(#args) #ty {
                            super::sqlite3funcs::#sqlite3_name(#arg_names)
                        }
                    }
                }
            } else {
                if let Some(_) = varargs {
                    quote! {
                        pub unsafe fn #sqlite3_name() -> #unsafety #abi fn(#args #varargs) #ty {
                            #checks
                            (*API).#name.unwrap_unchecked()
                        }
                    }
                } else {
                    quote! {
                        pub unsafe fn #sqlite3_name(#args) #ty {
                            #checks
                            ((*API).#name.unwrap_unchecked())(#arg_names)
                        }
                    }
                }
            }
        })
        .collect();

    let preamble = if static_link {
        quote! {
            extern crate libsqlite3_sys;
            pub unsafe fn init_api_routines(api: *mut sqlite3_api_routines) -> crate::types::Result<()> {
                // This method is called when this statically linked extension is
                // loaded on a database connection. However, it's possible that
                // the extension is being loaded dynamically, in which case there
                // may be multiple versions of SQLite. We can check this by
                // verifying that the dynamically-provided API routines are at
                // the same address as our statically-known ones. Note that if
                // api is a null pointer, it means we are statically linked into
                // an application with SQLITE_OMIT_LOAD_EXTENSION (so no
                // worries).
                if !api.is_null() && (*api).libversion_number.unwrap() != libsqlite3_sys::sqlite3_libversion_number {
                    return Err(crate::types::Error::Module("this extension is statically linked to SQLite and cannot be used as a loadable extension".to_owned()));
                }
                Ok(())
            }
        }
    } else {
        quote! {
            static mut API: *mut sqlite3_api_routines = std::ptr::null_mut();
            pub unsafe fn init_api_routines(api: *mut sqlite3_api_routines) -> crate::types::Result<()> {
                API = api;
                Ok(())
            }
        }
    };

    let result = quote! {
        use super::sqlite3types::*;
        #preamble
        #(#methods)*
    };

    let tokens = TokenStream::from(result);
    let src = format!("{}", tokens);
    let formatted = rustfmt(src).unwrap();
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("linking.rs");
    fs::write(&dest_path, formatted).unwrap();
}

fn extract_method(ty: &syn::Type) -> Option<&syn::TypeBareFn> {
    match &ty {
        syn::Type::Path(tp) => tp.path.segments.last(),
        _ => None,
    }
    .map(|seg| match &seg.arguments {
        syn::PathArguments::AngleBracketed(args) => args.args.first(),
        _ => None,
    })?
    .map(|arg| match &arg {
        syn::GenericArgument::Type(t) => Some(t),
        _ => None,
    })?
    .map(|ty| match &ty {
        syn::Type::BareFn(r) => Some(r),
        _ => None,
    })?
}

fn rustfmt(input: String) -> Result<String, String> {
    let rustfmt =
        which::which("rustfmt").map_err(|e| format!("unable to locate rustfmt: {:?}", e))?;
    let mut rustfmt_child = std::process::Command::new(rustfmt)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn rustfmt: {:?}", e))?;
    let mut stdin = rustfmt_child.stdin.take().unwrap();

    ::std::thread::spawn(move || {
        stdin.write_all(input.as_bytes()).unwrap();
    });

    let output = rustfmt_child.wait_with_output().unwrap();
    if output.status.code() != Some(0) {
        return Err(format!("rustfmt exited with {:?}", output.status.code()));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("rustfmt returned invalid utf-8: {:?}", e))
}
