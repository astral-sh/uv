mod options_metadata;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

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
