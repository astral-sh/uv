mod options_metadata;

use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Attribute, DeriveInput, ImplItem, ItemImpl, LitStr, parse_macro_input};

#[proc_macro_derive(OptionsMetadata, attributes(option, option_group))]
pub fn derive_options_metadata(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    options_metadata::derive_impl(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(PreviewMetadata, attributes(preview))]
pub fn derive_preview_metadata(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    impl_preview_metadata(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn impl_preview_metadata(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;

    let syn::Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            name,
            "PreviewMetadata can only be derived for enums",
        ));
    };

    let entries = data
        .variants
        .iter()
        .map(|variant| {
            let documentation = get_doc_comment(&variant.attrs);
            if documentation.is_empty() {
                return Err(syn::Error::new_spanned(
                    variant,
                    "PreviewMetadata variants must have documentation",
                ));
            }

            let mut aliases = Vec::new();
            for attribute in variant
                .attrs
                .iter()
                .filter(|attribute| attribute.path().is_ident("preview"))
            {
                attribute.parse_nested_meta(|meta| {
                    if meta.path.is_ident("alias") {
                        aliases.push(meta.value()?.parse::<LitStr>()?);
                        Ok(())
                    } else {
                        Err(meta.error("expected `alias`"))
                    }
                })?;
            }

            let variant_name = &variant.ident;
            Ok(quote! { (Self::#variant_name, #documentation, &[#(#aliases),*]) })
        })
        .collect::<syn::Result<Vec<_>>>()?;

    Ok(quote! {
        impl #name {
            /// Returns each enum variant, its documentation, and its aliases.
            pub const fn metadata() -> &'static [(Self, &'static str, &'static [&'static str])] {
                &[#(#entries),*]
            }
        }
    })
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
            if attr.path().is_ident("doc")
                && let syn::Meta::NameValue(meta) = &attr.meta
                && let syn::Expr::Lit(expr) = &meta.value
                && let syn::Lit::Str(str) = &expr.lit
            {
                return Some(str.value().trim().to_string());
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

fn is_valid_added_in(added_in: &str) -> bool {
    added_in == "next release" || is_semantic_version(added_in)
}

fn is_semantic_version(version: &str) -> bool {
    let mut components = version.split('.');
    let Some(major) = components.next() else {
        return false;
    };
    let Some(minor) = components.next() else {
        return false;
    };
    let Some(patch) = components.next() else {
        return false;
    };

    if components.next().is_some() {
        return false;
    }

    [major, minor, patch].into_iter().all(|component| {
        !component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit())
    })
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
                get_env_var_pattern_from_attr(&item.attrs).map(|pattern| {
                    // Extract the environment variable patterns.
                    let doc = get_doc_comment(&item.attrs);
                    let added_in = get_added_in(&item.attrs);
                    (pattern, doc, added_in, item.sig.span())
                })
            }
            _ => None,
        })
        .collect();

    // Look for missing or invalid attr_added_in values and issue a compiler error if any are found.
    let added_in_errors: Vec<_> = constants
        .iter()
        .filter_map(|(name, _, added_in, span)| {
            let msg = match added_in {
                None => format!(
                    "missing #[attr_added_in(\"x.y.z\")] on `{name}`\nnote: env vars for an upcoming release should be annotated with `#[attr_added_in(\"next release\")]`"
                ),
                Some(added_in) if !is_valid_added_in(added_in) => format!(
                    "invalid #[attr_added_in(\"{added_in}\")] on `{name}`\nnote: expected `#[attr_added_in(\"x.y.z\")]` or `#[attr_added_in(\"next release\")]`"
                ),
                Some(_) => return None,
            };
            Some(quote_spanned! {*span => compile_error!(#msg); })
        })
        .collect();

    if !added_in_errors.is_empty() {
        return quote! { #ast #(#added_in_errors)* }.into();
    }

    let env_var_names = ast.items.iter().filter_map(|item| {
        if let ImplItem::Const(item) = item {
            let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(lit),
                ..
            }) = &item.expr
            else {
                return None;
            };
            Some(lit.value())
        } else {
            None
        }
    });

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

            /// Returns all environment variable names defined as constants (including hidden ones).
            pub fn all_names() -> &'static [&'static str] {
                &[#(#env_var_names),*]
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
