mod attributes;
mod util;

use attributes::{BoilermatesFieldAttribute, BoilermatesStructAttribute};
use heck::{ToPascalCase, ToSnakeCase};
use indexmap::IndexMap;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    parse::Parser, punctuated::Punctuated, Attribute, AttributeArgs, Data, DataStruct, DeriveInput,
    Field, Fields, FieldsNamed, Lit, NestedMeta, Token,
};

#[derive(Clone)]
struct FieldConfig {
    field: Field,
    default: bool,
}

impl FieldConfig {
    fn new(field: Field, default: bool) -> Self {
        Self { field, default }
    }

    fn name(&self) -> Ident {
        self.field
            .ident
            .clone()
            .unwrap_or_else(|| panic!("Can't get field name. This should never happen."))
    }

    fn trait_name(&self) -> Ident {
        Ident::new(
            &format!("Has{}", &self.name().to_string().to_pascal_case()),
            Span::call_site(),
        )
    }

    fn neg_trait_name(&self) -> Ident {
        Ident::new(
            &format!("HasNo{}", &self.name().to_string().to_pascal_case()),
            Span::call_site(),
        )
    }
}

impl PartialEq for FieldConfig {
    fn eq(&self, other: &Self) -> bool {
        self.name() == other.name()
    }
}

impl From<Field> for FieldConfig {
    fn from(field: Field) -> Self {
        Self::new(field, false)
    }
}

impl From<FieldConfig> for Field {
    fn from(field_config: FieldConfig) -> Self {
        field_config.field
    }
}

struct Struct {
    attrs: Vec<Attribute>,
    fields: Vec<FieldConfig>,
}

impl Struct {
    fn missing_fields_from(&self, other: &Self) -> Vec<FieldConfig> {
        self.fields.iter().fold(vec![], |mut acc, field| {
            if !other.fields.contains(field) {
                acc.push(field.clone())
            }
            acc
        })
    }

    fn same_fields_as(&self, other: &Self) -> Vec<FieldConfig> {
        self.fields.iter().fold(vec![], |mut acc, field| {
            if other.fields.contains(field) {
                acc.push(field.clone())
            }
            acc
        })
    }
}

pub fn boilermates(attrs: TokenStream2, item: TokenStream2) -> TokenStream2 {
    let mut item: DeriveInput = syn::parse2(item).unwrap();
    let attrs: AttributeArgs = Punctuated::<NestedMeta, Token![,]>::parse_terminated
        .parse2(attrs)
        .unwrap()
        .into_iter()
        .collect();

    // indexmap so the order is deterministic and predictable
    let mut structs = IndexMap::<String, Struct>::new();

    // Get the struct fields
    let Data::Struct(data_struct) = item.data.clone() else {
        panic!("Expected a struct");
    };

    let Fields::Named(mut fields) = data_struct.fields.clone() else {
        panic!("Expected a struct with named fields");
    };

    attrs.into_iter().for_each(|arg| {
        match arg {
            NestedMeta::Lit(Lit::Str(lit)) => {
                let struct_name = lit.value().trim_matches('"').to_owned();
                // new_structs.add(struct_name);
                structs.insert(
                    struct_name,
                    Struct {
                        attrs: vec![],
                        fields: vec![],
                    },
                );
            }
            _ => panic!("Expected a string literal"),
        }
    });

    for struct_attr in BoilermatesStructAttribute::extract(&mut item.attrs).unwrap() {
        match struct_attr {
            BoilermatesStructAttribute::AttrFor(attr_for) => {
                structs
                    .get_mut(&attr_for.target_struct)
                    .unwrap()
                    .attrs
                    .push(attr_for.attribute);
            }
        }
    }

    structs.insert(
        item.ident.to_string(),
        Struct {
            attrs: item.attrs.clone(),
            fields: vec![],
        },
    );

    let mut traits = quote! {};

    fields.named.iter_mut().for_each(|field| {
        let mut add_to = structs.keys().cloned().collect::<Vec<_>>();
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

        let field = FieldConfig::new(field.clone(), default);
        let trait_name = field.trait_name();
        let neg_trait_name = field.neg_trait_name();
        let field_name = field.name();
        let setter_fn = Ident::new(&format!("set_{}", field_name), Span::call_site());
        let field_ty = &field.field.ty;
        traits = quote! {
            #traits
            trait #trait_name {
                fn #field_name(&self) -> &#field_ty;
                fn #setter_fn(&mut self, value: #field_ty);
            }

            trait #neg_trait_name {}
        };

        structs.iter_mut().for_each(|(struct_name, strukt)| {
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
        });
    });

    let mut output = quote! {};
    structs.iter().for_each(|(name, strukt)| {
        let out_struct = DeriveInput {
            attrs: strukt.attrs.clone(),
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

        structs.iter().for_each(|(other_name, other)| {
            if name == other_name {
                return;
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
                    let field_ty = &field.field.ty;
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
                            let field_ty = &field.field.ty;
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
        })
    });

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
