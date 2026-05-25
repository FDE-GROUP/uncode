//! Core ontology types: TypeId, EntityDef, ActionDef, FieldDef

use serde::{Deserialize, Serialize};

/// Entity category: distinguishes domain semantics from system resource semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityCategory {
    /// Domain entities: File, Workspace, Module, Action — consumed by the semantic firewall.
    Domain,
    /// System resource entities: LLM, Provider, Capability — consumed by model routing / cost governance.
    System,
}

impl Default for EntityCategory {
    fn default() -> Self {
        Self::Domain
    }
}

/// String-based type identifier for debuggability and LLM context clarity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TypeId(pub String);

impl TypeId {
    pub const STRING: Self = TypeId(String::new());
}

impl std::fmt::Display for TypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Entity type definition (≈ Palantir Object Type).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDef {
    pub id: TypeId,
    pub fields: Vec<FieldDef>,
    /// Category: Domain (domain semantics) or System (resource semantics).
    #[serde(default)]
    pub category: EntityCategory,
    pub description: Option<String>,
}

/// Action type definition (≈ Palantir Action Type).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionDef {
    pub name: String,
    pub fields: Vec<FieldDef>,
    pub output_type: TypeId,
    /// Category: Domain (domain actions like read/write) or System (system actions like model_query).
    #[serde(default)]
    pub category: EntityCategory,
    pub preconditions: Vec<Constraint>,
    pub effects: Vec<Effect>,
    pub execution_category: ExecutionCategory,
    pub description: Option<String>,
}

/// Field definition with aliases for LLM output normalization.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub enum ExecutionCategory {
    ReadOnly,
    Destructive,
    Network,
    Shell,
    Unknown,
}

/// Constraint axioms for validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
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
    ConstraintLevel::Hard
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintLevel {
    Hard,
    Soft,
}

/// Side-effect declaration for adjudication.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
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
    pub fn is_read_only(&self) -> bool {
        matches!(self, Self::Read { .. })
    }
}

impl ActionDef {
    pub fn is_read_only(&self) -> bool {
        self.effects.iter().all(|e| e.is_read_only())
    }
}

/// Link cardinality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    OneToOne,
    OneToMany,
    ManyToMany,
}

/// Link definition — declares a relationship between two entity types.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArithmeticOp {
    Add,
    Subtract,
    Multiply,
    Divide,
}
