//! Builtin coding agent ontology — 9 tools + 3 entities.

use crate::registry::TypeRegistry;
use crate::types::{ActionDef, Constraint, Effect, EntityDef, ExecutionCategory, FieldDef, TypeId};

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

    reg
}

fn entity_file() -> EntityDef {
    EntityDef {
        id: TypeId("File".into()),
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
}
