#[cfg(test)]
pub fn pretty_print(token_stream: proc_macro2::TokenStream) -> String {
    prettyplease::unparse(&syn::parse_file(&token_stream.to_string()).unwrap())
}
