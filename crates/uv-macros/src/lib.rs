mod options_metadata;

use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Attribute, DeriveInput, ImplItem, ItemImpl, LitStr, parse_macro_input};

#[proc_macro_derive(OptionsMetadata, attributes(option, doc, option_group))]
pub fn derive_options_metadata(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    options_metadata::derive_impl(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(CombineOptions)]
pub fn derive_combine(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    impl_combine(&input)
}

fn impl_combine(ast: &DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let fields = if let syn::Data::Struct(syn::DataStruct {
        fields: syn::Fields::Named(ref fields),
        ..
    }) = ast.data
    {
        &fields.named
    } else {
        unimplemented!();
    };

    let combines = fields.iter().map(|f| {
        let name = &f.ident;
        quote! {
            #name: self.#name.combine(other.#name)
        }
    });

    let stream = quote! {
        impl crate::Combine for #name {
            fn combine(self, other: #name) -> #name {
                #name {
                    #(#combines),*
                }
            }
        }
    };
    stream.into()
}

fn get_doc_comment(attrs: &[Attribute]) -> String {
    attrs
        .iter()
        .filter_map(|attr| {
            if attr.path().is_ident("doc") {
                if let syn::Meta::NameValue(meta) = &attr.meta {
                    if let syn::Expr::Lit(expr) = &meta.value {
                        if let syn::Lit::Str(str) = &expr.lit {
                            return Some(str.value().trim().to_string());
                        }
                    }
                }
            }
            None
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn get_env_var_pattern_from_attr(attrs: &[Attribute]) -> Option<String> {
    attrs
        .iter()
        .find(|attr| attr.path().is_ident("attr_env_var_pattern"))
        .and_then(|attr| attr.parse_args::<LitStr>().ok())
        .map(|lit_str| lit_str.value())
}

fn get_added_in(attrs: &[Attribute]) -> Option<String> {
    attrs
        .iter()
        .find(|a| a.path().is_ident("attr_added_in"))
        .and_then(|attr| attr.parse_args::<LitStr>().ok())
        .map(|lit_str| lit_str.value())
}

fn is_hidden(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("attr_hidden"))
}

/// This attribute is used to generate environment variables metadata for [`uv_static::EnvVars`].
#[proc_macro_attribute]
pub fn attribute_env_vars_metadata(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as ItemImpl);

    let constants: Vec<_> = ast
        .items
        .iter()
        .filter_map(|item| match item {
            ImplItem::Const(item) if !is_hidden(&item.attrs) => {
                let doc = get_doc_comment(&item.attrs);
                let added_in = get_added_in(&item.attrs);
                let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(lit),
                    ..
                }) = &item.expr
                else {
                    return None;
                };
                let name = lit.value();
                Some((name, doc, added_in, item.ident.span()))
            }
            ImplItem::Fn(item) if !is_hidden(&item.attrs) => {
                // Extract the environment variable patterns.
                if let Some(pattern) = get_env_var_pattern_from_attr(&item.attrs) {
                    let doc = get_doc_comment(&item.attrs);
                    let added_in = get_added_in(&item.attrs);
                    Some((pattern, doc, added_in, item.sig.span()))
                } else {
                    None // Skip if pattern extraction fails.
                }
            }
            _ => None,
        })
        .collect();

    // Look for missing attr_added_in and issue a compiler error if any are found.
    let added_in_errors: Vec<_> = constants
        .iter()
        .filter_map(|(name, _, added_in, span)| {
            added_in.is_none().then_some({
                let msg = format!(
                    "missing #[attr_added_in(\"x.y.z\")] on `{name}`\nnote: env vars for an upcoming release should be annotated with `#[attr_added_in(\"next release\")]`"
                );
                quote_spanned! {*span => compile_error!(#msg); }
            })
        })
        .collect();

    if !added_in_errors.is_empty() {
        return quote! { #ast #(#added_in_errors)* }.into();
    }

    let struct_name = &ast.self_ty;
    let pairs = constants.iter().map(|(name, doc, added_in, _span)| {
        if let Some(added_in) = added_in {
            quote! { (#name, #doc, Some(#added_in)) }
        } else {
            quote! { (#name, #doc, None) }
        }
    });

    let expanded = quote! {
        #ast

        impl #struct_name {
            /// Returns a list of pairs of env var and their documentation defined in this impl block.
            pub fn metadata<'a>() -> &'a [(&'static str, &'static str, Option<&'static str>)] {
                &[#(#pairs),*]
            }
        }
    };

    expanded.into()
}

#[proc_macro_attribute]
pub fn attr_hidden(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn attr_env_var_pattern(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn attr_added_in(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
