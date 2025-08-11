mod attributes;
mod model;
mod util;

use attributes::{BoilermatesFieldAttribute, BoilermatesStructAttribute};
use heck::ToSnakeCase;
use itertools::Itertools;
use model::{OutputField, OutputStructs};
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    Data, DataStruct, DeriveInput, Field,
    Fields, FieldsNamed,
};

pub fn boilermates(attrs: TokenStream2, item: TokenStream2) -> TokenStream2 {
    let mut item: DeriveInput = syn::parse2(item).unwrap();

    // XXX: must do this first to remove all boilermates attributes before initializing the output
    let stuct_attrs = BoilermatesStructAttribute::extract(&mut item.attrs).unwrap();
    let mut structs = OutputStructs::initialize(&item, attrs).unwrap();

    let Data::Struct(data_struct) = item.data.clone() else {
        panic!("Expected a struct");
    };

    let Fields::Named(mut fields) = data_struct.fields.clone() else {
        panic!("Expected a struct with named fields");
    };

    for struct_attr in stuct_attrs {
        match struct_attr {
            BoilermatesStructAttribute::AttrFor(attr_for) => {
                structs
                    .get_mut(&attr_for.target_struct)
                    .unwrap()
                    .attributes
                    .push(attr_for.attribute);
            }
        }
    }

    let mut traits = quote! {};

    fields.named.iter_mut().for_each(|field| {
        let mut add_to = structs.names().map(<_>::to_owned).collect_vec();
        let mut default = false;

        for boilermates_attr in BoilermatesFieldAttribute::extract(&mut field.attrs).unwrap() {
            match boilermates_attr {
                BoilermatesFieldAttribute::OnlyIn(only_in) => add_to = only_in.0,
                BoilermatesFieldAttribute::NotIn(not_in) => {
                    add_to.retain(|strukt| !not_in.0.contains(&strukt))
                }
                BoilermatesFieldAttribute::Default => default = true,
                BoilermatesFieldAttribute::OnlyInSelf => add_to = vec![item.ident.to_string()],
            }
        }

        let field = OutputField::new(field.clone(), default);
        let trait_name = field.trait_name();
        let neg_trait_name = field.neg_trait_name();
        let field_name = field.name();
        let setter_fn = Ident::new(&format!("set_{}", field_name), Span::call_site());
        let field_ty = &field.definition.ty;
        traits = quote! {
            #traits
            trait #trait_name {
                fn #field_name(&self) -> &#field_ty;
                fn #setter_fn(&mut self, value: #field_ty);
            }

            trait #neg_trait_name {}
        };

        for (struct_name, strukt) in &mut structs {
            // MAYBE we don't need to store names and strings and can store idents directly?
            let struct_ident = Ident::new(struct_name, Span::call_site());

            if add_to.contains(struct_name) {
                strukt.fields.push(field.clone());

                traits = quote! {
                    #traits
                    impl #trait_name for #struct_ident {
                        fn #field_name(&self) -> &#field_ty {
                            &self.#field_name
                        }

                        fn #setter_fn(&mut self, value: #field_ty) {
                            self.#field_name = value;
                        }
                    }
                };
            } else {
                traits = quote! {
                    #traits
                    impl #neg_trait_name for #struct_ident {}
                };
            }
        }
    });

    let mut output = quote! {};
    for (name, strukt) in &structs {
        let out_struct = DeriveInput {
            attrs: strukt.attributes.clone(),
            data: Data::Struct(DataStruct {
                fields: Fields::Named(FieldsNamed {
                    named: strukt
                        .fields
                        .iter()
                        .cloned()
                        .map(Into::<Field>::into)
                        .collect(),
                    ..fields
                }),
                ..data_struct
            }),
            ident: Ident::new(name, Span::call_site()),
            ..item.clone()
        };
        output = quote! {
            #output
            #out_struct
        };

        for (other_name, other) in &structs {
            if name == other_name {
                continue;
            }
            let name = Ident::new(name, Span::call_site());
            let other_name = Ident::new(other_name, Span::call_site());
            let missing_fields = strukt.missing_fields_from(other);
            let missing_fields_without_defaults = missing_fields
                .iter()
                .filter(|f| !f.default)
                .collect::<Vec<_>>();

            let default_field_setters =
                missing_fields
                    .iter()
                    .filter(|f| f.default)
                    .fold(quote! {}, |acc, field| {
                        let field_name = field.name();
                        quote! {
                            #acc
                            #field_name: Default::default(),
                        }
                    });

            if missing_fields_without_defaults.is_empty() {
                let common_field_setters =
                    strukt
                        .same_fields_as(other)
                        .iter()
                        .fold(quote! {}, |acc, field| {
                            let field_name = &field.name();
                            quote! {
                                #acc
                                #field_name: other.#field_name,
                            }
                        });

                output = quote! {
                    #output
                    impl From<#other_name> for #name {
                        fn from(other: #other_name) -> Self {
                            Self {
                                #common_field_setters
                                #default_field_setters
                            }
                        }
                    }
                };
            }
            if !missing_fields.is_empty() {
                let common_field_setters =
                    strukt
                        .same_fields_as(other)
                        .iter()
                        .fold(quote! {}, |acc, field| {
                            let field_name = field.name();
                            quote! {
                                #acc
                                #field_name: self.#field_name,
                            }
                        });

                let into_args = missing_fields.iter().fold(quote! {}, |acc, field| {
                    let field_name = field.name();
                    let field_ty = &field.definition.ty;
                    quote! {
                        #acc
                        #field_name: #field_ty,
                    }
                });

                let into_defaults_args =
                    missing_fields_without_defaults
                        .iter()
                        .fold(quote! {}, |acc, field| {
                            let field_name = field.name();
                            let field_ty = &field.definition.ty;
                            quote! {
                                #acc
                                #field_name: #field_ty,
                            }
                        });

                let into_missing_setters = missing_fields.iter().fold(quote! {}, |acc, field| {
                    let field_name = field.name();
                    quote! { #acc #field_name, }
                });

                let into_defaults_missing_setters =
                    missing_fields_without_defaults
                        .iter()
                        .fold(quote! {}, |acc, field| {
                            let field_name = field.name();
                            quote! { #acc #field_name, }
                        });

                let into_defaults_fn_name = Ident::new(
                    &format!("into{}_defaults", name).to_snake_case(),
                    Span::call_site(),
                );

                let into_fn_name =
                    Ident::new(&format!("into{}", name).to_snake_case(), Span::call_site());

                output = quote! {
                    #output
                    impl #other_name {
                        pub fn #into_fn_name(self, #into_args) -> #name {
                            #name {
                                #common_field_setters
                                #into_missing_setters
                            }
                        }

                        pub fn #into_defaults_fn_name(self, #into_defaults_args) -> #name {
                            #name {
                                #common_field_setters
                                #default_field_setters
                                #into_defaults_missing_setters
                            }
                        }
                    }
                };
            }
        }
    }

    output = quote! {
        #output
        #traits
    };

    output.into()
}

#[cfg(test)]
mod test {
    use crate::util::pretty_print;

    use super::*;
    use quote::quote;

    #[test]
    fn snapshot_test() {
        let output = boilermates(
            quote! { "StructWithX", "StructWithoutY" },
            quote! {
              pub struct MainStruct {
                pub field: String,
                #[boilermates(only_in = "StructWithX")]
                pub x: u32,
                #[boilermates(not_in = "StructWithoutY")]
                pub y: i32,
              }
            },
        );

        insta::assert_snapshot!(pretty_print(output));
    }
}
