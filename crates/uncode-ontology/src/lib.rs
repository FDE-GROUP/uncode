//! uncode-ontology — type registry, constraint axioms, action metadata.
//!
//! Implements the "ontology wheel" from the Cognitive Explicitation & Decision-Driven
//! Design paradigm. Provides a central TypeRegistry for domain knowledge that feeds
//! the semantic firewall (normalizer, validator) and adjudicator (effect-based policy).

pub mod builtin;
pub mod evaluate;
pub mod registry;
pub mod types;

pub use evaluate::{ConstraintResult, evaluate_constraint};
pub use registry::TypeRegistry;
pub use types::{
    ActionDef, Constraint, ConstraintLevel, Effect, EntityDef, ExecutionCategory, FieldDef, TypeId,
};
