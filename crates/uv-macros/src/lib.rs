mod options_metadata;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Attribute, DeriveInput, ImplItem, ItemImpl};

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

fn get_doc_comment(attr: &Attribute) -> Option<String> {
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
}

#[proc_macro_attribute]
pub fn attribute_env_vars_metadata(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as ItemImpl);

    let constants: Vec<_> = ast
        .items
        .iter()
        .filter_map(|item| match item {
            ImplItem::Const(item)
                if !item
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("attr_hidden")) =>
            {
                let name = item.ident.to_string();
                let doc = item
                    .attrs
                    .iter()
                    .find_map(get_doc_comment)
                    .expect("Missing doc comment");
                Some((name, doc))
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
            /// Returns a list of pairs of constants and their documentation defined in this impl block.
            pub fn constants<'a>() -> &'a [(&'static str, &'static str)] {
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
