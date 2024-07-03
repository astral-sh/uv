use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Error, Field, Fields, Token};

pub(crate) fn derive_cache_key(item: &DeriveInput) -> syn::Result<TokenStream> {
    let fields = match &item.data {
        Data::Enum(item_enum) => {
            let arms = item_enum.variants.iter().enumerate().map(|(i, variant)| {
                let variant_name = &variant.ident;

                match &variant.fields {
                    Fields::Named(fields) => {
                        let field_names: Vec<_> = fields
                            .named
                            .iter()
                            .map(|field| field.ident.clone().unwrap())
                            .collect();

                        let fields_code = field_names
                            .iter()
                            .map(|field| quote!(#field.cache_key(key);));

                        quote! {
                            Self::#variant_name{#(#field_names),*} => {
                                key.write_usize(#i);
                                #(#fields_code)*
                            }
                        }
                    }
                    Fields::Unnamed(fields) => {
                        let field_names: Vec<_> = fields
                            .unnamed
                            .iter()
                            .enumerate()
                            .map(|(i, _)| format_ident!("field_{i}"))
                            .collect();

                        let fields_code = field_names
                            .iter()
                            .map(|field| quote!(#field.cache_key(key);));

                        quote! {
                            Self::#variant_name(#(#field_names),*) => {
                                key.write_usize(#i);
                                #(#fields_code)*
                            }
                        }
                    }
                    Fields::Unit => {
                        quote! {
                            Self::#variant_name => {
                                key.write_usize(#i);
                            }
                        }
                    }
                }
            });

            quote! {
                match self {
                    #(#arms)*
                }
            }
        }

        Data::Struct(item_struct) => {
            let mut fields = Vec::with_capacity(item_struct.fields.len());

            for (i, field) in item_struct.fields.iter().enumerate() {
                if let Some(cache_field_attribute) = cache_key_field_attribute(field)? {
                    if cache_field_attribute.ignore {
                        continue;
                    }
                }

                let field_attr = match &field.ident {
                    Some(ident) => quote!(self.#ident),
                    None => {
                        let index = syn::Index::from(i);
                        quote!(self.#index)
                    }
                };

                fields.push(quote!(#field_attr.cache_key(key);));
            }

            quote! {#(#fields)*}
        }

        Data::Union(_) => {
            return Err(Error::new(
                item.span(),
                "CacheKey does not support unions. Only structs and enums are supported",
            ))
        }
    };

    let name = &item.ident;
    let (impl_generics, ty_generics, where_clause) = &item.generics.split_for_impl();

    Ok(quote!(
        #[automatically_derived]
        impl #impl_generics cache_key::CacheKey for #name #ty_generics #where_clause {
            fn cache_key(&self, key: &mut cache_key::CacheKeyHasher) {
                use std::hash::Hasher;
                use cache_key::CacheKey;
                #fields
            }
        }
    ))
}

fn cache_key_field_attribute(field: &Field) -> syn::Result<Option<CacheKeyFieldAttributes>> {
    if let Some(attribute) = field
        .attrs
        .iter()
        .find(|attribute| attribute.path().is_ident("cache_key"))
    {
        attribute.parse_args::<CacheKeyFieldAttributes>().map(Some)
    } else {
        Ok(None)
    }
}

#[derive(Debug, Default)]
struct CacheKeyFieldAttributes {
    ignore: bool,
}

impl Parse for CacheKeyFieldAttributes {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut attributes = CacheKeyFieldAttributes::default();

        let args = input.parse_terminated(Ident::parse, Token![,])?;

        for arg in args {
            match arg.to_string().as_str() {
                "ignore" => {
                    attributes.ignore = true;
                }
                name => {
                    return Err(Error::new(
                        arg.span(),
                        format!("Unknown `cache_field` argument {name}"),
                    ))
                }
            }
        }

        Ok(attributes)
    }
}
