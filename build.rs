use proc_macro2::{Ident, Span, TokenStream};
use quote::*;
use std::env;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use syn;

fn extract_method(ty: &syn::Type) -> Option<&syn::TypeBareFn> {
    match &ty {
        syn::Type::Path(tp) => tp.path.segments.last(),
        _ => None,
    }
    .map(|seg| match &seg.arguments {
        syn::PathArguments::AngleBracketed(args) => args.args.first(),
        _ => None,
    })
    .unwrap_or(None)
    .map(|arg| match &arg {
        syn::GenericArgument::Type(t) => Some(t),
        _ => None,
    })
    .unwrap_or(None)
    .map(|ty| match &ty {
        syn::Type::BareFn(r) => Some(r),
        _ => None,
    })
    .unwrap_or(None)
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

fn generate_api_routines() {
    println!("cargo:rerun-if-changed=src/ffi/sqlite3ext.rs");
    let mut file = File::open("src/ffi/sqlite3ext.rs").unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();

    let ast = syn::parse_file(&content).unwrap();
    let ident = Ident::new("sqlite3_api_routines", Span::call_site());
    let api_routines: Vec<&syn::Field> = ast
        .items
        .iter()
        .filter_map(|i| match i {
            syn::Item::Struct(s) => Some(s),
            _ => None,
        })
        .find(|s| s.ident == ident)
        .map(|s| match &s.fields {
            syn::Fields::Named(n) => Some(n.named.iter().collect()),
            _ => None,
        })
        .unwrap_or(None)
        .expect("sqlite3_api_routines missing");

    let init_lines: Vec<TokenStream> = api_routines
        .iter()
        .map(|field| {
            let field_name = &field.ident;
            let var_name =
                format_ident!("sqlite3_{}", field.ident.as_ref().expect("unnamed method"));
            quote! {
                if let Some(x) = (*api).#field_name {
                    #var_name = x;
                }
            }
        })
        .collect();

    let methods: Vec<TokenStream> = api_routines
        .iter()
        .map(|field| {
            let name = format_ident!("sqlite3_{}", field.ident.as_ref().expect("unnamed method"));
            let method = extract_method(&field.ty).expect("invalid field");
            let unsafety = &method.unsafety;
            let abi = &method.abi;
            let args = &method.inputs;
            let varargs = &method.variadic;
            let ty = &method.output;
            quote! {
                pub static mut #name: #unsafety #abi fn (#args #varargs) #ty = unsafe { ::std::mem::transmute(unavailable as *mut ::std::os::raw::c_void) };
            }
        })
        .collect();

    let result = quote! {
        pub fn init_api_routines(api: *mut sqlite3_api_routines) {
            unsafe {
                #(#init_lines)*
            }
        }

        fn unavailable() {
            panic!("call to unavailabke sqlite method");
        }

        #(#methods)*
    };

    let tokens = TokenStream::from(result);
    let unformatted = format!("{}", tokens);
    let formatted = rustfmt(unformatted).unwrap();
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("sqlite3_api_routines.rs");
    fs::write(&dest_path, formatted).unwrap();
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    generate_api_routines();
}
