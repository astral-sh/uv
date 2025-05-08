use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

#[proc_macro_derive(TraversableError)]
pub fn derive_traversable_error(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let name_impl = match input.data {
        Data::Enum(ref enum_data) => {
            let match_arms = enum_data.variants.iter().map(|variant| {
                let variant_name = &variant.ident;
                let field_pattern = match &variant.fields {
                    Fields::Named(_) => quote! { { .. } },
                    Fields::Unnamed(_) => quote! { (..) },
                    Fields::Unit => quote! {},
                };

                quote! {
                    #name::#variant_name #field_pattern => {
                        concat!(stringify!(#name), "::", stringify!(#variant_name))
                    }
                }
            });

            quote! {
                fn name(&self) -> &str {
                    match self {
                        #(#match_arms,)*
                    }
                }
            }
        }
        _ => {
            quote! {
                fn name(&self) -> &str {
                    stringify!(#name)
                }
            }
        }
    };

    let expanded = quote! {
        impl ::traversable_error::anyhow::TraversableError for #name {
            #name_impl
        }
    };

    expanded.into()
}
