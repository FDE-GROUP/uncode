//! Builtin domain ontology — 9 tools + 3 entities.

use crate::registry::TypeRegistry;
use crate::types::{
    ActionDef, Cardinality, Constraint, Effect, EntityCategory, EntityDef, ExecutionCategory,
    FieldDef, LinkDef, TypeId,
};

/// Build the complete coding agent domain ontology.
pub fn coding_agent_ontology() -> TypeRegistry {
    let mut reg = TypeRegistry::new();

    // Entities
    reg.register_entity(entity_file());
    reg.register_entity(entity_workspace());
    reg.register_entity(entity_module());

    // Tools
    reg.register_action(action_read());
    reg.register_action(action_write());
    reg.register_action(action_edit());
    reg.register_action(action_grep());
    reg.register_action(action_find());
    reg.register_action(action_ls());
    reg.register_action(action_bash());
    reg.register_action(action_web_fetch());
    reg.register_action(action_web_search());

    // Links — domain semantic
    reg.register_link(link_workspace_contains_file());
    reg.register_link(link_workspace_contains_module());
    reg.register_link(link_file_in_module());

    reg
}

fn entity_file() -> EntityDef {
    EntityDef {
        id: TypeId("File".into()),
        category: EntityCategory::Domain,
        fields: vec![
            FieldDef {
                name: "path".into(),
                value_type: "string".into(),
                required: true,
                default: None,
                aliases: vec![
                    "filepath".into(),
                    "file_path".into(),
                    "file".into(),
                    "filename".into(),
                ],
                description: Some("Filesystem path".into()),
            },
            FieldDef {
                name: "content".into(),
                value_type: "string".into(),
                required: false,
                default: None,
                aliases: vec!["body".into()],
                description: Some("File content".into()),
            },
        ],
        description: Some("Filesystem file".into()),
    }
}

fn entity_workspace() -> EntityDef {
    EntityDef {
        id: TypeId("Workspace".into()),
        category: EntityCategory::Domain,
        fields: vec![FieldDef {
            name: "root".into(),
            value_type: "string".into(),
            required: true,
            default: None,
            aliases: vec!["dir".into(), "directory".into(), "folder".into()],
            description: Some("Workspace root directory".into()),
        }],
        description: Some("Project workspace".into()),
    }
}

fn entity_module() -> EntityDef {
    EntityDef {
        id: TypeId("Module".into()),
        category: EntityCategory::Domain,
        fields: vec![FieldDef {
            name: "name".into(),
            value_type: "string".into(),
            required: true,
            default: None,
            aliases: vec![],
            description: Some("Rust module name".into()),
        }],
        description: Some("Code module / crate".into()),
    }
}

fn action_read() -> ActionDef {
    ActionDef {
        name: "read".into(),
        category: EntityCategory::Domain,
        fields: vec![
            FieldDef {
                name: "path".into(),
                value_type: "string".into(),
                required: true,
                default: None,
                aliases: vec![
                    "filepath".into(),
                    "file_path".into(),
                    "file".into(),
                    "filename".into(),
                ],
                description: Some("File path to read".into()),
            },
            FieldDef {
                name: "offset".into(),
                value_type: "integer".into(),
                required: false,
                default: Some(serde_json::json!(0)),
                aliases: vec![],
                description: None,
            },
            FieldDef {
                name: "limit".into(),
                value_type: "integer".into(),
                required: false,
                default: None,
                aliases: vec![],
                description: None,
            },
        ],
        output_type: TypeId::STRING,
        preconditions: vec![Constraint::RequiredField {
            field: "path".into(),
        }],
        effects: vec![Effect::Read {
            target: "File".into(),
            fields: vec!["content".into()],
        }],
        execution_category: ExecutionCategory::ReadOnly,
        description: Some("Read file contents".into()),
    }
}

fn action_write() -> ActionDef {
    ActionDef {
        name: "write".into(),
        category: EntityCategory::Domain,
        fields: vec![
            FieldDef {
                name: "path".into(),
                value_type: "string".into(),
                required: true,
                default: None,
                aliases: vec![
                    "filepath".into(),
                    "file_path".into(),
                    "file".into(),
                    "filename".into(),
                ],
                description: Some("File path to write".into()),
            },
            FieldDef {
                name: "content".into(),
                value_type: "string".into(),
                required: true,
                default: None,
                aliases: vec!["body".into()],
                description: Some("Content to write".into()),
            },
        ],
        output_type: TypeId::STRING,
        preconditions: vec![
            Constraint::RequiredField {
                field: "path".into(),
            },
            Constraint::RequiredField {
                field: "content".into(),
            },
        ],
        effects: vec![Effect::Modify {
            entity: "File".into(),
            fields: vec!["content".into()],
        }],
        execution_category: ExecutionCategory::Destructive,
        description: Some("Write file contents".into()),
    }
}

fn action_edit() -> ActionDef {
    ActionDef {
        name: "edit".into(),
        category: EntityCategory::Domain,
        fields: vec![FieldDef {
            name: "path".into(),
            value_type: "string".into(),
            required: true,
            default: None,
            aliases: vec![
                "filepath".into(),
                "file_path".into(),
                "file".into(),
                "filename".into(),
            ],
            description: Some("File path to edit".into()),
        }],
        output_type: TypeId::STRING,
        preconditions: vec![Constraint::RequiredField {
            field: "path".into(),
        }],
        effects: vec![Effect::Modify {
            entity: "File".into(),
            fields: vec!["content".into()],
        }],
        execution_category: ExecutionCategory::Destructive,
        description: Some("Edit file with search/replace".into()),
    }
}

fn action_grep() -> ActionDef {
    ActionDef {
        name: "grep".into(),
        category: EntityCategory::Domain,
        fields: vec![
            FieldDef {
                name: "pattern".into(),
                value_type: "string".into(),
                required: true,
                default: None,
                aliases: vec!["query".into(), "regex".into(), "search".into()],
                description: Some("Search pattern".into()),
            },
            FieldDef {
                name: "path".into(),
                value_type: "string".into(),
                required: false,
                default: None,
                aliases: vec![
                    "dir".into(),
                    "directory".into(),
                    "folder".into(),
                    "root".into(),
                ],
                description: None,
            },
            FieldDef {
                name: "case_sensitive".into(),
                value_type: "boolean".into(),
                required: false,
                default: Some(serde_json::json!(false)),
                aliases: vec![],
                description: None,
            },
        ],
        output_type: TypeId::STRING,
        preconditions: vec![Constraint::RequiredField {
            field: "pattern".into(),
        }],
        effects: vec![Effect::Read {
            target: "Workspace".into(),
            fields: vec!["files".into()],
        }],
        execution_category: ExecutionCategory::ReadOnly,
        description: Some("Search file contents".into()),
    }
}

fn action_find() -> ActionDef {
    ActionDef {
        name: "find".into(),
        category: EntityCategory::Domain,
        fields: vec![FieldDef {
            name: "path".into(),
            value_type: "string".into(),
            required: false,
            default: None,
            aliases: vec![
                "dir".into(),
                "directory".into(),
                "folder".into(),
                "root".into(),
            ],
            description: None,
        }],
        output_type: TypeId::STRING,
        preconditions: vec![],
        effects: vec![Effect::Read {
            target: "Workspace".into(),
            fields: vec!["files".into()],
        }],
        execution_category: ExecutionCategory::ReadOnly,
        description: Some("Find files by name".into()),
    }
}

fn action_ls() -> ActionDef {
    ActionDef {
        name: "ls".into(),
        category: EntityCategory::Domain,
        fields: vec![
            FieldDef {
                name: "path".into(),
                value_type: "string".into(),
                required: false,
                default: None,
                aliases: vec![
                    "dir".into(),
                    "directory".into(),
                    "folder".into(),
                    "root".into(),
                ],
                description: None,
            },
            FieldDef {
                name: "show_hidden".into(),
                value_type: "boolean".into(),
                required: false,
                default: Some(serde_json::json!(false)),
                aliases: vec![],
                description: None,
            },
        ],
        output_type: TypeId::STRING,
        preconditions: vec![],
        effects: vec![Effect::Read {
            target: "Workspace".into(),
            fields: vec!["files".into()],
        }],
        execution_category: ExecutionCategory::ReadOnly,
        description: Some("List directory contents".into()),
    }
}

fn action_bash() -> ActionDef {
    ActionDef {
        name: "bash".into(),
        category: EntityCategory::Domain,
        fields: vec![
            FieldDef {
                name: "command".into(),
                value_type: "string".into(),
                required: true,
                default: None,
                aliases: vec!["cmd".into(), "command_line".into(), "script".into()],
                description: Some("Shell command to execute".into()),
            },
            FieldDef {
                name: "workdir".into(),
                value_type: "string".into(),
                required: false,
                default: None,
                aliases: vec![],
                description: None,
            },
            FieldDef {
                name: "timeout".into(),
                value_type: "integer".into(),
                required: false,
                default: None,
                aliases: vec![],
                description: None,
            },
        ],
        output_type: TypeId::STRING,
        preconditions: vec![Constraint::RequiredField {
            field: "command".into(),
        }],
        effects: vec![Effect::Exec {
            command: "[dynamic]".into(),
        }],
        execution_category: ExecutionCategory::Shell,
        description: Some("Execute shell command".into()),
    }
}

fn action_web_fetch() -> ActionDef {
    ActionDef {
        name: "web_fetch".into(),
        category: EntityCategory::Domain,
        fields: vec![FieldDef {
            name: "url".into(),
            value_type: "string".into(),
            required: true,
            default: None,
            aliases: vec!["uri".into(), "link".into(), "href".into()],
            description: Some("URL to fetch".into()),
        }],
        output_type: TypeId::STRING,
        preconditions: vec![Constraint::RequiredField {
            field: "url".into(),
        }],
        effects: vec![Effect::Network {
            destination: "[dynamic]".into(),
        }],
        execution_category: ExecutionCategory::Network,
        description: Some("Fetch web page content".into()),
    }
}

fn action_web_search() -> ActionDef {
    ActionDef {
        name: "web_search".into(),
        category: EntityCategory::Domain,
        fields: vec![FieldDef {
            name: "query".into(),
            value_type: "string".into(),
            required: true,
            default: None,
            aliases: vec!["q".into(), "term".into(), "search".into()],
            description: Some("Search query".into()),
        }],
        output_type: TypeId::STRING,
        preconditions: vec![Constraint::RequiredField {
            field: "query".into(),
        }],
        effects: vec![Effect::Network {
            destination: "[search]".into(),
        }],
        execution_category: ExecutionCategory::Network,
        description: Some("Search the web".into()),
    }
}

// ═══════════════════════════════════════════════════════════
// System Resource Semantic Ontology
// ═══════════════════════════════════════════════════════════

/// Build the system resource ontology — LLM, Provider, Capability.
pub fn system_resource_ontology() -> TypeRegistry {
    let mut reg = TypeRegistry::new();

    reg.register_entity(entity_llm());
    reg.register_entity(entity_provider());
    reg.register_entity(entity_capability());

    reg.register_action(action_model_query());
    reg.register_action(action_model_switch());

    // Links — system resource semantic
    reg.register_link(link_provider_provides_llm());
    reg.register_link(link_llm_has_capability());

    reg
}

/// Build the full ontology — domain + system resource.
pub fn full_ontology() -> TypeRegistry {
    let domain = coding_agent_ontology();
    let system = system_resource_ontology();
    domain.merge(system)
}

fn entity_llm() -> EntityDef {
    EntityDef {
        id: TypeId("LLM".into()),
        category: EntityCategory::System,
        fields: vec![
            FieldDef {
                name: "model_id".into(),
                value_type: "string".into(),
                required: true,
                default: None,
                aliases: vec!["model".into(), "id".into()],
                description: Some("Unique model identifier (e.g. \"deepseek-v3\")".into()),
            },
            FieldDef {
                name: "provider".into(),
                value_type: "string".into(),
                required: true,
                default: None,
                aliases: vec!["vendor".into()],
                description: Some("Provider name (e.g. \"deepseek\", \"anthropic\")".into()),
            },
            FieldDef {
                name: "context_window".into(),
                value_type: "integer".into(),
                required: true,
                default: Some(serde_json::json!(128_000)),
                aliases: vec!["max_context".into(), "context_length".into()],
                description: Some("Maximum context window in tokens".into()),
            },
            FieldDef {
                name: "max_output_tokens".into(),
                value_type: "integer".into(),
                required: false,
                default: Some(serde_json::json!(8192)),
                aliases: vec!["max_tokens".into(), "max_completion_tokens".into()],
                description: Some("Maximum output tokens per request".into()),
            },
            FieldDef {
                name: "supports_vision".into(),
                value_type: "boolean".into(),
                required: false,
                default: Some(serde_json::json!(false)),
                aliases: vec!["vision".into(), "image_input".into()],
                description: Some("Whether the model supports image input".into()),
            },
            FieldDef {
                name: "supports_reasoning".into(),
                value_type: "boolean".into(),
                required: false,
                default: Some(serde_json::json!(false)),
                aliases: vec!["reasoning".into(), "thinking".into()],
                description: Some("Whether the model supports extended reasoning".into()),
            },
            FieldDef {
                name: "supports_tools".into(),
                value_type: "boolean".into(),
                required: false,
                default: Some(serde_json::json!(true)),
                aliases: vec!["tool_use".into(), "function_calling".into()],
                description: Some("Whether the model supports tool/function calling".into()),
            },
            FieldDef {
                name: "api_protocol".into(),
                value_type: "string".into(),
                required: false,
                default: Some(serde_json::json!("openai-completions")),
                aliases: vec!["api".into(), "protocol".into()],
                description: Some("API protocol identifier".into()),
            },
            FieldDef {
                name: "pricing_input_per_million".into(),
                value_type: "number".into(),
                required: false,
                default: Some(serde_json::json!(0.0)),
                aliases: vec!["input_cost".into()],
                description: Some("Input token cost per million tokens (USD)".into()),
            },
            FieldDef {
                name: "pricing_output_per_million".into(),
                value_type: "number".into(),
                required: false,
                default: Some(serde_json::json!(0.0)),
                aliases: vec!["output_cost".into()],
                description: Some("Output token cost per million tokens (USD)".into()),
            },
        ],
        description: Some("LLM model resource".into()),
    }
}

fn entity_provider() -> EntityDef {
    EntityDef {
        id: TypeId("Provider".into()),
        category: EntityCategory::System,
        fields: vec![
            FieldDef {
                name: "provider_id".into(),
                value_type: "string".into(),
                required: true,
                default: None,
                aliases: vec!["provider".into(), "vendor".into()],
                description: Some("Provider identifier (e.g. \"deepseek\")".into()),
            },
            FieldDef {
                name: "default_api_protocol".into(),
                value_type: "string".into(),
                required: true,
                default: None,
                aliases: vec!["api".into(), "protocol".into()],
                description: Some("Default API protocol for this provider".into()),
            },
            FieldDef {
                name: "base_url".into(),
                value_type: "string".into(),
                required: true,
                default: None,
                aliases: vec!["endpoint".into(), "api_url".into()],
                description: Some("Base URL for API requests".into()),
            },
        ],
        description: Some("LLM provider / API endpoint".into()),
    }
}

fn entity_capability() -> EntityDef {
    EntityDef {
        id: TypeId("Capability".into()),
        category: EntityCategory::System,
        fields: vec![
            FieldDef {
                name: "name".into(),
                value_type: "string".into(),
                required: true,
                default: None,
                aliases: vec!["capability".into()],
                description: Some("Capability name (e.g. \"vision\", \"reasoning\", \"tool_use\")".into()),
            },
            FieldDef {
                name: "capability_type".into(),
                value_type: "string".into(),
                required: true,
                default: None,
                aliases: vec!["type".into(), "category".into()],
                description: Some("Capability category: input_modality, reasoning, interaction".into()),
            },
        ],
        description: Some("Model capability declaration".into()),
    }
}

fn action_model_query() -> ActionDef {
    ActionDef {
        name: "model_query".into(),
        category: EntityCategory::System,
        fields: vec![
            FieldDef {
                name: "criteria".into(),
                value_type: "object".into(),
                required: false,
                default: None,
                aliases: vec!["filter".into(), "query".into()],
                description: Some("Query criteria (e.g. {\"supports_vision\": true})".into()),
            },
            FieldDef {
                name: "sort_by".into(),
                value_type: "string".into(),
                required: false,
                default: Some(serde_json::json!("cost")),
                aliases: vec![],
                description: Some("Sort field for results".into()),
            },
        ],
        output_type: TypeId("LLM".into()),
        preconditions: vec![],
        effects: vec![Effect::Read {
            target: "LLM".into(),
            fields: vec!["all".into()],
        }],
        execution_category: ExecutionCategory::ReadOnly,
        description: Some("Query available LLM models by capability".into()),
    }
}

fn action_model_switch() -> ActionDef {
    ActionDef {
        name: "model_switch".into(),
        category: EntityCategory::System,
        fields: vec![
            FieldDef {
                name: "model_id".into(),
                value_type: "string".into(),
                required: true,
                default: None,
                aliases: vec!["model".into(), "target_model".into()],
                description: Some("Target model to switch to".into()),
            },
            FieldDef {
                name: "reason".into(),
                value_type: "string".into(),
                required: false,
                default: None,
                aliases: vec!["rationale".into()],
                description: Some("Reason for model switch".into()),
            },
        ],
        output_type: TypeId("LLM".into()),
        preconditions: vec![Constraint::RequiredField {
            field: "model_id".into(),
        }],
        effects: vec![Effect::Modify {
            entity: "LLM".into(),
            fields: vec!["active_model".into()],
        }],
        execution_category: ExecutionCategory::Destructive,
        description: Some("Switch the active LLM model".into()),
    }
}

// ═══════════════════════════════════════════════════════════
// Link Definitions — Domain Semantic
// ═══════════════════════════════════════════════════════════

fn link_workspace_contains_file() -> LinkDef {
    LinkDef {
        id: TypeId("Workspace_contains_File".into()),
        source_type: TypeId("Workspace".into()),
        target_type: TypeId("File".into()),
        cardinality: Cardinality::OneToMany,
        inverse: Some(TypeId("File_belongs_to_Workspace".into())),
        description: Some("Workspace contains many files".into()),
    }
}

fn link_workspace_contains_module() -> LinkDef {
    LinkDef {
        id: TypeId("Workspace_contains_Module".into()),
        source_type: TypeId("Workspace".into()),
        target_type: TypeId("Module".into()),
        cardinality: Cardinality::OneToMany,
        inverse: Some(TypeId("Module_belongs_to_Workspace".into())),
        description: Some("Workspace contains many modules/crates".into()),
    }
}

fn link_file_in_module() -> LinkDef {
    LinkDef {
        id: TypeId("Module_contains_File".into()),
        source_type: TypeId("Module".into()),
        target_type: TypeId("File".into()),
        cardinality: Cardinality::OneToMany,
        inverse: Some(TypeId("File_belongs_to_Module".into())),
        description: Some("Module contains many files".into()),
    }
}

// ═══════════════════════════════════════════════════════════
// Link Definitions — System Resource Semantic
// ═══════════════════════════════════════════════════════════

fn link_provider_provides_llm() -> LinkDef {
    LinkDef {
        id: TypeId("Provider_provides_LLM".into()),
        source_type: TypeId("Provider".into()),
        target_type: TypeId("LLM".into()),
        cardinality: Cardinality::OneToMany,
        inverse: Some(TypeId("LLM_provided_by_Provider".into())),
        description: Some("Provider provides multiple LLM models".into()),
    }
}

fn link_llm_has_capability() -> LinkDef {
    LinkDef {
        id: TypeId("LLM_has_Capability".into()),
        source_type: TypeId("LLM".into()),
        target_type: TypeId("Capability".into()),
        cardinality: Cardinality::OneToMany,
        inverse: Some(TypeId("Capability_of_LLM".into())),
        description: Some("LLM model has multiple capabilities".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ontology_has_9_actions() {
        let reg = coding_agent_ontology();
        assert_eq!(reg.actions.len(), 9, "expected 9 tool actions");
    }

    #[test]
    fn test_ontology_has_3_entities() {
        let reg = coding_agent_ontology();
        assert_eq!(reg.entities.len(), 3, "expected 3 entity types");
    }

    #[test]
    fn test_read_only_tools() {
        let reg = coding_agent_ontology();
        let read = reg.get_action("read").unwrap();
        assert_eq!(read.execution_category, ExecutionCategory::ReadOnly);
        assert!(read.is_read_only());

        let grep = reg.get_action("grep").unwrap();
        assert!(grep.is_read_only());

        let find = reg.get_action("find").unwrap();
        assert!(find.is_read_only());

        let ls = reg.get_action("ls").unwrap();
        assert!(ls.is_read_only());
    }

    #[test]
    fn test_destructive_tools() {
        let reg = coding_agent_ontology();
        let write = reg.get_action("write").unwrap();
        assert_eq!(write.execution_category, ExecutionCategory::Destructive);
        assert!(!write.is_read_only());

        let edit = reg.get_action("edit").unwrap();
        assert_eq!(edit.execution_category, ExecutionCategory::Destructive);
    }

    #[test]
    fn test_shell_tool() {
        let reg = coding_agent_ontology();
        let bash = reg.get_action("bash").unwrap();
        assert_eq!(bash.execution_category, ExecutionCategory::Shell);
    }

    #[test]
    fn test_network_tools() {
        let reg = coding_agent_ontology();
        let fetch = reg.get_action("web_fetch").unwrap();
        assert_eq!(fetch.execution_category, ExecutionCategory::Network);
        let search = reg.get_action("web_search").unwrap();
        assert_eq!(search.execution_category, ExecutionCategory::Network);
    }

    #[test]
    fn test_field_aliases_collected() {
        let reg = coding_agent_ontology();
        let aliases = reg.all_field_aliases();
        assert_eq!(aliases.get("filepath").unwrap(), "path");
        assert_eq!(aliases.get("file_path").unwrap(), "path");
        assert_eq!(aliases.get("cmd").unwrap(), "command");
        assert_eq!(aliases.get("uri").unwrap(), "url");
        assert_eq!(aliases.get("q").unwrap(), "query");
    }

    #[test]
    fn test_defaults_collected() {
        let reg = coding_agent_ontology();
        let defaults = reg.all_defaults();
        assert_eq!(defaults["read"]["offset"], 0);
        assert_eq!(defaults["grep"]["case_sensitive"], false);
        assert_eq!(defaults["ls"]["show_hidden"], false);
    }

    #[test]
    fn test_system_ontology_has_3_entities() {
        let reg = system_resource_ontology();
        assert_eq!(reg.entities.len(), 3, "expected 3 system entities");
    }

    #[test]
    fn test_system_ontology_has_2_actions() {
        let reg = system_resource_ontology();
        assert_eq!(reg.actions.len(), 2, "expected 2 system actions");
    }

    #[test]
    fn test_system_entities_are_system_category() {
        let reg = system_resource_ontology();
        let llm = reg.get_entity(&TypeId("LLM".into())).unwrap();
        assert_eq!(llm.category, EntityCategory::System);
        let provider = reg.get_entity(&TypeId("Provider".into())).unwrap();
        assert_eq!(provider.category, EntityCategory::System);
        let cap = reg.get_entity(&TypeId("Capability".into())).unwrap();
        assert_eq!(cap.category, EntityCategory::System);
    }

    #[test]
    fn test_system_actions_are_system_category() {
        let reg = system_resource_ontology();
        let query = reg.get_action("model_query").unwrap();
        assert_eq!(query.category, EntityCategory::System);
        let switch = reg.get_action("model_switch").unwrap();
        assert_eq!(switch.category, EntityCategory::System);
    }

    #[test]
    fn test_full_ontology_combines_both() {
        let reg = full_ontology();
        assert_eq!(reg.entities.len(), 6, "3 domain + 3 system entities");
        assert_eq!(reg.actions.len(), 11, "9 domain + 2 system actions");
    }

    #[test]
    fn test_category_queries() {
        let reg = full_ontology();
        let domain_entities = reg.entities_by_category(EntityCategory::Domain);
        let system_entities = reg.entities_by_category(EntityCategory::System);
        assert_eq!(domain_entities.len(), 3);
        assert_eq!(system_entities.len(), 3);
        let domain_actions = reg.actions_by_category(EntityCategory::Domain);
        let system_actions = reg.actions_by_category(EntityCategory::System);
        assert_eq!(domain_actions.len(), 9);
        assert_eq!(system_actions.len(), 2);
    }

    #[test]
    fn test_system_action_field_aliases() {
        let reg = system_resource_ontology();
        let aliases = reg.field_aliases_by_category(EntityCategory::System);
        assert_eq!(aliases.get("filter").unwrap(), "criteria");
        assert_eq!(aliases.get("target_model").unwrap(), "model_id");
    }

    #[test]
    fn test_system_entity_field_aliases() {
        let reg = system_resource_ontology();
        let aliases = reg.entity_field_aliases_by_category(EntityCategory::System);
        assert_eq!(aliases.get("model").unwrap(), "model_id");
        assert_eq!(aliases.get("max_context").unwrap(), "context_window");
        assert_eq!(aliases.get("vision").unwrap(), "supports_vision");
        assert_eq!(aliases.get("input_cost").unwrap(), "pricing_input_per_million");
    }

    #[test]
    fn test_llm_entity_defaults() {
        let reg = system_resource_ontology();
        let defaults = reg.defaults_by_category(EntityCategory::System);
        assert_eq!(defaults["model_query"]["sort_by"], "cost");
    }

    #[test]
    fn test_model_query_is_readonly() {
        let reg = system_resource_ontology();
        let query = reg.get_action("model_query").unwrap();
        assert!(query.is_read_only());
    }

    #[test]
    fn test_model_switch_is_destructive() {
        let reg = system_resource_ontology();
        let switch = reg.get_action("model_switch").unwrap();
        assert_eq!(switch.execution_category, ExecutionCategory::Destructive);
        assert!(!switch.is_read_only());
    }

    // ═══════════════════════════════════════════════════════════
    // LinkDef Tests
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn test_domain_ontology_has_3_links() {
        let reg = coding_agent_ontology();
        assert_eq!(reg.links.len(), 3, "expected 3 domain links");
    }

    #[test]
    fn test_system_ontology_has_2_links() {
        let reg = system_resource_ontology();
        assert_eq!(reg.links.len(), 2, "expected 2 system links");
    }

    #[test]
    fn test_full_ontology_has_5_links() {
        let reg = full_ontology();
        assert_eq!(reg.links.len(), 5, "3 domain + 2 system links");
    }

    #[test]
    fn test_link_workspace_contains_file() {
        let reg = coding_agent_ontology();
        let link = reg.get_link(&TypeId("Workspace_contains_File".into())).unwrap();
        assert_eq!(link.source_type, TypeId("Workspace".into()));
        assert_eq!(link.target_type, TypeId("File".into()));
        assert_eq!(link.cardinality, Cardinality::OneToMany);
        assert_eq!(
            link.inverse,
            Some(TypeId("File_belongs_to_Workspace".into()))
        );
    }

    #[test]
    fn test_link_provider_provides_llm() {
        let reg = system_resource_ontology();
        let link = reg
            .get_link(&TypeId("Provider_provides_LLM".into()))
            .unwrap();
        assert_eq!(link.source_type, TypeId("Provider".into()));
        assert_eq!(link.target_type, TypeId("LLM".into()));
        assert_eq!(link.cardinality, Cardinality::OneToMany);
    }

    #[test]
    fn test_links_from() {
        let reg = full_ontology();
        let ws_links = reg.links_from(&TypeId("Workspace".into()));
        assert_eq!(ws_links.len(), 2);
        let provider_links = reg.links_from(&TypeId("Provider".into()));
        assert_eq!(provider_links.len(), 1);
    }

    #[test]
    fn test_links_to() {
        let reg = full_ontology();
        let file_links = reg.links_to(&TypeId("File".into()));
        assert_eq!(file_links.len(), 2); // Workspace→File, Module→File
    }

    #[test]
    fn test_inverse_link() {
        let reg = full_ontology();
        let inv = reg.inverse_link(&TypeId("Workspace_contains_File".into()));
        assert!(inv.is_none()); // inverse ID exists but link not registered

        let link = reg.get_link(&TypeId("Workspace_contains_File".into())).unwrap();
        assert_eq!(
            link.inverse,
            Some(TypeId("File_belongs_to_Workspace".into()))
        );
    }

    #[test]
    fn test_llm_capability_link() {
        let reg = system_resource_ontology();
        let link = reg
            .get_link(&TypeId("LLM_has_Capability".into()))
            .unwrap();
        assert_eq!(link.source_type, TypeId("LLM".into()));
        assert_eq!(link.target_type, TypeId("Capability".into()));
        assert_eq!(link.cardinality, Cardinality::OneToMany);
    }
}
