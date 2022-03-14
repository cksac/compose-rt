use darling::FromMeta;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemFn, NestedMeta};

#[derive(Debug, FromMeta)]
pub struct MacroArgs {
    #[darling(default)]
    skip_inject_cx: bool,
}


pub fn transform_compose_fn(attr: Vec<NestedMeta>, f: ItemFn) -> TokenStream {
    let fn_args = &f.sig.inputs;

    let macro_args = match MacroArgs::from_list(&attr) {
        Ok(v) => v,
        Err(e) => { return TokenStream::from(e.write_errors()); }
    };

    let compose_fn = if macro_args.skip_inject_cx {
        quote! {
            #[track_caller]
            #f
        }
    } else {
        let fn_name = &f.sig.ident;
        let fn_generics = &f.sig.generics.params;
        let fn_return = &f.sig.output;
        let fn_where = &f.sig.generics.where_clause;
        let fn_block = &f.block;

        quote! {
            #[track_caller]
            fn #fn_name<#fn_generics>(cx: &mut compose_rt::Composer, #fn_args) #fn_return
            #fn_where 
            #fn_block
        }
    };

    compose_fn
}
