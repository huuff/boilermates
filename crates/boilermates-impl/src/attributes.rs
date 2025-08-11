use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_quote, Attribute, Lit, Meta, MetaList, NestedMeta};

nestify::nest! {
  pub enum BoilermatesStructAttribute {
    AttrFor(pub struct BoilermatesAttrFor {
      pub target_struct: String,
      pub attribute: Attribute,
    })
  }
}

impl BoilermatesStructAttribute {
    pub fn extract(attributes: &mut Vec<Attribute>) -> Result<Vec<Self>, anyhow::Error> {
        use itertools::Itertools as _;

        let (boilermates_attrs, non_boilermates_attrs) = std::mem::take(attributes)
            .into_iter()
            .partition(is_boilermates);

        *attributes = non_boilermates_attrs;

        boilermates_attrs
            .into_iter()
            .map(|attr| match attr.parse_meta()? {
                Meta::List(MetaList { nested, .. }) => match nested.first() {
                    Some(NestedMeta::Meta(Meta::List(attr)))
                        if attr.path.get_ident().is_some_and(|it| it == "attr_for") =>
                    {
                        Ok(BoilermatesStructAttribute::AttrFor(
                            BoilermatesAttrFor::try_from(attr.clone())?,
                        ))
                    }
                    _ => Err(anyhow::anyhow!("unknown boilermates attribute")),
                },
                _ => Err(anyhow::anyhow!("unknown boilermates attribute")),
            })
            .try_collect()
    }
}

fn is_boilermates(attr: &Attribute) -> bool {
    match attr.parse_meta() {
        Ok(Meta::List(list)) if list.path.get_ident().is_some_and(|it| it == "boilermates") => true,
        _ => false,
    }
}

impl TryFrom<MetaList> for BoilermatesAttrFor {
    type Error = anyhow::Error;

    fn try_from(list_attr: MetaList) -> Result<Self, Self::Error> {
        if list_attr.nested.len() != 2 {
            anyhow::bail!("`#[boilermates(attr_for(...))]` must have two string literal arguments");
        }

        let mut list_iter = list_attr.nested.into_iter();

        match (
            list_iter.next().expect("we just checked length is 2"),
            list_iter.next().expect("we just checked length is 2"),
        ) {
            (NestedMeta::Lit(Lit::Str(struct_name)), NestedMeta::Lit(Lit::Str(attr_list))) => {
                Ok(Self {
                    target_struct: struct_name.value().trim_matches('"').into(),
                    attribute: {
                        let token_stream: TokenStream = attr_list
                            .value()
                            .trim_matches('"')
                            .parse::<TokenStream>()
                            .map_err(|_| anyhow::anyhow!("can't parse attr"))?;
                        let q = quote! { #token_stream };
                        parse_quote! { #q }
                    },
                })
            }
            _ => anyhow::bail!(
                "`#[boilermates(attr_for(...))]` must have two string literal arguments"
            ),
        }
    }
}

nestify::nest! {
  pub enum BoilermatesFieldAttribute {
    OnlyIn(pub struct BoilermatesOnlyIn(pub Vec<String>)),
    NotIn(pub struct BoilermatesNotIn(pub Vec<String>)),
    Default,
    OnlyInSelf,
  }
}

impl BoilermatesFieldAttribute {
    pub fn extract(
        attributes: &mut Vec<Attribute>,
    ) -> Result<Vec<BoilermatesFieldAttribute>, anyhow::Error> {
        use itertools::Itertools as _;

        let (boilermates_attrs, non_boilermates_attrs) = std::mem::take(attributes)
            .into_iter()
            .partition(is_boilermates);

        *attributes = non_boilermates_attrs;

        boilermates_attrs
            .into_iter()
            .map(|attr| match attr.parse_meta()? {
                Meta::List(MetaList { nested, .. }) => match nested.first() {
                    Some(NestedMeta::Meta(Meta::List(attr))) => match attr
                        .path
                        .get_ident()
                        .map(|ident| ident.to_string())
                        .as_deref()
                    {
                        Some("only_in") => Ok(BoilermatesFieldAttribute::OnlyIn(
                            BoilermatesOnlyIn(extract_nested_list(attr)),
                        )),
                        Some("not_in") => Ok(BoilermatesFieldAttribute::NotIn(BoilermatesNotIn(
                            extract_nested_list(attr),
                        ))),
                        _ => Err(anyhow::anyhow!("unknown boilermates attribute")),
                    },
                    Some(NestedMeta::Meta(Meta::Path(path))) => {
                        match path.get_ident().map(|ident| ident.to_string()).as_deref() {
                            Some("default") => Ok(BoilermatesFieldAttribute::Default),
                            Some("only_in_self") => Ok(BoilermatesFieldAttribute::OnlyInSelf),
                            _ => anyhow::bail!("unknown boilermates attribute"),
                        }
                    }
                    _ => Err(anyhow::anyhow!("unknown boilermates attribute")),
                },
                _ => Err(anyhow::anyhow!("unknown boilermates attribute")),
            })
            .try_collect()
    }
}

fn extract_nested_list(meta_list: &MetaList) -> Vec<String> {
    meta_list
        .nested
        .iter()
        .map(|n| match n {
            NestedMeta::Lit(Lit::Str(lit)) => lit.value().trim_matches('"').to_owned(),
            _ => panic!("Expected a string literal"),
        })
        .collect()
}
