//! Core ontology types: TypeId, EntityDef, ActionDef, FieldDef

use serde::{Deserialize, Serialize};

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
    pub description: Option<String>,
}

/// Action type definition (≈ Palantir Action Type).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionDef {
    pub name: String,
    pub fields: Vec<FieldDef>,
    pub output_type: TypeId,
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
