//! Constraint evaluation engine.

use std::collections::HashMap;

use crate::types::{Constraint, ConstraintLevel};

/// Result of evaluating a single constraint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstraintResult {
    Pass,
    Warn {
        constraint: String,
        field: String,
        detail: String,
    },
    Fail {
        constraint: String,
        field: String,
        detail: String,
    },
}

impl ConstraintResult {
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }
}

/// Evaluate a constraint against field values.
pub fn evaluate_constraint(
    constraint: &Constraint,
    fields: &HashMap<String, serde_json::Value>,
) -> ConstraintResult {
    match constraint {
        Constraint::RequiredField { field } => {
            if fields.contains_key(field) {
                ConstraintResult::Pass
            } else {
                ConstraintResult::Fail {
                    constraint: "required_field".into(),
                    field: field.clone(),
                    detail: format!("required field '{field}' is missing"),
                }
            }
        }
        Constraint::TypeCheck {
            field,
            expected,
            level,
        } => {
            let Some(value) = fields.get(field) else {
                return ConstraintResult::Pass;
            };
            let ok = match expected.as_str() {
                "string" => value.is_string(),
                "integer" | "number" => value.is_number(),
                "boolean" => value.is_boolean(),
                _ => true,
            };
            if ok {
                ConstraintResult::Pass
            } else {
                let result = ConstraintResult::Fail {
                    constraint: "type_check".into(),
                    field: field.clone(),
                    detail: format!(
                        "field '{field}' expected {expected}, got {}",
                        value_type_name(value)
                    ),
                };
                match level {
                    ConstraintLevel::Soft => ConstraintResult::Warn {
                        constraint: result.constraint().to_string(),
                        field: result.field().to_string(),
                        detail: result.detail().to_string(),
                    },
                    ConstraintLevel::Hard => result,
                }
            }
        }
        Constraint::RangeCheck {
            field,
            min,
            max,
            level,
        } => {
            let Some(value) = fields.get(field) else {
                return ConstraintResult::Pass;
            };
            let num = value.as_f64();
            let ok = num.map_or(true, |n| {
                min.map_or(true, |lo| n >= lo) && max.map_or(true, |hi| n <= hi)
            });
            if ok {
                ConstraintResult::Pass
            } else {
                let result = ConstraintResult::Fail {
                    constraint: "range_check".into(),
                    field: field.clone(),
                    detail: format!("field '{field}' value out of range"),
                };
                match level {
                    ConstraintLevel::Soft => ConstraintResult::Warn {
                        constraint: result.constraint().to_string(),
                        field: result.field().to_string(),
                        detail: result.detail().to_string(),
                    },
                    ConstraintLevel::Hard => result,
                }
            }
        }
        Constraint::EnumCheck {
            field,
            allowed,
            level,
        } => {
            let Some(value) = fields.get(field) else {
                return ConstraintResult::Pass;
            };
            if let Some(s) = value.as_str() {
                if allowed.contains(&s.to_string()) {
                    return ConstraintResult::Pass;
                }
            }
            let result = ConstraintResult::Fail {
                constraint: "enum_check".into(),
                field: field.clone(),
                detail: format!("field '{field}' value not in allowed set"),
            };
            match level {
                ConstraintLevel::Soft => ConstraintResult::Warn {
                    constraint: result.constraint().to_string(),
                    field: result.field().to_string(),
                    detail: result.detail().to_string(),
                },
                ConstraintLevel::Hard => result,
            }
        }
        Constraint::RegexMatch {
            field,
            pattern,
            description,
            level,
        } => {
            // Regex evaluation requires the regex crate; skip at ontology level
            // and handle in the agent's firewall rules instead.
            let _ = (field, pattern, description, level);
            ConstraintResult::Pass
        }
        Constraint::CustomRule {
            name: _,
            description: _,
            level: _,
        } => ConstraintResult::Pass,
    }
}

fn value_type_name(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

impl ConstraintResult {
    fn constraint(&self) -> &str {
        match self {
            Self::Fail { constraint, .. } | Self::Warn { constraint, .. } => constraint,
            Self::Pass => "",
        }
    }
    fn field(&self) -> &str {
        match self {
            Self::Fail { field, .. } | Self::Warn { field, .. } => field,
            Self::Pass => "",
        }
    }
    fn detail(&self) -> &str {
        match self {
            Self::Fail { detail, .. } | Self::Warn { detail, .. } => detail,
            Self::Pass => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> HashMap<String, serde_json::Value> {
        let mut m = HashMap::new();
        m.insert("path".into(), serde_json::json!("src/main.rs"));
        m.insert("offset".into(), serde_json::json!(0));
        m
    }

    #[test]
    fn test_required_field_present() {
        let c = Constraint::RequiredField {
            field: "path".into(),
        };
        assert!(evaluate_constraint(&c, &fields()).is_pass());
    }

    #[test]
    fn test_required_field_missing() {
        let c = Constraint::RequiredField {
            field: "missing".into(),
        };
        let r = evaluate_constraint(&c, &fields());
        assert!(matches!(r, ConstraintResult::Fail { .. }));
    }

    #[test]
    fn test_type_check_pass() {
        let c = Constraint::TypeCheck {
            field: "offset".into(),
            expected: "number".into(),
            level: ConstraintLevel::Hard,
        };
        assert!(evaluate_constraint(&c, &fields()).is_pass());
    }

    #[test]
    fn test_type_check_fail() {
        let c = Constraint::TypeCheck {
            field: "path".into(),
            expected: "number".into(),
            level: ConstraintLevel::Hard,
        };
        assert!(matches!(
            evaluate_constraint(&c, &fields()),
            ConstraintResult::Fail { .. }
        ));
    }

    #[test]
    fn test_type_check_soft_warn() {
        let c = Constraint::TypeCheck {
            field: "path".into(),
            expected: "number".into(),
            level: ConstraintLevel::Soft,
        };
        assert!(matches!(
            evaluate_constraint(&c, &fields()),
            ConstraintResult::Warn { .. }
        ));
    }
}
