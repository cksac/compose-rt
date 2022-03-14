use proc_macro::TokenStream;

mod compose;
use compose::transform_compose_fn;
use syn::{parse_macro_input, AttributeArgs, ItemFn};

#[proc_macro_attribute]
pub fn compose(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as AttributeArgs);
    let func: ItemFn = parse_macro_input!(item as ItemFn);
    transform_compose_fn(attr, func).into()
}
