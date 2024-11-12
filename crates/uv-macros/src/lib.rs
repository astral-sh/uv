mod options_metadata;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Attribute, DeriveInput, ImplItem, ItemImpl, LitStr};

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

    let gen = quote! {
        impl crate::Combine for #name {
            fn combine(self, other: #name) -> #name {
                #name {
                    #(#combines),*
                }
            }
        }
    };
    gen.into()
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
                let name = item.ident.to_string();
                let doc = get_doc_comment(&item.attrs);
                Some((name, doc))
            }
            ImplItem::Fn(item) if !is_hidden(&item.attrs) => {
                // Extract the environment variable patterns.
                if let Some(pattern) = get_env_var_pattern_from_attr(&item.attrs) {
                    let doc = get_doc_comment(&item.attrs);
                    Some((pattern, doc))
                } else {
                    None // Skip if pattern extraction fails.
                }
            }
            _ => None,
        })
        .collect();

    let struct_name = &ast.self_ty;
    let pairs = constants.iter().map(|(name, doc)| {
        quote! {
            (#name, #doc)
        }
    });

    let expanded = quote! {
        #ast

        impl #struct_name {
            /// Returns a list of pairs of env var and their documentation defined in this impl block.
            pub fn metadata<'a>() -> &'a [(&'static str, &'static str)] {
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
