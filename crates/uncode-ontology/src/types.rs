//! Core ontology types: TypeId, EntityDef, ActionDef, FieldDef

use std::borrow::Borrow;

use serde::{Deserialize, Serialize};

/// Entity category: distinguishes domain semantics from system resource semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EntityCategory {
    #[default]
    Domain,
    System,
}

impl EntityCategory {
    #[must_use]
    pub fn is_system(&self) -> bool {
        matches!(self, Self::System)
    }
}

impl std::fmt::Display for EntityCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Domain => f.write_str("domain"),
            Self::System => f.write_str("system"),
        }
    }
}

/// String-based type identifier for debuggability and LLM context clarity.
///
/// # Examples
///
/// ```
/// use uncode_ontology::TypeId;
///
/// let id1 = TypeId::from("File");
/// let id2: TypeId = "Workspace".into();
/// assert_eq!(id1, TypeId("File".into()));
/// assert_ne!(id1, id2);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TypeId(pub String);

impl TypeId {
    pub const STRING: Self = TypeId(String::new());
}

impl std::fmt::Display for TypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for TypeId {
    fn from(s: String) -> Self {
        TypeId(s)
    }
}

impl From<&str> for TypeId {
    fn from(s: &str) -> Self {
        TypeId(s.to_owned())
    }
}

impl std::ops::Deref for TypeId {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for TypeId {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Borrow<str> for TypeId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// Entity type definition (≈ Palantir Object Type).
///
/// Fields, invariants, and extends together define the entity's complete
/// shape after inheritance resolution. `extends` chains are resolved lazily
/// by `TypeRegistry::resolve_entity()`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityDef {
    pub id: TypeId,
    pub fields: Vec<FieldDef>,
    /// Entity-level constraints that apply to all actions referencing this entity.
    #[serde(default)]
    pub invariants: Vec<Constraint>,
    /// Parent entity type to inherit fields and invariants from.
    #[serde(default)]
    pub extends: Option<TypeId>,
    /// Category: Domain or System.
    #[serde(default)]
    pub category: EntityCategory,
    pub description: Option<String>,
}

/// Action type definition (≈ Palantir Action Type).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionDef {
    pub name: String,
    pub fields: Vec<FieldDef>,
    pub output_type: TypeId,
    /// Category: Domain (domain actions like read/write) or System (system actions like llm_query).
    #[serde(default)]
    pub category: EntityCategory,
    pub preconditions: Vec<Constraint>,
    pub effects: Vec<Effect>,
    pub execution_category: ExecutionCategory,
    pub description: Option<String>,
}

/// Field definition with aliases for LLM output normalization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDef {
    pub name: String,
    pub value_type: String,
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub description: Option<String>,
}

/// Execution category replaces hardcoded permission logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ExecutionCategory {
    ReadOnly,
    Destructive,
    Network,
    Shell,
    Unknown,
}

/// Constraint axioms for validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Constraint {
    RequiredField {
        field: String,
    },
    TypeCheck {
        field: String,
        expected: String,
        #[serde(default = "default_hard")]
        level: ConstraintLevel,
    },
    RangeCheck {
        field: String,
        min: Option<f64>,
        max: Option<f64>,
        #[serde(default = "default_hard")]
        level: ConstraintLevel,
    },
    EnumCheck {
        field: String,
        allowed: Vec<String>,
        #[serde(default = "default_hard")]
        level: ConstraintLevel,
    },
    RegexMatch {
        field: String,
        pattern: String,
        description: String,
        #[serde(default = "default_hard")]
        level: ConstraintLevel,
    },
    CustomRule {
        name: String,
        description: String,
        #[serde(default = "default_hard")]
        level: ConstraintLevel,
    },
}

fn default_hard() -> ConstraintLevel {
    ConstraintLevel::default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConstraintLevel {
    #[default]
    Hard,
    Soft,
}

/// Side-effect declaration for adjudication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Effect {
    Read {
        target: String,
        #[serde(default)]
        fields: Vec<String>,
    },
    Create {
        entity: String,
    },
    Modify {
        entity: String,
        #[serde(default)]
        fields: Vec<String>,
    },
    Delete {
        entity: String,
    },
    Exec {
        command: String,
    },
    Network {
        destination: String,
    },
}

impl std::fmt::Display for Effect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read { target, .. } => write!(f, "Read({target})"),
            Self::Create { entity } => write!(f, "Create({entity})"),
            Self::Modify { entity, .. } => write!(f, "Modify({entity})"),
            Self::Delete { entity } => write!(f, "Delete({entity})"),
            Self::Exec { command } => write!(f, "Exec({command})"),
            Self::Network { destination } => write!(f, "Network({destination})"),
        }
    }
}

impl Effect {
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        matches!(self, Self::Read { .. })
    }
}

impl ActionDef {
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        self.effects.iter().all(|e| e.is_read_only())
    }

    /// Generate JSON Schema for this action's parameters from `fields`.
    ///
    /// Returns a JSON Schema `{ "type": "object", "properties": { ... }, "required": [ ... ] }`
    /// suitable for use as `ToolDefinition.parameters`.
    #[must_use]
    pub fn to_json_schema(&self) -> serde_json::Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for field in &self.fields {
            let mut prop = serde_json::Map::new();
            prop.insert(
                "type".into(),
                serde_json::Value::String(field.value_type.clone()),
            );
            if let Some(ref desc) = field.description {
                prop.insert(
                    "description".into(),
                    serde_json::Value::String(desc.clone()),
                );
            }
            if field.required {
                required.push(serde_json::Value::String(field.name.clone()));
            }
            properties.insert(field.name.clone(), serde_json::Value::Object(prop));
        }

        let mut schema = serde_json::Map::new();
        schema.insert("type".into(), serde_json::Value::String("object".into()));
        schema.insert(
            "additionalProperties".into(),
            serde_json::Value::Bool(false),
        );
        schema.insert(
            "properties".into(),
            serde_json::Value::Object(properties),
        );
        if !required.is_empty() {
            schema.insert(
                "required".into(),
                serde_json::Value::Array(required),
            );
        }

        serde_json::Value::Object(schema)
    }
}

/// Link cardinality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Cardinality {
    OneToOne,
    OneToMany,
    ManyToMany,
}

/// Link definition — declares a relationship between two entity types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkDef {
    pub id: TypeId,
    pub source_type: TypeId,
    pub target_type: TypeId,
    pub cardinality: Cardinality,
    pub inverse: Option<TypeId>,
    pub description: Option<String>,
}

/// Reasoning rule — derives implicit knowledge from ontology data.
///
/// Two rule types:
/// - **Traversal**: follow a LinkDef to collect related entities
/// - **Derivation**: from known field values, compute derived field values
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReasoningRule {
    /// Traverse a link to collect related entity IDs.
    ///
    /// Example: given Provider "deepseek", follow `Provider_provides_LLM`
    /// to find all LLM models from that provider.
    Traversal {
        id: TypeId,
        /// The link to follow
        link_id: TypeId,
        /// The entity type that provides the starting point
        source_type: TypeId,
        /// The entity type discovered at the other end
        target_type: TypeId,
        #[serde(default)]
        description: Option<String>,
    },
    /// Derive a field value from other fields on the same entity.
    ///
    /// Example: if `supports_vision = true`, derive `input_modalities` contains "Image".
    Derivation {
        id: TypeId,
        /// The entity type this rule applies to
        entity_type: TypeId,
        /// Source field(s) to read
        source_fields: Vec<String>,
        /// Derived field to write
        derived_field: String,
        /// Derivation logic as a declarative expression
        expression: DerivationExpr,
        #[serde(default)]
        description: Option<String>,
    },
}

/// Declarative derivation expression.
///
/// These are simple, deterministic transformations — not a Turing-complete language.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum DerivationExpr {
    /// If source_field equals expected_value, set derived_field to result_value.
    FieldEquals {
        field: String,
        expected: serde_json::Value,
        result: serde_json::Value,
    },
    /// If source_field is true, set derived_field to result_value.
    FieldIsTrue {
        field: String,
        result: serde_json::Value,
    },
    /// Compute a numeric result from two fields.
    Arithmetic {
        left_field: String,
        operator: ArithmeticOp,
        right_field: String,
    },
    /// Copy a field value to another field (alias).
    Alias { source: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mk_field(name: &str, value_type: &str, required: bool) -> FieldDef {
        FieldDef {
            name: name.into(),
            value_type: value_type.into(),
            required,
            default: None,
            aliases: vec![],
            description: None,
        }
    }

    // ── to_json_schema tests ──

    #[test]
    fn test_to_json_schema_empty() {
        let action = ActionDef {
            name: "empty".into(),
            fields: vec![],
            output_type: TypeId::STRING,
            category: EntityCategory::Domain,
            preconditions: vec![],
            effects: vec![],
            execution_category: ExecutionCategory::ReadOnly,
            description: None,
        };
        let schema = action.to_json_schema();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["properties"], json!({}));
        assert!(schema.get("required").is_none());
    }

    #[test]
    fn test_to_json_schema_single_field() {
        let action = ActionDef {
            name: "read".into(),
            fields: vec![mk_field("path", "string", true)],
            output_type: TypeId::STRING,
            category: EntityCategory::Domain,
            preconditions: vec![],
            effects: vec![],
            execution_category: ExecutionCategory::ReadOnly,
            description: None,
        };
        let schema = action.to_json_schema();
        assert_eq!(schema["properties"]["path"]["type"], "string");
        let required: Vec<_> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(required, vec!["path"]);
    }

    #[test]
    fn test_to_json_schema_optional_field() {
        let action = ActionDef {
            name: "read".into(),
            fields: vec![
                mk_field("path", "string", true),
                FieldDef {
                    name: "offset".into(),
                    value_type: "integer".into(),
                    required: false,
                    default: Some(json!(0)),
                    aliases: vec![],
                    description: Some("跳过的行数".into()),
                },
            ],
            output_type: TypeId::STRING,
            category: EntityCategory::Domain,
            preconditions: vec![],
            effects: vec![],
            execution_category: ExecutionCategory::ReadOnly,
            description: None,
        };
        let schema = action.to_json_schema();
        assert_eq!(schema["properties"]["offset"]["type"], "integer");
        assert_eq!(schema["properties"]["offset"]["description"], "跳过的行数");
        // offset is optional, should not appear in required
        let required: Vec<_> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(required, vec!["path"]);
    }

    #[test]
    fn test_to_json_schema_boolean_field() {
        let action = ActionDef {
            name: "read".into(),
            fields: vec![FieldDef {
                name: "hashline".into(),
                value_type: "boolean".into(),
                required: false,
                default: Some(json!(false)),
                aliases: vec![],
                description: Some("行号#哈希锚点".into()),
            }],
            output_type: TypeId::STRING,
            category: EntityCategory::Domain,
            preconditions: vec![],
            effects: vec![],
            execution_category: ExecutionCategory::ReadOnly,
            description: None,
        };
        let schema = action.to_json_schema();
        assert_eq!(schema["properties"]["hashline"]["type"], "boolean");
    }
}

/// Arithmetic operation for derivation expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ArithmeticOp {
    Add,
    Subtract,
    Multiply,
    Divide,
}

/// Ontology version — semver-compatible.
///
/// Version bump rules:
/// - **Patch** (0.0.x): bug fixes, no schema change
/// - **Minor** (0.x.0): backward-compatible additions (new entities, new fields with defaults, new links/rules)
/// - **Major** (x.0.0): breaking changes (removed entities/fields, changed field types)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OntologyVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl OntologyVersion {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Two versions are compatible if they share the same major version
    /// and the stored version is >= the required version at the minor level.
    #[must_use]
    pub fn is_compatible_with(&self, required: &OntologyVersion) -> bool {
        self.major == required.major
            && (self.minor > required.minor
                || (self.minor == required.minor && self.patch >= required.patch))
    }
}

impl std::fmt::Display for OntologyVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}
