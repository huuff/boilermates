use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn boilermates(attrs: TokenStream, item: TokenStream) -> TokenStream {
    
   boilermates_impl::boilermates(attrs.into(), item.into()).into()
}