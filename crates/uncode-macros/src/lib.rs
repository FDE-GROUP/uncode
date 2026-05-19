use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ItemFn, Pat, Type, parse_macro_input};

#[proc_macro_attribute]
pub fn tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();

    let tool_attr = parse_tool_attr(attr);
    let description = extract_doc(&input.attrs);
    let params = extract_params(&input.sig.inputs);

    let param_names: Vec<_> = params.iter().map(|p| &p.name).collect();
    let param_types: Vec<_> = params.iter().map(|p| &p.ty).collect();
    let param_descs: Vec<_> = params.iter().map(|p| &p.description).collect();
    let required: Vec<_> = params
        .iter()
        .filter_map(|p| p.required.then_some(&p.name))
        .collect();

    let schema_fn_name = format_ident!("__tool_schema_{}", fn_name);

    let label_expr = match &tool_attr.label {
        Some(l) => quote! { Some(#l.to_string()) },
        None => quote! { None },
    };
    let exec_mode_expr = match &tool_attr.execution_mode {
        Some(m) if m == "sequential" => {
            quote! { uncode_core::tool::ExecutionMode::Sequential }
        }
        _ => quote! { uncode_core::tool::ExecutionMode::default() },
    };

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
                label: #label_expr,
                execution_mode: #exec_mode_expr,
            }
        }
    };

    TokenStream::from(expanded)
}

struct ToolAttr {
    label: Option<String>,
    execution_mode: Option<String>,
}

fn parse_tool_attr(attr: TokenStream) -> ToolAttr {
    parse_tool_attr_str(&attr.to_string())
}

fn parse_tool_attr_str(attr_str: &str) -> ToolAttr {
    let mut label = None;
    let mut execution_mode = None;

    if attr_str.is_empty() {
        return ToolAttr {
            label,
            execution_mode,
        };
    }

    for part in attr_str.split(',') {
        let part = part.trim();
        if let Some(value) = extract_kv(part, "label") {
            label = Some(value);
        } else if let Some(value) = extract_kv(part, "execution_mode") {
            execution_mode = Some(value);
        }
    }

    ToolAttr {
        label,
        execution_mode,
    }
}

fn extract_kv(input: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    if let Some(rest) = input.strip_prefix(&prefix) {
        let rest = rest.trim();
        if rest.starts_with('"') || rest.starts_with('\'') {
            let quote = rest.chars().next().unwrap();
            let rest = &rest[1..];
            let end = rest.find(quote)?;
            Some(rest[..end].to_string())
        } else {
            let end = rest.find(',').unwrap_or(rest.len());
            Some(rest[..end].trim().to_string())
        }
    } else {
        None
    }
}

fn extract_doc(attrs: &[syn::Attribute]) -> String {
    let mut lines = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("doc")
            && let syn::Meta::NameValue(mnv) = &attr.meta
            && let syn::Expr::Lit(expr_lit) = &mnv.value
            && let syn::Lit::Str(lit_str) = &expr_lit.lit
        {
            lines.push(lit_str.value());
        }
    }
    let doc = lines.join(" ");
    if doc.is_empty() {
        "工具函数".to_string()
    } else {
        doc.trim().to_string()
    }
}

#[derive(Debug)]
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
    if let Type::Path(type_path) = ty
        && let Some(seg) = type_path.path.segments.last()
        && seg.ident == "Option"
        && let syn::PathArguments::AngleBracketed(ref args) = seg.arguments
        && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
    {
        return (inner.clone(), false);
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

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_str;

    #[test]
    fn test_extract_kv_with_quoted_value() {
        let result = extract_kv(r#"label="Hello World""#, "label");
        assert_eq!(result, Some("Hello World".to_string()));
    }

    #[test]
    fn test_extract_kv_with_single_quoted_value() {
        let result = extract_kv("label='hello'", "label");
        assert_eq!(result, Some("hello".to_string()));
    }

    #[test]
    fn test_extract_kv_with_unquoted_value() {
        let result = extract_kv("execution_mode=sequential", "execution_mode");
        assert_eq!(result, Some("sequential".to_string()));
    }

    #[test]
    fn test_extract_kv_key_not_found() {
        let result = extract_kv("other=value", "label");
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_kv_empty_input() {
        let result = extract_kv("", "label");
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_kv_trailing_comma() {
        let result = extract_kv(r#"label="test","#, "label");
        assert_eq!(result, Some("test".to_string()));
    }

    #[test]
    fn test_type_to_json_type_string() {
        let ty: Type = parse_str("String").unwrap();
        assert_eq!(type_to_json_type(&ty), "string");
    }

    #[test]
    fn test_type_to_json_type_str() {
        let ty: Type = parse_str("&str").unwrap();
        assert_eq!(type_to_json_type(&ty), "string");
    }

    #[test]
    fn test_type_to_json_type_u32() {
        let ty: Type = parse_str("u32").unwrap();
        assert_eq!(type_to_json_type(&ty), "number");
    }

    #[test]
    fn test_type_to_json_type_i64() {
        let ty: Type = parse_str("i64").unwrap();
        assert_eq!(type_to_json_type(&ty), "number");
    }

    #[test]
    fn test_type_to_json_type_f64() {
        let ty: Type = parse_str("f64").unwrap();
        assert_eq!(type_to_json_type(&ty), "number");
    }

    #[test]
    fn test_type_to_json_type_bool() {
        let ty: Type = parse_str("bool").unwrap();
        assert_eq!(type_to_json_type(&ty), "boolean");
    }

    #[test]
    fn test_type_to_json_type_fallback() {
        let ty: Type = parse_str("PathBuf").unwrap();
        assert_eq!(type_to_json_type(&ty), "string");
    }

    #[test]
    fn test_unwrap_option_simple_type() {
        let ty: Type = parse_str("String").unwrap();
        let (inner, required) = unwrap_option(&ty);
        assert!(required);
        // Just check we got something back
        let _ = inner;
    }

    #[test]
    fn test_unwrap_option_type() {
        let ty: Type = parse_str("Option<String>").unwrap();
        let (inner, required) = unwrap_option(&ty);
        assert!(!required);
        let _ = inner;
    }

    #[test]
    fn test_parse_tool_attr_empty() {
        let result = parse_tool_attr_str("");
        assert!(result.label.is_none());
        assert!(result.execution_mode.is_none());
    }

    #[test]
    fn test_parse_tool_attr_with_label() {
        let result = parse_tool_attr_str(r#"label="Read File""#);
        assert_eq!(result.label, Some("Read File".to_string()));
        assert!(result.execution_mode.is_none());
    }

    #[test]
    fn test_parse_tool_attr_with_execution_mode() {
        let result = parse_tool_attr_str("execution_mode=sequential");
        assert_eq!(result.execution_mode, Some("sequential".to_string()));
        assert!(result.label.is_none());
    }

    #[test]
    fn test_parse_tool_attr_with_both() {
        let result = parse_tool_attr_str(r#"label="Write", execution_mode=sequential"#);
        assert_eq!(result.label, Some("Write".to_string()));
        assert_eq!(result.execution_mode, Some("sequential".to_string()));
    }

    #[test]
    fn test_extract_doc_with_docs() {
        use quote::quote;
        let item_fn: ItemFn = parse_str(
            &quote! {
                /// This is a test tool.
                /// It does amazing things.
                fn my_tool() {}
            }
            .to_string(),
        )
        .unwrap();
        let desc = extract_doc(&item_fn.attrs);
        assert!(desc.contains("This is a test tool."));
        assert!(desc.contains("It does amazing things."));
    }

    #[test]
    fn test_extract_doc_without_docs() {
        let item_fn: ItemFn = parse_str("fn my_tool() {}").unwrap();
        let desc = extract_doc(&item_fn.attrs);
        assert_eq!(desc, "工具函数");
    }

    #[test]
    fn test_extract_params_basic() {
        let item_fn: ItemFn = parse_str("fn greet(name: String) {}").unwrap();
        let params = extract_params(&item_fn.sig.inputs);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "name");
        assert_eq!(params[0].ty, "string");
        assert!(params[0].required);
    }

    #[test]
    fn test_extract_params_with_optional() {
        let item_fn: ItemFn = parse_str("fn greet(name: Option<String>) {}").unwrap();
        let params = extract_params(&item_fn.sig.inputs);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "name");
        assert_eq!(params[0].ty, "string");
        assert!(!params[0].required);
    }

    #[test]
    fn test_extract_params_multiple() {
        let item_fn: ItemFn = parse_str("fn f(a: String, b: u32, c: bool) {}").unwrap();
        let params = extract_params(&item_fn.sig.inputs);
        assert_eq!(params.len(), 3);
        assert_eq!(params[0].ty, "string");
        assert_eq!(params[1].ty, "number");
        assert_eq!(params[2].ty, "boolean");
    }
}
