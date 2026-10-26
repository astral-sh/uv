use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::{Data, DeriveInput, Fields, parse_quote};

pub(crate) fn derive_impl(input: DeriveInput) -> syn::Result<TokenStream> {
    let data = match input.data {
        Data::Enum(data) => data,
        Data::Struct(_) | Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                input.ident,
                "DebugNoInline can only be derived for enums",
            ));
        }
    };

    let name = input.ident;
    let mut generics = input.generics;
    for parameter in generics.type_params_mut() {
        parameter.bounds.push(parse_quote!(::core::fmt::Debug));
    }
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    let arms = data.variants.iter().map(|variant| {
        let variant_name = &variant.ident;
        let label = variant_name.unraw().to_string();
        let bindings: Vec<_> = (0..variant.fields.len())
            .map(|index| format_ident!("__field_{index}"))
            .collect();

        match &variant.fields {
            Fields::Unit => quote! {
                Self::#variant_name => __formatter.write_str(#label)
            },
            Fields::Unnamed(_) => quote! {
                Self::#variant_name(#(#bindings),*) => __formatter
                    .debug_tuple(#label)
                    #(.field(#bindings))*
                    .finish()
            },
            Fields::Named(fields) => {
                let names: Vec<_> = fields
                    .named
                    .iter()
                    .filter_map(|field| field.ident.as_ref())
                    .collect();
                let labels = names.iter().map(|name| name.unraw().to_string());
                quote! {
                    Self::#variant_name { #(#names: #bindings),* } => __formatter
                        .debug_struct(#label)
                        #(.field(#labels, #bindings))*
                        .finish()
                }
            }
        }
    });

    let body = if data.variants.is_empty() {
        quote! { match *self {} }
    } else {
        quote! { match self { #(#arms),* } }
    };

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::core::fmt::Debug for #name #type_generics #where_clause {
            #[inline(never)]
            fn fmt(&self, __formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                #body
            }
        }
    })
}
