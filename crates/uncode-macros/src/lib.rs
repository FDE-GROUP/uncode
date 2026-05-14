use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ItemFn, Pat, Type, parse_macro_input};

#[proc_macro_attribute]
pub fn tool(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();

    let description = extract_doc(&input.attrs);
    let params = extract_params(&input.sig.inputs);

    let param_names: Vec<_> = params.iter().map(|p| &p.name).collect();
    let param_types: Vec<_> = params.iter().map(|p| &p.ty).collect();
    let param_descs: Vec<_> = params.iter().map(|p| &p.description).collect();
    let required: Vec<_> = params
        .iter()
        .filter_map(|p| if p.required { Some(&p.name) } else { None })
        .collect();

    let schema_fn_name = format_ident!("__tool_schema_{}", fn_name);

    let expanded = quote! {
        #input

        pub fn #schema_fn_name() -> uncode_core::tool::ToolDefinition {
            let mut properties = serde_json::Map::new();
            #(
                properties.insert(
                    #param_names.to_string(),
                    serde_json::json!({
                        "type": #param_types,
                        "description": #param_descs
                    })
                );
            )*

            uncode_core::tool::ToolDefinition {
                name: #fn_name_str.to_string(),
                description: #description.to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": properties,
                    "required": [#(#required),*]
                }),
            }
        }
    };

    TokenStream::from(expanded)
}

fn extract_doc(attrs: &[syn::Attribute]) -> String {
    let mut lines = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let syn::Meta::NameValue(mnv) = &attr.meta {
                if let syn::Expr::Lit(expr_lit) = &mnv.value {
                    if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                        lines.push(lit_str.value());
                    }
                }
            }
        }
    }
    let doc = lines.join(" ");
    if doc.is_empty() {
        "工具函数".to_string()
    } else {
        doc.trim().to_string()
    }
}

struct ParamInfo {
    name: String,
    ty: String,
    description: String,
    required: bool,
}

fn extract_params(
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
) -> Vec<ParamInfo> {
    let mut params = Vec::new();

    for arg in inputs {
        if let FnArg::Typed(pat_type) = arg {
            let name = match &*pat_type.pat {
                Pat::Ident(id) => id.ident.to_string(),
                _ => continue,
            };

            let (inner_type, required) = unwrap_option(&pat_type.ty);

            let json_type = type_to_json_type(&inner_type);

            params.push(ParamInfo {
                name: name.clone(),
                ty: json_type,
                description: name,
                required,
            });
        }
    }

    params
}

fn unwrap_option(ty: &Type) -> (Type, bool) {
    if let Type::Path(type_path) = ty {
        if let Some(seg) = type_path.path.segments.last() {
            if seg.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(ref args) = seg.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return (inner.clone(), false);
                    }
                }
            }
        }
    }
    (ty.clone(), true)
}

fn type_to_json_type(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => {
            let name = type_path
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default();
            match name.as_str() {
                "String" | "str" => "string".into(),
                "i8" | "i16" | "i32" | "i64" | "isize" | "u8" | "u16" | "u32" | "u64" | "usize"
                | "f32" | "f64" => "number".into(),
                "bool" => "boolean".into(),
                _ => "string".into(),
            }
        }
        _ => "string".into(),
    }
}
