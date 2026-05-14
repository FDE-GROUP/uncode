use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

#[proc_macro_attribute]
pub fn tool(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();

    let expanded = quote! {
        #input

        pub fn #fn_name() -> uncode_core::tool::ToolDefinition {
            uncode_core::tool::ToolDefinition {
                name: #fn_name_str.to_string(),
                description: stringify!(#fn_name).to_string(),
                parameters: serde_json::json!({}),
            }
        }
    };

    TokenStream::from(expanded)
}
