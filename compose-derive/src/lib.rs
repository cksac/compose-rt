mod compose;

use crate::compose::{transform_compose_fn, MacroArgs};
use darling::FromMeta;
use proc_macro::TokenStream;
use syn::{parse_macro_input, AttributeArgs, ItemFn};

#[proc_macro_attribute]
pub fn compose(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as AttributeArgs);
    let macro_args = match MacroArgs::from_list(&attr) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(e.write_errors());
        }
    };

    let func: ItemFn = parse_macro_input!(item as ItemFn);
    transform_compose_fn(macro_args, func).into()
}
