//! uncode-ontology — type registry, constraint axioms, action metadata.
//!
//! Implements the "ontology wheel" from the Cognitive Explicitation & Decision-Driven
//! Design paradigm. Provides a central TypeRegistry for all shared concepts in the system:
//!
//! - **Domain semantic ontology** — File, Workspace, Module, Action definitions.
//!   Consumed by the semantic firewall (parser, validator, normalizer) and adjudicator.
//!
//! - **System resource ontology** — LLM, Provider, Capability definitions.
//!   Consumed by model routing, cost governance, and capability queries.
//!
//! Both categories share the same TypeRegistry / Constraint / Effect infrastructure,
//! distinguished by the `EntityCategory` enum.

pub mod builtin;
pub mod evaluate;
pub mod registry;
pub mod types;

pub use evaluate::{ConstraintResult, evaluate_constraint};
pub use registry::TypeRegistry;
pub use types::{
    ActionDef, Cardinality, Constraint, ConstraintLevel, Effect, EntityCategory, EntityDef,
    ExecutionCategory, FieldDef, LinkDef, TypeId,
};
