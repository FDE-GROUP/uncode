use std::collections::HashMap;

use uncode_ontology::{
    ActionDef, Cardinality, Constraint, ConstraintLevel, DerivationExpr, EntityCategory, EntityDef,
    ExecutionCategory, FieldDef, LinkDef, OntologyVersion, TypeId, TypeRegistry,
    evaluate_constraint,
};

#[test]
fn empty_registry_has_default_version() {
    let reg = TypeRegistry::new();
    assert_eq!(reg.version, OntologyVersion::new(0, 1, 0));
}

#[test]
fn registry_with_version() {
    let reg = TypeRegistry::with_version(OntologyVersion::new(2, 3, 4));
    assert_eq!(reg.version, OntologyVersion::new(2, 3, 4));
}

#[test]
fn register_and_get_entity() {
    let mut reg = TypeRegistry::new();
    let id = TypeId::from("TestEntity");
    reg.register_entity(EntityDef {
        id: id.clone(),
        description: Some("A test entity".into()),
        category: EntityCategory::Domain,
        fields: vec![],
        invariants: vec![],
        extends: None,
    });
    let entity = reg.get_entity(&id);
    assert!(entity.is_some());
    assert_eq!(entity.unwrap().id, TypeId::from("TestEntity"));
}

#[test]
fn register_and_get_action() {
    let mut reg = TypeRegistry::new();
    reg.register_action(ActionDef {
        name: "test_action".into(),
        output_type: TypeId::from("string"),
        description: Some("A test action".into()),
        category: EntityCategory::Domain,
        execution_category: ExecutionCategory::ReadOnly,
        fields: vec![FieldDef {
            name: "path".into(),
            value_type: TypeId::string(),
            required: true,
            default: None,
            aliases: vec!["file".into(), "filepath".into()],
            description: None,
        }],
        preconditions: vec![],
        effects: vec![],
        path_fields: vec![],
    });
    let action = reg.get_action("test_action");
    assert!(action.is_some());
    assert_eq!(action.unwrap().name, "test_action");
}

#[test]
fn register_and_query_link() {
    let mut reg = TypeRegistry::new();
    let link = LinkDef {
        id: TypeId::from("Entity_contains_Entity"),
        source_type: TypeId::from("EntityA"),
        target_type: TypeId::from("EntityB"),
        cardinality: Cardinality::OneToMany,
        inverse: Some(TypeId::from("Entity_contained_by_Entity")),
        description: Some("A links to B".into()),
    };
    let inverse = LinkDef {
        id: TypeId::from("Entity_contained_by_Entity"),
        source_type: TypeId::from("EntityB"),
        target_type: TypeId::from("EntityA"),
        cardinality: Cardinality::OneToOne,
        inverse: Some(TypeId::from("Entity_contains_Entity")),
        description: Some("B linked by A".into()),
    };
    reg.register_link(link);
    reg.register_link(inverse);
    let links = reg.links_from(&TypeId::from("EntityA"));
    assert!(!links.is_empty());
    let found = reg.inverse_link(&TypeId::from("Entity_contains_Entity"));
    assert!(found.is_some());
}

#[test]
fn constraint_type_check_pass() {
    let mut fields = HashMap::new();
    fields.insert("path".into(), serde_json::json!("src/main.rs"));
    let constraint = Constraint::TypeCheck {
        field: "path".into(),
        expected: "string".into(),
        level: ConstraintLevel::Hard,
    };
    let result = evaluate_constraint(&constraint, &fields);
    assert!(result.is_pass());
}

#[test]
fn constraint_required_field_missing() {
    let fields: HashMap<String, serde_json::Value> = HashMap::new();
    let constraint = Constraint::RequiredField {
        field: "name".into(),
    };
    let result = evaluate_constraint(&constraint, &fields);
    assert!(!result.is_pass());
}

#[test]
fn ontology_version_compatibility() {
    let v1_2_0 = OntologyVersion::new(1, 2, 0);
    let v1_3_0 = OntologyVersion::new(1, 3, 0);
    let v2_0_0 = OntologyVersion::new(2, 0, 0);
    assert!(v1_3_0.is_compatible_with(&v1_2_0));
    assert!(!v1_2_0.is_compatible_with(&v2_0_0));
}

#[test]
fn merge_registry_preserves_higher_version() {
    let mut r1 = TypeRegistry::with_version(OntologyVersion::new(1, 0, 0));
    r1.register_entity(EntityDef {
        id: TypeId::from("E1"),
        description: None,
        category: EntityCategory::Domain,
        fields: vec![],
        invariants: vec![],
        extends: None,
    });
    let mut r2 = TypeRegistry::with_version(OntologyVersion::new(2, 0, 0));
    r2.register_entity(EntityDef {
        id: TypeId::from("E2"),
        description: None,
        category: EntityCategory::Domain,
        fields: vec![],
        invariants: vec![],
        extends: None,
    });
    let merged = r1.merge(&r2);
    assert_eq!(merged.version, OntologyVersion::new(2, 0, 0));
    assert!(merged.get_entity(&TypeId::from("E1")).is_some());
    assert!(merged.get_entity(&TypeId::from("E2")).is_some());
}

#[test]
fn entity_category_queries() {
    let mut reg = TypeRegistry::new();
    reg.register_action(ActionDef {
        name: "domain_action".into(),
        output_type: TypeId::from("string"),
        description: None,
        category: EntityCategory::Domain,
        execution_category: ExecutionCategory::ReadOnly,
        fields: vec![],
        preconditions: vec![],
        effects: vec![],
        path_fields: vec![],
    });
    reg.register_action(ActionDef {
        name: "system_action".into(),
        output_type: TypeId::from("string"),
        description: None,
        category: EntityCategory::System,
        execution_category: ExecutionCategory::ReadOnly,
        fields: vec![],
        preconditions: vec![],
        effects: vec![],
        path_fields: vec![],
    });
    let domain = reg.actions_by_category(EntityCategory::Domain);
    assert_eq!(domain.len(), 1);
    let system = reg.actions_by_category(EntityCategory::System);
    assert_eq!(system.len(), 1);
}

#[test]
fn reasoning_rule_derivation_evaluation() {
    use uncode_ontology::{ReasoningRule, evaluate_derivation};
    let rule = ReasoningRule::Derivation {
        id: TypeId::from("test_rule"),
        entity_type: TypeId::from("LLM"),
        source_fields: vec!["has_vision".into()],
        derived_field: "supports_images".into(),
        expression: DerivationExpr::FieldIsTrue {
            field: "has_vision".into(),
            result: serde_json::json!(true),
        },
        description: None,
    };
    let mut fields = HashMap::new();
    fields.insert("has_vision".into(), serde_json::json!(true));
    let result = evaluate_derivation(&rule, &fields);
    assert!(result.is_some());
    let r = result.unwrap();
    assert_eq!(r.derived_field, "supports_images");
    assert_eq!(r.value, serde_json::json!(true));
}

#[test]
fn resolve_entity_no_extends() {
    let mut reg = TypeRegistry::new();
    reg.register_entity(EntityDef {
        id: TypeId::from("File"),
        description: None,
        category: EntityCategory::Domain,
        fields: vec![FieldDef {
            name: "path".into(),
            value_type: "string".into(),
            required: true,
            default: None,
            aliases: vec![],
            description: None,
        }],
        invariants: vec![Constraint::RequiredField {
            field: "path".into(),
        }],
        extends: None,
    });
    let resolved = reg.resolve_entity(&TypeId::from("File")).unwrap();
    assert_eq!(resolved.id, TypeId::from("File"));
    assert_eq!(resolved.fields.len(), 1);
    assert_eq!(resolved.invariants.len(), 1);
}

#[test]
fn resolve_entity_with_extends_merges_fields() {
    let mut reg = TypeRegistry::new();
    reg.register_entity(EntityDef {
        id: TypeId::from("Base"),
        description: None,
        category: EntityCategory::Domain,
        fields: vec![FieldDef {
            name: "id".into(),
            value_type: "string".into(),
            required: true,
            default: None,
            aliases: vec![],
            description: None,
        }],
        invariants: vec![Constraint::RequiredField { field: "id".into() }],
        extends: None,
    });
    reg.register_entity(EntityDef {
        id: TypeId::from("Derived"),
        description: None,
        category: EntityCategory::Domain,
        fields: vec![FieldDef {
            name: "name".into(),
            value_type: "string".into(),
            required: false,
            default: None,
            aliases: vec![],
            description: None,
        }],
        invariants: vec![],
        extends: Some(TypeId::from("Base")),
    });
    let resolved = reg.resolve_entity(&TypeId::from("Derived")).unwrap();
    assert_eq!(resolved.id, TypeId::from("Derived"));
    assert_eq!(resolved.fields.len(), 2);
    assert_eq!(resolved.invariants.len(), 1);
}

#[test]
fn resolve_entity_child_field_overrides_parent() {
    let mut reg = TypeRegistry::new();
    reg.register_entity(EntityDef {
        id: TypeId::from("Base"),
        description: None,
        category: EntityCategory::Domain,
        fields: vec![FieldDef {
            name: "x".into(),
            value_type: "integer".into(),
            required: true,
            default: None,
            aliases: vec![],
            description: None,
        }],
        invariants: vec![],
        extends: None,
    });
    reg.register_entity(EntityDef {
        id: TypeId::from("Derived"),
        description: None,
        category: EntityCategory::Domain,
        fields: vec![FieldDef {
            name: "x".into(),
            value_type: "string".into(),
            required: false,
            default: None,
            aliases: vec![],
            description: None,
        }],
        invariants: vec![],
        extends: Some(TypeId::from("Base")),
    });
    let resolved = reg.resolve_entity(&TypeId::from("Derived")).unwrap();
    assert_eq!(resolved.fields.len(), 1, "child field overrides parent");
    assert_eq!(resolved.fields[0].value_type, TypeId::from("string"));
    assert!(!resolved.fields[0].required);
}
