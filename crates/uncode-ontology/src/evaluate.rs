//! Constraint evaluation engine.

use crate::types::{Constraint, ConstraintLevel};

/// Result of evaluating a single constraint.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
#[non_exhaustive]
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
    #[must_use]
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }

    fn severity(&self) -> u8 {
        match self {
            Self::Pass => 0,
            Self::Warn { .. } => 1,
            Self::Fail { .. } => 2,
        }
    }
}

impl std::ops::BitOr for ConstraintResult {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        if rhs.severity() > self.severity() {
            rhs
        } else {
            self
        }
    }
}

impl std::ops::BitOrAssign for ConstraintResult {
    fn bitor_assign(&mut self, rhs: Self) {
        if rhs.severity() > self.severity() {
            *self = rhs;
        }
    }
}

/// Field lookup trait — abstracts over `HashMap` and `serde_json::Map`.
pub trait FieldLookup {
    fn get_field(&self, key: &str) -> Option<&serde_json::Value>;
    fn contains_field(&self, key: &str) -> bool;
}

impl FieldLookup for std::collections::HashMap<String, serde_json::Value> {
    fn get_field(&self, key: &str) -> Option<&serde_json::Value> {
        self.get(key)
    }
    fn contains_field(&self, key: &str) -> bool {
        self.contains_key(key)
    }
}

impl FieldLookup for serde_json::Map<String, serde_json::Value> {
    fn get_field(&self, key: &str) -> Option<&serde_json::Value> {
        self.get(key)
    }
    fn contains_field(&self, key: &str) -> bool {
        self.contains_key(key)
    }
}

/// Evaluate a constraint against field values.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use uncode_ontology::{Constraint, ConstraintLevel, evaluate_constraint};
///
/// let mut fields = HashMap::new();
/// fields.insert("name".into(), serde_json::json!("hello"));
///
/// let constraint = Constraint::RequiredField { field: "name".into() };
/// let result = evaluate_constraint(&constraint, &fields);
/// assert!(result.is_pass());
/// ```
///
/// Missing required field produces a failure:
///
/// ```
/// use std::collections::HashMap;
/// use uncode_ontology::{Constraint, evaluate_constraint};
///
/// let fields: HashMap<String, serde_json::Value> = HashMap::new();
/// let constraint = Constraint::RequiredField { field: "missing".into() };
/// let result = evaluate_constraint(&constraint, &fields);
/// assert!(!result.is_pass());
/// ```
pub fn evaluate_constraint<F: FieldLookup + ?Sized>(
    constraint: &Constraint,
    fields: &F,
) -> ConstraintResult {
    match constraint {
        Constraint::RequiredField { field } => {
            if fields.contains_field(field) {
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
            let Some(value) = fields.get_field(field) else {
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
                let detail = format!(
                    "field '{field}' expected {expected}, got {}",
                    value_type_name(value)
                );
                match level {
                    ConstraintLevel::Soft => ConstraintResult::Warn {
                        constraint: "type_check".into(),
                        field: field.clone(),
                        detail,
                    },
                    ConstraintLevel::Hard => ConstraintResult::Fail {
                        constraint: "type_check".into(),
                        field: field.clone(),
                        detail,
                    },
                }
            }
        }
        Constraint::RangeCheck {
            field,
            min,
            max,
            level,
        } => {
            let Some(value) = fields.get_field(field) else {
                return ConstraintResult::Pass;
            };
            let num = value.as_f64();
            let ok = num.map_or(true, |n| {
                let in_min = min.is_none_or(|lo| (lo..).contains(&n));
                let in_max = max.is_none_or(|hi| (..=hi).contains(&n));
                in_min && in_max
            });
            if ok {
                ConstraintResult::Pass
            } else {
                let detail = format!("field '{field}' value out of range");
                match level {
                    ConstraintLevel::Soft => ConstraintResult::Warn {
                        constraint: "range_check".into(),
                        field: field.clone(),
                        detail,
                    },
                    ConstraintLevel::Hard => ConstraintResult::Fail {
                        constraint: "range_check".into(),
                        field: field.clone(),
                        detail,
                    },
                }
            }
        }
        Constraint::EnumCheck {
            field,
            allowed,
            level,
        } => {
            let Some(value) = fields.get_field(field) else {
                return ConstraintResult::Pass;
            };
            if let Some(s) = value.as_str() {
                if allowed.iter().any(|a| a == s) {
                    return ConstraintResult::Pass;
                }
            }
            let detail = format!("field '{field}' value not in allowed set");
            match level {
                ConstraintLevel::Soft => ConstraintResult::Warn {
                    constraint: "enum_check".into(),
                    field: field.clone(),
                    detail,
                },
                ConstraintLevel::Hard => ConstraintResult::Fail {
                    constraint: "enum_check".into(),
                    field: field.clone(),
                    detail,
                },
            }
        }
        Constraint::Referential {
            field,
            target_type: _,
            description,
            level: _,
        } => match fields.get_field(field) {
            Some(serde_json::Value::String(s)) if !s.is_empty() => ConstraintResult::Pass,
            _ => ConstraintResult::Fail {
                constraint: "referential".into(),
                field: field.clone(),
                detail: description.clone(),
            },
        },
        Constraint::RegexMatch {
            field,
            pattern,
            description,
            level,
        } => {
            let Some(value) = fields.get_field(field) else {
                return ConstraintResult::Pass;
            };
            let matched = value
                .as_str()
                .map_or(false, |s| s.contains(pattern.as_str()));
            if matched {
                ConstraintResult::Pass
            } else {
                let detail = format!(
                    "field '{field}' does not match pattern '{pattern}': {description}"
                );
                match level {
                    ConstraintLevel::Soft => ConstraintResult::Warn {
                        constraint: "regex_match".into(),
                        field: field.clone(),
                        detail,
                    },
                    ConstraintLevel::Hard => ConstraintResult::Fail {
                        constraint: "regex_match".into(),
                        field: field.clone(),
                        detail,
                    },
                }
            }
        }
        Constraint::CustomRule {
            name,
            description,
            level,
        } => {
            // Custom rule evaluation requires a callback mechanism that will be added later.
            // Currently passes unconditionally; future versions will invoke registered
            // custom validators based on the rule name.
            let _ = (name, description, level);
            ConstraintResult::Pass
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

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
