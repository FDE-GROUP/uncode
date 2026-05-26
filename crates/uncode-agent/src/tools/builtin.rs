//! Built-in coding tools registration and Pi-aligned default active sets.
//!
//! **Pi:** default builtin tools are `read`, `bash`, `edit`, `write`, plus `grep`, `find`, `ls`.
//! Network tools (`web_fetch`, `web_search`) are uncode extensions — not in the default LLM tool schema.

use std::collections::HashMap;
use std::sync::Arc;

use uncode_core::config::ToolsConfig;

use super::{
    BashTool, EditTool, FindTool, GrepTool, LLMQueryTool, LsTool, QuestionTool, ReadTool,
    ToolRegistry, WebFetchTool, WebSearchTool, WriteTool,
};

/// Pi `coding-agent` built-in tool names (no web tools).
pub const PI_BUILTIN_TOOL_NAMES: &[&str] = &[
    "read",
    "write",
    "edit",
    "bash",
    "grep",
    "find",
    "ls",
    "llm_query",
];

/// CLI / harness options for which tools are exposed to the LLM.
#[derive(Debug, Clone, Default)]
pub struct ToolLaunchConfig {
    /// `--no-tools`: empty active set.
    pub no_tools: bool,
    /// `--no-builtin-tools`: only non-Pi builtins (e.g. `web_fetch`, `web_search`).
    pub no_builtin_tools: bool,
    /// `--tools`: explicit comma-separated whitelist (parsed by caller).
    pub tools: Option<Vec<String>>,
}

/// Whether `name` is a Pi coding-agent built-in (excludes uncode web tools).
pub fn is_pi_builtin_tool(name: &str) -> bool {
    PI_BUILTIN_TOOL_NAMES.contains(&name)
}

/// Register all standard coding tools on the registry (superset of Pi builtins).
pub fn register_coding_tools(
    registry: &ToolRegistry,
    api_keys: &HashMap<String, String>,
    tools_config: &ToolsConfig,
) {
    registry.register(
        "read",
        Arc::new(ReadTool::with_max_file_bytes(tools_config.max_file_bytes)),
    );
    registry.register("write", Arc::new(WriteTool));
    registry.register("edit", Arc::new(EditTool));
    registry.register(
        "grep",
        Arc::new(GrepTool::new(
            tools_config.max_grep_results,
            tools_config.max_file_bytes,
        )),
    );
    registry.register(
        "bash",
        Arc::new(
            BashTool::new()
                .with_sandbox(tools_config.bash.sandbox, tools_config.bash.sandbox_profile),
        ),
    );
    registry.register("find", Arc::new(FindTool));
    registry.register("ls", Arc::new(LsTool));
    registry.register("web_fetch", Arc::new(WebFetchTool::new()));
    registry.register(
        "llm_query",
        Arc::new(LLMQueryTool::new(std::sync::Arc::new(
            uncode_ai::model::builtin_models(),
        ))),
    );

    if let Some(key) = api_keys.get("tavily")
        && let Some(tool) = WebSearchTool::try_new(key)
    {
        registry.register("web_search", Arc::new(tool));
    }
    registry.register("question", Arc::new(QuestionTool::new()));
    // Use ontology ActionDefs to drive ToolDefinition generation.
    registry.set_builtin_ontology();
}

/// Apply Pi-aligned default: only built-in coding tools visible to the LLM (no `web_*`).
pub fn apply_pi_default_active_tools(registry: &ToolRegistry) -> Result<(), String> {
    registry.set_active_tools(PI_BUILTIN_TOOL_NAMES)
}

/// In-memory registry with Pi coding tools registered and default active set.
pub fn new_pi_coding_registry(api_keys: &HashMap<String, String>) -> Result<ToolRegistry, String> {
    new_pi_coding_registry_with_tools(api_keys, &ToolsConfig::default())
}

pub fn new_pi_coding_registry_with_tools(
    api_keys: &HashMap<String, String>,
    tools_config: &ToolsConfig,
) -> Result<ToolRegistry, String> {
    let registry = ToolRegistry::new();
    register_coding_tools_and_configure(
        &registry,
        api_keys,
        &ToolLaunchConfig::default(),
        tools_config,
    )?;
    Ok(registry)
}

/// Register coding tools and apply launch-time active set (CLI / harness entry).
pub fn register_coding_tools_and_configure(
    registry: &ToolRegistry,
    api_keys: &HashMap<String, String>,
    config: &ToolLaunchConfig,
    tools_config: &ToolsConfig,
) -> Result<(), String> {
    register_coding_tools(registry, api_keys, tools_config);
    configure_active_tools(registry, config)
}

/// Configure active tools from CLI-style launch options (after `register_coding_tools`).
pub fn configure_active_tools(
    registry: &ToolRegistry,
    config: &ToolLaunchConfig,
) -> Result<(), String> {
    if config.no_tools {
        registry.set_active_tools(&[] as &[&str])
    } else if let Some(ref names) = config.tools {
        if names.is_empty() {
            return Err("--tools requires at least one tool name".into());
        }
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        registry.set_active_tools(&refs)
    } else if config.no_builtin_tools {
        let names = registry.list();
        let extension_only: Vec<&str> = names
            .iter()
            .filter(|n| !is_pi_builtin_tool(n))
            .map(|s| s.as_str())
            .collect();
        registry.set_active_tools(&extension_only)
    } else {
        apply_pi_default_active_tools(registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_pi_builtin_excludes_web() {
        assert!(is_pi_builtin_tool("read"));
        assert!(!is_pi_builtin_tool("web_fetch"));
    }

    #[test]
    fn list_active_matches_pi_default() {
        let reg =
            new_pi_coding_registry_with_tools(&HashMap::new(), &ToolsConfig::default()).unwrap();
        let mut active = reg.list_active();
        active.sort();
        let mut expected: Vec<String> = PI_BUILTIN_TOOL_NAMES
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        expected.sort();
        assert_eq!(active, expected);
    }

    #[test]
    fn new_pi_coding_registry_has_seven_active() {
        let reg =
            new_pi_coding_registry_with_tools(&HashMap::new(), &ToolsConfig::default()).unwrap();
        assert_eq!(reg.definitions().len(), PI_BUILTIN_TOOL_NAMES.len());
        for name in PI_BUILTIN_TOOL_NAMES {
            assert!(reg.is_active(name));
        }
        assert!(!reg.is_active("web_fetch"));
    }

    #[test]
    fn no_builtin_tools_leaves_extensions_only() {
        let reg = ToolRegistry::new();
        register_coding_tools(&reg, &HashMap::new(), &ToolsConfig::default());
        configure_active_tools(
            &reg,
            &ToolLaunchConfig {
                no_builtin_tools: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(reg.is_active("web_fetch"));
        assert!(!reg.is_active("llm_query"));
        assert!(!reg.is_active("read"));
        // question is non-Pi, stays active like web_fetch
        assert!(reg.is_active("question"));
        assert_eq!(reg.definitions().len(), 2); // web_fetch + question
    }

    #[test]
    fn ontology_definitions_equal_executor_definitions() {
        // Build registry WITHOUT ontology for baseline
        let reg_no_onto = ToolRegistry::new();
        reg_no_onto.register("read", Arc::new(ReadTool::with_max_file_bytes(1_048_576)));
        reg_no_onto.register("write", Arc::new(WriteTool));
        reg_no_onto.register("edit", Arc::new(EditTool));
        reg_no_onto.register("grep", Arc::new(GrepTool::new(1000, 1_048_576)));
        reg_no_onto.register("bash", Arc::new(BashTool::new()));
        reg_no_onto.register("find", Arc::new(FindTool));
        reg_no_onto.register("ls", Arc::new(LsTool));
        let baseline: HashMap<_, _> = reg_no_onto
            .definitions()
            .into_iter()
            .map(|d| (d.name.clone(), d))
            .collect();

        // Build registry WITH ontology
        let reg_with_onto = ToolRegistry::new();
        reg_with_onto.set_builtin_ontology();
        reg_with_onto.register("read", Arc::new(ReadTool::with_max_file_bytes(1_048_576)));
        reg_with_onto.register("write", Arc::new(WriteTool));
        reg_with_onto.register("edit", Arc::new(EditTool));
        reg_with_onto.register("grep", Arc::new(GrepTool::new(1000, 1_048_576)));
        reg_with_onto.register("bash", Arc::new(BashTool::new()));
        reg_with_onto.register("find", Arc::new(FindTool));
        reg_with_onto.register("ls", Arc::new(LsTool));
        let onto_defs: HashMap<_, _> = reg_with_onto
            .definitions()
            .into_iter()
            .map(|d| (d.name.clone(), d))
            .collect();

        let mut mismatches = Vec::new();
        for name in baseline.keys() {
            let base = &baseline[name];
            let onto = &onto_defs[name];
            if onto.name != base.name {
                mismatches.push(format!("{name}: name {}=>{}", onto.name, base.name));
            }
            if onto.execution_mode != base.execution_mode {
                mismatches.push(format!("{name}: execution_mode mismatch"));
            }
            let onto_props = onto.parameters["properties"]
                .as_object()
                .unwrap_or_else(|| panic!("onto properties for {name}"));
            let base_props = base.parameters["properties"]
                .as_object()
                .unwrap_or_else(|| panic!("base properties for {name}"));
            for key in base_props.keys() {
                if !onto_props.contains_key(key) {
                    mismatches.push(format!("{name}: onto misses field '{key}'"));
                } else if onto_props[key]["type"] != base_props[key]["type"]
                    && base_props[key]["type"] != "array"
                {
                    mismatches.push(format!(
                        "{name}: type mismatch for '{key}' ({}=>{})",
                        onto_props[key]["type"], base_props[key]["type"]
                    ));
                }
            }
            // Required fields
            let onto_req: Vec<_> = onto.parameters["required"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                .unwrap_or_default();
            let base_req: Vec<_> = base.parameters["required"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                .unwrap_or_default();
            let mut onto_req_sorted = onto_req.clone();
            onto_req_sorted.sort_unstable();
            let mut base_req_sorted = base_req;
            base_req_sorted.sort_unstable();
            if onto_req_sorted != base_req_sorted {
                mismatches.push(format!(
                    "{name}: required mismatch (onto={:?} base={:?})",
                    onto_req_sorted, base_req_sorted
                ));
            }
        }
        assert!(
            mismatches.is_empty(),
            "Ontology <-> executor mismatches:\n{}",
            mismatches.join("\n")
        );
    }
}
