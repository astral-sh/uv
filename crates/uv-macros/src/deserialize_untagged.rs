use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::{
    Attribute, Data, DeriveInput, Fields, Generics, Ident, LitStr, Meta, Token, Variant,
    parse_quote,
};

pub(crate) fn derive_impl(input: DeriveInput) -> syn::Result<TokenStream> {
    let Data::Enum(data) = input.data else {
        return Err(syn::Error::new_spanned(input.ident, "expected an enum"));
    };
    let name = &input.ident;
    let generics = &input.generics;
    let (_, type_generics, _) = generics.split_for_impl();

    let mut expecting = None;
    let mut deny_unknown_fields = false;
    for attribute in input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("serde"))
    {
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("expecting") {
                expecting = Some(meta.value()?.parse::<LitStr>()?.value());
            } else if meta.path.is_ident("deny_unknown_fields") {
                deny_unknown_fields = true;
            } else if !meta.path.is_ident("untagged") {
                return Err(meta.error("unsupported enum option for DeserializeUntagged"));
            }
            Ok(())
        })?;
    }

    let mut shapes = Vec::new();
    let mut methods = TokenStream::new();
    for variant in data.variants {
        let (shape, decode) = variant_decoder(name, generics, &variant, deny_unknown_fields)?;
        let (description, callbacks) = visitor_methods(&shape, &decode)?;
        if shapes.contains(&description) {
            return Err(syn::Error::new_spanned(
                &variant.ident,
                format!("more than one variant accepts {description}"),
            ));
        }
        shapes.push(description);
        methods.extend(callbacks);
    }
    if shapes.is_empty() {
        return Err(syn::Error::new_spanned(
            name,
            "expected at least one variant",
        ));
    }
    let expecting = expecting.unwrap_or_else(|| shapes.join(" or "));

    let mut de_generics = generics.clone();
    let lifetimes = generics.lifetimes().map(|param| &param.lifetime);
    de_generics
        .params
        .insert(0, parse_quote!('__de: #(#lifetimes)+*));
    for parameter in de_generics.type_params_mut() {
        parameter
            .bounds
            .push(parse_quote!(::serde::Deserialize<'__de>));
    }
    let (impl_generics, _, de_where_clause) = de_generics.split_for_impl();
    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::serde::Deserialize<'__de> for #name #type_generics #de_where_clause {
            fn deserialize<__D: ::serde::Deserializer<'__de>>(__deserializer: __D) -> ::core::result::Result<Self, __D::Error> {
                struct __Visitor<T>(::core::marker::PhantomData<fn() -> T>);

                impl #impl_generics ::serde::de::Visitor<'__de> for __Visitor<#name #type_generics> #de_where_clause {
                    type Value = #name #type_generics;

                    fn expecting(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        formatter.write_str(#expecting)
                    }

                    #methods
                }
                __deserializer.deserialize_any(__Visitor(::core::marker::PhantomData))
            }
        }
    })
}

fn variant_decoder(
    name: &Ident,
    generics: &Generics,
    variant: &Variant,
    deny_unknown_fields: bool,
) -> syn::Result<(Ident, TokenStream)> {
    let variant_name = &variant.ident;
    let mut attributes = variant
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("untagged"));
    let explicit_shape = attributes
        .next()
        .map(Attribute::parse_args::<Ident>)
        .transpose()?;
    if let Some(duplicate) = attributes.next() {
        return Err(syn::Error::new_spanned(
            duplicate,
            "expected one input shape per variant",
        ));
    }
    let mut options = serde_options(&variant.attrs)?;
    Ok(match &variant.fields {
        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
            options.extend(serde_options(&fields.unnamed[0].attrs)?);
            if let Some(option) = options.first() {
                return Err(syn::Error::new_spanned(
                    option,
                    "put Serde options on the payload type",
                ));
            }
            let shape = explicit_shape.ok_or_else(|| {
                syn::Error::new_spanned(
                    variant_name,
                    "expected #[untagged(...)] on a newtype variant",
                )
            })?;
            (
                shape,
                quote! { ::serde::Deserialize::deserialize(__input).map(#name::#variant_name) },
            )
        }
        Fields::Named(fields) => {
            if let Some(shape) = &explicit_shape
                && shape != "map"
            {
                return Err(syn::Error::new_spanned(
                    shape,
                    "a struct variant takes a map",
                ));
            }
            if let Some(option) = options
                .iter()
                .find(|option| !option.path().is_ident("rename_all"))
            {
                return Err(syn::Error::new_spanned(
                    option,
                    "unsupported struct variant option for DeserializeUntagged",
                ));
            }
            let deny_unknown_fields =
                deny_unknown_fields.then(|| quote!(#[serde(deny_unknown_fields)]));
            let (_, type_generics, where_clause) = generics.split_for_impl();
            let mut fields = fields.named.clone();
            for field in &mut fields {
                field.attrs.retain(|attr| attr.path().is_ident("serde"));
            }
            let names: Vec<_> = fields
                .iter()
                .filter_map(|field| field.ident.as_ref())
                .collect();
            // A type parameter or lifetime may only appear in a different variant.
            let mut marker = format_ident!("__marker");
            while names.contains(&&marker) {
                marker = format_ident!("{marker}_");
            }
            (
                format_ident!("map"),
                quote! {
                    #[derive(::serde::Deserialize)]
                    #deny_unknown_fields
                    #[serde(#(#options),*)]
                    struct __Fields #generics #where_clause {
                        #[serde(skip)]
                        #marker: ::core::marker::PhantomData<fn() -> #name #type_generics>,
                        #fields
                    }
                    let __Fields { #(#names,)* .. }: __Fields #type_generics =
                        ::serde::Deserialize::deserialize(__input)?;
                    ::core::result::Result::Ok(#name::#variant_name { #(#names),* })
                },
            )
        }
        Fields::Unit | Fields::Unnamed(_) => {
            return Err(syn::Error::new_spanned(
                variant,
                "expected a newtype or struct variant",
            ));
        }
    })
}

fn visitor_methods(
    shape: &Ident,
    decode: &TokenStream,
) -> syn::Result<(&'static str, TokenStream)> {
    Ok(match shape.to_string().as_str() {
        "string" => (
            "a string",
            quote! {
                fn visit_str<__E: ::serde::de::Error>(self, value: &str) -> ::core::result::Result<Self::Value, __E> {
                    let __input = ::serde::de::value::StrDeserializer::<__E>::new(value);
                    #decode
                }
                fn visit_borrowed_str<__E: ::serde::de::Error>(self, value: &'__de str) -> ::core::result::Result<Self::Value, __E> {
                    let __input = ::serde::de::value::BorrowedStrDeserializer::<__E>::new(value);
                    #decode
                }
                fn visit_string<__E: ::serde::de::Error>(self, value: ::std::string::String) -> ::core::result::Result<Self::Value, __E> {
                    let __input = ::serde::de::value::StringDeserializer::<__E>::new(value);
                    #decode
                }
            },
        ),
        "bool" => (
            "a boolean",
            quote! {
                fn visit_bool<__E: ::serde::de::Error>(self, value: bool) -> ::core::result::Result<Self::Value, __E> {
                    let __input = ::serde::de::value::BoolDeserializer::<__E>::new(value);
                    #decode
                }
            },
        ),
        "seq" => (
            "a sequence",
            quote! {
                fn visit_seq<__A: ::serde::de::SeqAccess<'__de>>(self, seq: __A) -> ::core::result::Result<Self::Value, __A::Error> {
                    let __input = ::serde::de::value::SeqAccessDeserializer::new(seq);
                    #decode
                }
            },
        ),
        // Several configuration readers can share the same table decoder.
        "map" => (
            "a map",
            quote! {
                #[inline(never)]
                fn visit_map<__A: ::serde::de::MapAccess<'__de>>(self, map: __A) -> ::core::result::Result<Self::Value, __A::Error> {
                    let __input = ::serde::de::value::MapAccessDeserializer::new(map);
                    #decode
                }
            },
        ),
        _ => {
            return Err(syn::Error::new_spanned(
                shape,
                "expected string, bool, seq, or map",
            ));
        }
    })
}

fn serde_options(attributes: &[Attribute]) -> syn::Result<Vec<Meta>> {
    let mut options = Vec::new();
    for attribute in attributes
        .iter()
        .filter(|attr| attr.path().is_ident("serde"))
    {
        options.extend(attribute.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?);
    }
    Ok(options)
}
