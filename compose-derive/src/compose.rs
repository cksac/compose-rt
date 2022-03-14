use darling::FromMeta;
use proc_macro2::TokenStream;
use quote::quote;
use syn::ItemFn;

#[derive(Debug, FromMeta)]
pub struct MacroArgs {
    #[darling(default)]
    skip_inject_cx: bool,
}

pub fn transform_compose_fn(macro_args: MacroArgs, func: ItemFn) -> TokenStream {
    let fn_args = &func.sig.inputs;

    if macro_args.skip_inject_cx {
        quote! {
            #[track_caller]
            #func
        }
    } else {
        let fn_name = &func.sig.ident;
        let fn_generics = &func.sig.generics.params;
        let fn_return = &func.sig.output;
        let fn_where = &func.sig.generics.where_clause;
        let fn_block = &func.block;

        quote! {
            #[track_caller]
            fn #fn_name<#fn_generics>(cx: &mut compose_rt::Composer, #fn_args) #fn_return
            #fn_where
            #fn_block
        }
    }
}
