use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use uncode_core::tool::{ExecutionMode, ToolDefinition, ToolExecutor};
use uncode_ontology::TypeRegistry;
use uncode_ontology::builtin::full_ontology;

use crate::decision::bridge::ToolBridge;

/// Origin of a registered tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSource {
    Builtin,
    Extension,
}

/// Reserved builtin tool names that extensions cannot override.
const RESERVED_TOOL_NAMES: &[&str] = &[
    "read",
    "write",
    "edit",
    "bash",
    "grep",
    "find",
    "ls",
    "web_fetch",
    "web_search",
    "llm_query",
];

pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<dyn ToolExecutor>>>,
    sources: RwLock<HashMap<String, ToolSource>>,
    active_names: RwLock<Option<HashSet<String>>>,
    /// Optional ontology reference — when set, `definitions()` generates
    /// ToolDefinitions from ActionDefs instead of calling executor.definition().
    ontology: RwLock<Option<Arc<TypeRegistry>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::with_capacity(8)),
            sources: RwLock::new(HashMap::new()),
            active_names: RwLock::new(None),
            ontology: RwLock::new(None),
        }
    }

    /// Attach an ontology to drive ToolDefinition generation from ActionDefs.
    ///
    /// When set, `definitions()` prefers ontological definitions over
    /// hand-written `executor.definition()`.  When a tool has no matching
    /// `ActionDef`, the executor's own definition is used as fallback.
    pub fn set_ontology(&self, registry: Arc<TypeRegistry>) {
        *self.ontology.write() = Some(registry);
    }

    /// Convenience: use the built-in full ontology (domain + system).
    pub fn set_builtin_ontology(&self) {
        self.set_ontology(Arc::new(full_ontology()));
    }

    /// Restrict which tools are sent to the LLM and allowed to execute.
    ///
    /// **Pi:** `setActiveTools` — last call wins; unknown names are rejected.
    pub fn set_active_tools(&self, names: &[impl AsRef<str>]) -> Result<(), String> {
        let tools = self.tools.read();
        for name in names {
            let n = name.as_ref();
            if !tools.contains_key(n) {
                return Err(format!("unknown tool: {n}"));
            }
        }
        let set: HashSet<String> = names.iter().map(|n| n.as_ref().to_string()).collect();
        *self.active_names.write() = Some(set);
        Ok(())
    }

    /// Clear the active filter — all registered tools are visible and executable.
    pub fn clear_active_tools(&self) {
        *self.active_names.write() = None;
    }

    /// Whether a registered tool is in the active set (or all registered tools when no filter).
    ///
    /// Extension tools always bypass the active filter — they are always visible.
    pub fn is_active(&self, name: &str) -> bool {
        if !self.tools.read().contains_key(name) {
            return false;
        }
        if self.sources.read().get(name) == Some(&ToolSource::Extension) {
            return true;
        }
        match self.active_names.read().as_ref() {
            None => true,
            Some(active) => active.contains(name),
        }
    }

    pub fn active_tool_names(&self) -> Option<Vec<String>> {
        self.active_names
            .read()
            .as_ref()
            .map(|s| s.iter().cloned().collect())
    }

    pub fn register(&self, name: impl Into<String>, tool: Arc<dyn ToolExecutor>) {
        let name = name.into();
        self.tools.write().insert(name.clone(), tool);
        self.sources.write().insert(name, ToolSource::Builtin);
    }

    /// Register an extension tool. Validates name is not a reserved builtin.
    pub fn register_extension_tool(
        &self,
        name: impl Into<String>,
        tool: Arc<dyn ToolExecutor>,
    ) -> Result<(), String> {
        let name = name.into();
        if RESERVED_TOOL_NAMES.contains(&name.as_str()) {
            return Err(format!(
                "cannot register extension tool with reserved name: {name}"
            ));
        }
        {
            let tools = self.tools.read();
            if tools.contains_key(&name) {
                return Err(format!("tool already registered: {name}"));
            }
        }
        self.tools.write().insert(name.clone(), tool);
        self.sources.write().insert(name, ToolSource::Extension);
        Ok(())
    }

    /// Query the source of a registered tool.
    pub fn source(&self, name: &str) -> Option<ToolSource> {
        self.sources.read().get(name).copied()
    }

    /// Unregister a tool (for hot-reload support).
    pub fn unregister(&self, name: &str) -> bool {
        let removed = self.tools.write().remove(name).is_some();
        self.sources.write().remove(name);
        removed
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn ToolExecutor>> {
        self.tools.read().get(name).cloned()
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        let tools = self.tools.read();
        let active = self.active_names.read();
        let sources = self.sources.read();
        let ontology = self.ontology.read();

        tools
            .iter()
            .filter(|(name, _)| {
                if sources.get(*name) == Some(&ToolSource::Extension) {
                    return true;
                }
                match active.as_ref() {
                    None => true,
                    Some(set) => set.contains(*name),
                }
            })
            .map(|(name, t)| {
                // Prefer ontology ActionDef over hand-written executor definition
                if let Some(ref reg) = *ontology {
                    if let Some(action) =
                        reg.get_action(&uncode_ontology::TypeId::from(name.as_str()))
                    {
                        return ToolBridge::to_tool_definition(
                            &action,
                            t.definition().label.as_deref(),
                        );
                    }
                }
                t.definition()
            })
            .collect()
    }

    pub fn list(&self) -> Vec<String> {
        self.tools.read().keys().cloned().collect()
    }

    /// Registered tool names that are currently active (all registered when no filter).
    pub fn list_active(&self) -> Vec<String> {
        let tools = self.tools.read();
        let active = self.active_names.read();
        let sources = self.sources.read();
        tools
            .keys()
            .filter(|name| {
                if sources.get(*name) == Some(&ToolSource::Extension) {
                    return true;
                }
                match active.as_ref() {
                    None => true,
                    Some(set) => set.contains(*name),
                }
            })
            .cloned()
            .collect()
    }

    /// Get the execution mode for a named tool (defaults to Parallel)
    pub fn execution_mode(&self, name: &str) -> ExecutionMode {
        self.tools
            .read()
            .get(name)
            .map(|t| t.definition().execution_mode)
            .unwrap_or_default()
    }

    /// Get a tool's display label (falls back to name)
    pub fn label_for(&self, name: &str) -> String {
        self.tools
            .read()
            .get(name)
            .map(|t| {
                t.definition()
                    .label
                    .clone()
                    .unwrap_or_else(|| t.definition().name.clone())
            })
            .unwrap_or_else(|| name.to_string())
    }

    /// Check if all named tools can run in parallel
    pub fn can_run_parallel(&self, tool_names: &[String]) -> bool {
        tool_names
            .iter()
            .all(|name| self.execution_mode(name) == ExecutionMode::Parallel)
    }

    /// **Pi:** `prepareToolCallArguments` then `validateToolArguments`.
    pub fn prepare_and_validate(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let Some(exec) = self.get(name) else {
            return Err(format!("tool not found: {name}"));
        };
        let mut prepared = exec
            .prepare_arguments(args)
            .map_err(|e| format!("argument error: {e}"))?;
        if let Some(tool) = self.get(name) {
            coerce_args_against_schema(&tool.definition().parameters, &mut prepared);
        }
        self.validate(name, &prepared).map_err(|e| {
            let pretty =
                serde_json::to_string_pretty(&prepared).unwrap_or_else(|_| prepared.to_string());
            format!("Validation failed for tool \"{name}\": {e}\n\nReceived arguments:\n{pretty}")
        })?;
        Ok(prepared)
    }

    /// Validate tool arguments against the tool's JSON Schema parameters.
    ///
    /// **Pi:** `validateToolArguments` — runs after `prepareArguments`, before `beforeToolCall`.
    #[must_use]
    pub fn validate(&self, name: &str, args: &serde_json::Value) -> Result<(), String> {
        let tool = self.tools.read().get(name).cloned();
        let Some(tool) = tool else {
            return Err(format!("unknown tool: {name}"));
        };
        validate_args_against_schema(&tool.definition().parameters, args)
    }
}

/// Pi TypeBox `Value.Convert` subset: coerce common LLM string forms before validate.
fn coerce_args_against_schema(schema: &serde_json::Value, args: &mut serde_json::Value) {
    let Some(obj) = args.as_object_mut() else {
        return;
    };
    let Some(props) = schema.get("properties").and_then(|p| p.as_object()) else {
        return;
    };
    for (key, val) in obj.iter_mut() {
        if let Some(prop_schema) = props.get(key) {
            coerce_property_value(val, prop_schema);
        }
    }
}

fn coerce_property_value(val: &mut serde_json::Value, schema: &serde_json::Value) {
    let Some(typ) = schema.get("type").and_then(|t| t.as_str()) else {
        return;
    };
    match typ {
        "integer" => {
            if let Some(s) = val.as_str()
                && let Ok(n) = s.parse::<i64>()
            {
                *val = serde_json::Value::from(n);
            }
        }
        "number" => {
            if let Some(s) = val.as_str()
                && let Ok(n) = s.parse::<f64>()
                && let Some(num) = serde_json::Number::from_f64(n)
            {
                *val = serde_json::Value::Number(num);
            }
        }
        "boolean" => {
            if let Some(s) = val.as_str() {
                match s.to_ascii_lowercase().as_str() {
                    "true" => *val = serde_json::Value::Bool(true),
                    "false" => *val = serde_json::Value::Bool(false),
                    _ => {}
                }
            }
        }
        "object" => {
            if let (Some(nested), Some(nested_props)) = (
                val.as_object_mut(),
                schema.get("properties").and_then(|p| p.as_object()),
            ) {
                for (nk, nv) in nested.iter_mut() {
                    if let Some(ns) = nested_props.get(nk) {
                        coerce_property_value(nv, ns);
                    }
                }
            }
        }
        "array" => {
            if let (Some(arr), Some(items)) = (val.as_array_mut(), schema.get("items")) {
                for item in arr.iter_mut() {
                    coerce_property_value(item, items);
                }
            }
        }
        _ => {}
    }
}

/// Lightweight JSON Schema checks (required, property types, additionalProperties).
fn validate_args_against_schema(
    schema: &serde_json::Value,
    args: &serde_json::Value,
) -> Result<(), String> {
    let Some(obj) = args.as_object() else {
        if schema
            .get("required")
            .and_then(|r| r.as_array())
            .is_some_and(|a| !a.is_empty())
        {
            return Err("arguments must be an object".into());
        }
        return Ok(());
    };

    if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
        for req in required {
            if let Some(key) = req.as_str()
                && !obj.contains_key(key)
            {
                return Err(format!("missing required parameter: {key}"));
            }
        }
    }

    let props = schema.get("properties").and_then(|p| p.as_object());
    let reject_unknown =
        schema.get("additionalProperties") == Some(&serde_json::Value::Bool(false));

    for (key, val) in obj {
        if let Some(prop_schema) = props.and_then(|p| p.get(key)) {
            validate_property_value(key, val, prop_schema)?;
        } else if reject_unknown {
            return Err(format!("unknown property: {key}"));
        }
    }
    Ok(())
}

fn validate_property_value(
    key: &str,
    val: &serde_json::Value,
    schema: &serde_json::Value,
) -> Result<(), String> {
    if let Some(allowed) = schema.get("enum").and_then(|e| e.as_array()) {
        if !allowed.iter().any(|v| v == val) {
            let labels: Vec<String> = allowed
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(String::from)
                        .unwrap_or_else(|| v.to_string())
                })
                .collect();
            return Err(format!(
                "property `{key}`: must be one of [{}]",
                labels.join(", ")
            ));
        }
    }

    let Some(typ) = schema.get("type").and_then(|t| t.as_str()) else {
        return Ok(());
    };

    let type_ok = match typ {
        "string" => val.is_string(),
        "integer" => val.as_i64().is_some() || val.as_u64().is_some(),
        "number" => val.is_number(),
        "boolean" => val.is_boolean(),
        "array" => val.is_array(),
        "object" => val.is_object(),
        "null" => val.is_null(),
        _ => true,
    };
    if !type_ok {
        return Err(format!("property `{key}`: expected type `{typ}`"));
    }

    if typ == "integer" || typ == "number" {
        if let Some(n) = numeric_value(val) {
            if let Some(min) = schema.get("minimum").and_then(|m| m.as_f64()) {
                if (n as f64) < min {
                    return Err(format!("property `{key}`: must be >= {min}"));
                }
            }
            if let Some(max) = schema.get("maximum").and_then(|m| m.as_f64()) {
                if (n as f64) > max {
                    return Err(format!("property `{key}`: must be <= {max}"));
                }
            }
        }
    }

    if typ == "string" {
        if let Some(s) = val.as_str() {
            if let Some(min) = schema.get("minLength").and_then(|m| m.as_u64()) {
                if (s.len() as u64) < min {
                    return Err(format!("property `{key}`: string length must be >= {min}"));
                }
            }
        }
    }

    if typ == "array" {
        if let (Some(arr), Some(items)) = (val.as_array(), schema.get("items")) {
            for (i, item) in arr.iter().enumerate() {
                validate_property_value(&format!("{key}[{i}]"), item, items)?;
            }
        }
    }

    if typ == "object" {
        if let (Some(nested), Some(nested_props)) = (
            val.as_object(),
            schema.get("properties").and_then(|p| p.as_object()),
        ) {
            if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
                for req in required {
                    if let Some(rk) = req.as_str()
                        && !nested.contains_key(rk)
                    {
                        return Err(format!("property `{key}`: missing required `{rk}`"));
                    }
                }
            }
            for (nk, nv) in nested {
                if let Some(ns) = nested_props.get(nk) {
                    validate_property_value(&format!("{key}.{nk}"), nv, ns)?;
                }
            }
        }
    }

    Ok(())
}

fn numeric_value(val: &serde_json::Value) -> Option<i128> {
    val.as_i64()
        .map(|n| n as i128)
        .or_else(|| val.as_u64().map(|n| n as i128))
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::EditTool;
    use async_trait::async_trait;
    use uncode_core::error::UncodeError;
    use uncode_core::tool::{ToolDefinition, ToolExecutor};

    struct FakeTool {
        def: ToolDefinition,
    }

    #[async_trait]
    impl ToolExecutor for FakeTool {
        fn definition(&self) -> ToolDefinition {
            self.def.clone()
        }
        async fn execute(&self, _arguments: serde_json::Value) -> Result<String, UncodeError> {
            Ok("ok".into())
        }
    }

    #[test]
    fn test_validate_missing_required() {
        let reg = ToolRegistry::new();
        reg.register(
            "read",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "read".into(),
                    description: "Read file".into(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "required": ["path"],
                        "properties": { "path": {"type": "string"} }
                    }),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );

        let err = reg.validate("read", &serde_json::json!({})).unwrap_err();
        assert!(err.contains("missing required parameter: path"));
    }

    #[test]
    fn test_validate_ok() {
        let reg = ToolRegistry::new();
        reg.register(
            "read",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "read".into(),
                    description: "Read file".into(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "required": ["path"],
                        "properties": { "path": {"type": "string"} }
                    }),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );

        assert!(
            reg.validate("read", &serde_json::json!({"path": "/foo"}))
                .is_ok()
        );
    }

    #[test]
    fn test_validate_unknown_tool() {
        let reg = ToolRegistry::new();
        let err = reg.validate("nope", &serde_json::json!({})).unwrap_err();
        assert!(err.contains("unknown tool"));
    }

    #[test]
    fn test_active_tools_filters_definitions() {
        let reg = ToolRegistry::new();
        reg.register(
            "read",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "read".into(),
                    description: "Read".into(),
                    parameters: serde_json::json!({}),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );
        reg.register(
            "write",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "write".into(),
                    description: "Write".into(),
                    parameters: serde_json::json!({}),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );

        assert_eq!(reg.definitions().len(), 2);
        reg.set_active_tools(&["read"]).unwrap();
        let defs = reg.definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "read");
        assert!(reg.is_active("read"));
        assert!(!reg.is_active("write"));
    }

    #[test]
    fn test_set_active_tools_rejects_unknown() {
        let reg = ToolRegistry::new();
        reg.register(
            "read",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "read".into(),
                    description: "".into(),
                    parameters: serde_json::json!({}),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );
        let err = reg.set_active_tools(&["nope"]).unwrap_err();
        assert!(err.contains("unknown tool"));
    }

    #[test]
    fn test_no_tools_empty_active_set() {
        let reg = ToolRegistry::new();
        reg.register(
            "read",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "read".into(),
                    description: "".into(),
                    parameters: serde_json::json!({}),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );
        reg.set_active_tools(&[] as &[&str]).unwrap();
        assert!(reg.definitions().is_empty());
        assert!(!reg.is_active("read"));
    }

    #[test]
    fn test_validate_non_object_with_required() {
        let reg = ToolRegistry::new();
        reg.register(
            "tool",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "tool".into(),
                    description: "".into(),
                    parameters: serde_json::json!({"required": ["x"]}),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );
        let err = reg
            .validate("tool", &serde_json::json!("string"))
            .unwrap_err();
        assert!(err.contains("arguments must be an object"));
    }

    #[test]
    fn test_validate_wrong_type() {
        let reg = ToolRegistry::new();
        reg.register(
            "read",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "read".into(),
                    description: "".into(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": { "path": {"type": "string"} }
                    }),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );
        let err = reg
            .validate("read", &serde_json::json!({"path": 1}))
            .unwrap_err();
        assert!(err.contains("expected type `string`"));
    }

    #[test]
    fn test_prepare_and_validate_runs_prepare_before_schema() {
        struct CoercePathTool {
            def: ToolDefinition,
        }

        #[async_trait]
        impl ToolExecutor for CoercePathTool {
            fn definition(&self) -> ToolDefinition {
                self.def.clone()
            }
            fn prepare_arguments(
                &self,
                arguments: serde_json::Value,
            ) -> Result<serde_json::Value, UncodeError> {
                let mut obj = arguments.as_object().cloned().unwrap_or_default();
                obj.entry("path".to_owned())
                    .or_insert(serde_json::Value::String("/from-prepare".into()));
                Ok(serde_json::Value::Object(obj))
            }
            async fn execute(&self, _arguments: serde_json::Value) -> Result<String, UncodeError> {
                Ok("ok".into())
            }
        }

        let reg = ToolRegistry::new();
        reg.register(
            "read",
            Arc::new(CoercePathTool {
                def: ToolDefinition {
                    name: "read".into(),
                    description: "".into(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "required": ["path"],
                        "properties": { "path": {"type": "string"} }
                    }),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );

        let prepared = reg
            .prepare_and_validate("read", serde_json::json!({}))
            .unwrap();
        assert_eq!(prepared["path"], "/from-prepare");
    }

    #[test]
    fn test_coerce_string_integer_before_validate() {
        let reg = ToolRegistry::new();
        reg.register(
            "bash",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "bash".into(),
                    description: "".into(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "timeout": {"type": "integer", "minimum": 1}
                        }
                    }),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );
        let prepared = reg
            .prepare_and_validate("bash", serde_json::json!({"timeout": "120"}))
            .unwrap();
        assert_eq!(prepared["timeout"], 120);
    }

    #[test]
    fn test_list_active_respects_filter() {
        let reg = ToolRegistry::new();
        reg.register(
            "read",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "read".into(),
                    description: "".into(),
                    parameters: serde_json::json!({}),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );
        reg.register(
            "write",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "write".into(),
                    description: "".into(),
                    parameters: serde_json::json!({}),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );
        reg.set_active_tools(&["read"]).unwrap();
        let active = reg.list_active();
        assert_eq!(active, vec!["read"]);
        assert_eq!(reg.list().len(), 2);
    }

    #[test]
    fn test_validate_minimum_integer() {
        let reg = ToolRegistry::new();
        reg.register(
            "bash",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "bash".into(),
                    description: "".into(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "timeout": {"type": "integer", "minimum": 1}
                        }
                    }),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );
        let err = reg
            .validate("bash", &serde_json::json!({"timeout": 0}))
            .unwrap_err();
        assert!(err.contains("must be >="));
    }

    #[test]
    fn test_edit_tool_rejects_invalid_op() {
        let reg = ToolRegistry::new();
        reg.register("edit", Arc::new(EditTool));
        let err = reg
            .prepare_and_validate(
                "edit",
                serde_json::json!({
                    "path": "foo.txt",
                    "edits": [{"op": "delete", "pos": "1#ab", "lines": "x"}]
                }),
            )
            .unwrap_err();
        assert!(err.contains("must be one of"));
    }

    #[test]
    fn test_validate_enum_rejects_invalid() {
        let reg = ToolRegistry::new();
        reg.register(
            "edit",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "edit".into(),
                    description: "".into(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "edits": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "op": {
                                            "type": "string",
                                            "enum": ["replace", "prepend", "append"]
                                        }
                                    },
                                    "required": ["op"]
                                }
                            }
                        }
                    }),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );
        let err = reg
            .validate(
                "edit",
                &serde_json::json!({
                    "edits": [{"op": "delete"}]
                }),
            )
            .unwrap_err();
        assert!(err.contains("must be one of"));
        assert!(err.contains("replace"));
    }

    #[test]
    fn test_validate_unknown_property() {
        let reg = ToolRegistry::new();
        reg.register(
            "read",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "read".into(),
                    description: "".into(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "additionalProperties": false,
                        "properties": { "path": {"type": "string"} }
                    }),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );
        let err = reg
            .validate("read", &serde_json::json!({"path": "a", "extra": true}))
            .unwrap_err();
        assert!(err.contains("unknown property"));
    }

    // ── ToolSource tracking tests ──

    #[test]
    fn test_register_sets_builtin_source() {
        let reg = ToolRegistry::new();
        reg.register(
            "read",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "read".into(),
                    description: "".into(),
                    parameters: serde_json::json!({}),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );
        assert_eq!(reg.source("read"), Some(ToolSource::Builtin));
    }

    #[test]
    fn test_register_extension_tool_ok() {
        let reg = ToolRegistry::new();
        let result = reg.register_extension_tool(
            "my_tool",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "my_tool".into(),
                    description: "Custom".into(),
                    parameters: serde_json::json!({}),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );
        assert!(result.is_ok());
        assert_eq!(reg.source("my_tool"), Some(ToolSource::Extension));
    }

    #[test]
    fn test_register_extension_tool_rejects_reserved_name() {
        let reg = ToolRegistry::new();
        for reserved in &[
            "read",
            "write",
            "edit",
            "bash",
            "grep",
            "find",
            "ls",
            "web_fetch",
            "web_search",
        ] {
            let result = reg.register_extension_tool(
                *reserved,
                Arc::new(FakeTool {
                    def: ToolDefinition {
                        name: reserved.to_string(),
                        description: "Custom".into(),
                        parameters: serde_json::json!({}),
                        label: None,
                        execution_mode: Default::default(),
                    },
                }),
            );
            assert!(result.unwrap_err().contains("reserved"));
        }
    }

    #[test]
    fn test_register_extension_tool_rejects_duplicate() {
        let reg = ToolRegistry::new();
        reg.register(
            "existing",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "existing".into(),
                    description: "".into(),
                    parameters: serde_json::json!({}),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );
        let result = reg.register_extension_tool(
            "existing",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "existing".into(),
                    description: "dup".into(),
                    parameters: serde_json::json!({}),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );
        assert!(result.unwrap_err().contains("already registered"));
    }

    #[test]
    fn test_extension_tool_bypasses_active_filter() {
        let reg = ToolRegistry::new();
        reg.register(
            "read",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "read".into(),
                    description: "Read".into(),
                    parameters: serde_json::json!({}),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );
        reg.register_extension_tool(
            "ext_tool",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "ext_tool".into(),
                    description: "Extension".into(),
                    parameters: serde_json::json!({}),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        )
        .unwrap();

        reg.set_active_tools(&["read"]).unwrap();

        // Builtin: active
        assert!(reg.is_active("read"));
        // Extension: always active, bypasses filter
        assert!(reg.is_active("ext_tool"));
        // Definitions include extension tool even with active filter
        let defs = reg.definitions();
        assert_eq!(defs.len(), 2);
    }

    #[test]
    fn test_unregister_tool() {
        let reg = ToolRegistry::new();
        reg.register(
            "read",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "read".into(),
                    description: "".into(),
                    parameters: serde_json::json!({}),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );
        assert!(reg.unregister("read"));
        assert!(!reg.is_active("read"));
        assert!(reg.get("read").is_none());
        assert_eq!(reg.source("read"), None);
    }

    #[test]
    fn test_unregister_nonexistent() {
        let reg = ToolRegistry::new();
        assert!(!reg.unregister("nope"));
    }
}
