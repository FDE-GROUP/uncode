//! Reasoning engine — evaluates ReasoningRule against TypeRegistry data.
//!
//! Two reasoning modes:
//! - **Traversal**: follow LinkDef to find related entities
//! - **Derivation**: compute derived field values from known fields
//!
//! Both are deterministic, single-pass — no iterative fixpoint computation.

use std::collections::HashMap;

use crate::types::{ArithmeticOp, DerivationExpr, ReasoningRule, TypeId};

/// Result of a traversal query — entity IDs reachable via a link.
#[derive(Debug, Clone)]
pub struct TraversalResult {
    pub link_id: TypeId,
    pub source_id: TypeId,
    pub target_ids: Vec<TypeId>,
}

/// Result of a derivation — a single derived field value.
#[derive(Debug, Clone)]
pub struct DerivationResult {
    pub rule_id: TypeId,
    pub derived_field: String,
    pub value: serde_json::Value,
}

/// Evaluate a traversal rule against the registry's link data.
///
/// Given a source entity type and a link, returns all target entity TypeIds.
/// Note: this returns entity **types**, not instances. Instance-level traversal
/// requires runtime data (e.g., Model instances), which is handled by the bridge.
pub fn evaluate_traversal(
    registry: &crate::TypeRegistry,
    rule: &ReasoningRule,
) -> Option<TraversalResult> {
    let ReasoningRule::Traversal {
        id: _,
        link_id,
        source_type,
        target_type,
        ..
    } = rule
    else {
        return None;
    };

    let link = registry.get_link(link_id)?;
    if link.source_type != *source_type || link.target_type != *target_type {
        return None;
    }

    Some(TraversalResult {
        link_id: link_id.clone(),
        source_id: source_type.clone(),
        target_ids: vec![target_type.clone()],
    })
}

/// Evaluate a derivation rule against field values.
///
/// Returns the derived field value if the rule fires, or None.
pub fn evaluate_derivation(
    rule: &ReasoningRule,
    fields: &HashMap<String, serde_json::Value>,
) -> Option<DerivationResult> {
    let ReasoningRule::Derivation {
        id,
        derived_field,
        expression,
        ..
    } = rule
    else {
        return None;
    };

    let value = evaluate_expression(expression, fields)?;

    Some(DerivationResult {
        rule_id: id.clone(),
        derived_field: derived_field.clone(),
        value,
    })
}

fn evaluate_expression(
    expr: &DerivationExpr,
    fields: &HashMap<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    match expr {
        DerivationExpr::FieldEquals {
            field,
            expected,
            result,
        } => {
            if fields.get(field) == Some(expected) {
                Some(result.clone())
            } else {
                None
            }
        }
        DerivationExpr::FieldIsTrue { field, result } => {
            if fields.get(field).and_then(|v| v.as_bool()) == Some(true) {
                Some(result.clone())
            } else {
                None
            }
        }
        DerivationExpr::Arithmetic {
            left_field,
            operator,
            right_field,
        } => {
            let left = fields.get(left_field)?.as_f64()?;
            let right = fields.get(right_field)?.as_f64()?;
            let result = match operator {
                ArithmeticOp::Add => left + right,
                ArithmeticOp::Subtract => left - right,
                ArithmeticOp::Multiply => left * right,
                ArithmeticOp::Divide => {
                    if right == 0.0 {
                        return None;
                    }
                    left / right
                }
            };
            Some(serde_json::json!(result))
        }
        DerivationExpr::Alias { source } => fields.get(source).cloned(),
    }
}

/// Evaluate all derivation rules against field values.
///
/// Returns all derived field values that fire.
pub fn evaluate_all_derivations(
    rules: &[ReasoningRule],
    fields: &HashMap<String, serde_json::Value>,
) -> Vec<DerivationResult> {
    rules
        .iter()
        .filter_map(|rule| evaluate_derivation(rule, fields))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ReasoningRule;

    fn test_fields() -> HashMap<String, serde_json::Value> {
        let mut m = HashMap::new();
        m.insert("supports_vision".into(), serde_json::json!(true));
        m.insert("supports_reasoning".into(), serde_json::json!(false));
        m.insert("context_window".into(), serde_json::json!(128_000));
        m.insert("max_output_tokens".into(), serde_json::json!(8192));
        m.insert("pricing_input".into(), serde_json::json!(1.0));
        m.insert("pricing_output".into(), serde_json::json!(2.0));
        m
    }

    #[test]
    fn test_field_is_true_fires() {
        let rule = ReasoningRule::Derivation {
            id: TypeId("vision_implies_image_modality".into()),
            entity_type: TypeId("LLM".into()),
            source_fields: vec!["supports_vision".into()],
            derived_field: "input_modality_image".into(),
            expression: DerivationExpr::FieldIsTrue {
                field: "supports_vision".into(),
                result: serde_json::json!(true),
            },
            description: None,
        };
        let result = evaluate_derivation(&rule, &test_fields()).unwrap();
        assert_eq!(result.derived_field, "input_modality_image");
        assert_eq!(result.value, serde_json::json!(true));
    }

    #[test]
    fn test_field_is_true_no_fire() {
        let rule = ReasoningRule::Derivation {
            id: TypeId("reasoning_implies_thinking".into()),
            entity_type: TypeId("LLM".into()),
            source_fields: vec!["supports_reasoning".into()],
            derived_field: "supports_thinking".into(),
            expression: DerivationExpr::FieldIsTrue {
                field: "supports_reasoning".into(),
                result: serde_json::json!(true),
            },
            description: None,
        };
        assert!(evaluate_derivation(&rule, &test_fields()).is_none());
    }

    #[test]
    fn test_field_equals_fires() {
        let rule = ReasoningRule::Derivation {
            id: TypeId("protocol_implies_tool_support".into()),
            entity_type: TypeId("LLM".into()),
            source_fields: vec!["api_protocol".into()],
            derived_field: "supports_function_calling".into(),
            expression: DerivationExpr::FieldEquals {
                field: "api_protocol".into(),
                expected: serde_json::json!("openai-completions"),
                result: serde_json::json!(true),
            },
            description: None,
        };
        let mut fields = test_fields();
        fields.insert("api_protocol".into(), serde_json::json!("openai-completions"));
        let result = evaluate_derivation(&rule, &fields).unwrap();
        assert_eq!(result.value, serde_json::json!(true));
    }

    #[test]
    fn test_arithmetic_add() {
        let rule = ReasoningRule::Derivation {
            id: TypeId("total_pricing".into()),
            entity_type: TypeId("LLM".into()),
            source_fields: vec!["pricing_input".into(), "pricing_output".into()],
            derived_field: "total_pricing_per_million".into(),
            expression: DerivationExpr::Arithmetic {
                left_field: "pricing_input".into(),
                operator: ArithmeticOp::Add,
                right_field: "pricing_output".into(),
            },
            description: None,
        };
        let result = evaluate_derivation(&rule, &test_fields()).unwrap();
        assert_eq!(result.value, serde_json::json!(3.0));
    }

    #[test]
    fn test_arithmetic_divide() {
        let rule = ReasoningRule::Derivation {
            id: TypeId("context_ratio".into()),
            entity_type: TypeId("LLM".into()),
            source_fields: vec!["max_output_tokens".into(), "context_window".into()],
            derived_field: "output_context_ratio".into(),
            expression: DerivationExpr::Arithmetic {
                left_field: "max_output_tokens".into(),
                operator: ArithmeticOp::Divide,
                right_field: "context_window".into(),
            },
            description: None,
        };
        let result = evaluate_derivation(&rule, &test_fields()).unwrap();
        assert!((result.value.as_f64().unwrap() - 0.064).abs() < 1e-6);
    }

    #[test]
    fn test_arithmetic_divide_by_zero() {
        let rule = ReasoningRule::Derivation {
            id: TypeId("bad_divide".into()),
            entity_type: TypeId("LLM".into()),
            source_fields: vec!["pricing_input".into()],
            derived_field: "bad".into(),
            expression: DerivationExpr::Arithmetic {
                left_field: "pricing_input".into(),
                operator: ArithmeticOp::Divide,
                right_field: "zero_field".into(),
            },
            description: None,
        };
        let mut fields = test_fields();
        fields.insert("zero_field".into(), serde_json::json!(0));
        assert!(evaluate_derivation(&rule, &fields).is_none());
    }

    #[test]
    fn test_alias() {
        let rule = ReasoningRule::Derivation {
            id: TypeId("alias_model_id".into()),
            entity_type: TypeId("LLM".into()),
            source_fields: vec!["model_id".into()],
            derived_field: "name".into(),
            expression: DerivationExpr::Alias {
                source: "model_id".into(),
            },
            description: None,
        };
        let mut fields = HashMap::new();
        fields.insert("model_id".into(), serde_json::json!("deepseek-chat"));
        let result = evaluate_derivation(&rule, &fields).unwrap();
        assert_eq!(result.value, "deepseek-chat");
    }

    #[test]
    fn test_evaluate_all_derivations() {
        let rules = vec![
            ReasoningRule::Derivation {
                id: TypeId("r1".into()),
                entity_type: TypeId("LLM".into()),
                source_fields: vec!["supports_vision".into()],
                derived_field: "has_image_input".into(),
                expression: DerivationExpr::FieldIsTrue {
                    field: "supports_vision".into(),
                    result: serde_json::json!(true),
                },
                description: None,
            },
            ReasoningRule::Derivation {
                id: TypeId("r2".into()),
                entity_type: TypeId("LLM".into()),
                source_fields: vec!["supports_reasoning".into()],
                derived_field: "has_thinking".into(),
                expression: DerivationExpr::FieldIsTrue {
                    field: "supports_reasoning".into(),
                    result: serde_json::json!(true),
                },
                description: None,
            },
        ];
        let results = evaluate_all_derivations(&rules, &test_fields());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].derived_field, "has_image_input");
    }

    #[test]
    fn test_traversal_returns_target_type() {
        let ontology = crate::builtin::full_ontology();
        let rule = ReasoningRule::Traversal {
            id: TypeId("find_provider_models".into()),
            link_id: TypeId("Provider_provides_LLM".into()),
            source_type: TypeId("Provider".into()),
            target_type: TypeId("LLM".into()),
            description: None,
        };
        let result = evaluate_traversal(&ontology, &rule).unwrap();
        assert_eq!(result.source_id, TypeId("Provider".into()));
        assert_eq!(result.target_ids, vec![TypeId("LLM".into())]);
    }

    #[test]
    fn test_traversal_wrong_rule_type_returns_none() {
        let ontology = crate::builtin::full_ontology();
        let derivation = ReasoningRule::Derivation {
            id: TypeId("x".into()),
            entity_type: TypeId("LLM".into()),
            source_fields: vec![],
            derived_field: "y".into(),
            expression: DerivationExpr::Alias {
                source: "z".into(),
            },
            description: None,
        };
        assert!(evaluate_traversal(&ontology, &derivation).is_none());
    }
}
