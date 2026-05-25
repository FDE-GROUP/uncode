//! Type registry — the central store for all ontology definitions.

use std::collections::HashMap;

use crate::types::{
    ActionDef, EntityCategory, EntityDef, LinkDef, OntologyVersion, ReasoningRule, TypeId,
};

/// Action defaults map: action_name → field_name → default value.
pub type FieldDefaults = HashMap<String, HashMap<String, serde_json::Value>>;

/// Central type registry holding all entity, action, link, and reasoning definitions.
#[derive(Debug, Clone)]
pub struct TypeRegistry {
    pub version: OntologyVersion,
    pub entities: HashMap<TypeId, EntityDef>,
    pub actions: HashMap<String, ActionDef>,
    pub links: HashMap<TypeId, LinkDef>,
    pub reasoning_rules: HashMap<TypeId, ReasoningRule>,
}

impl TypeRegistry {
    /// Create a new empty registry with default version (0.1.0).
    ///
    /// # Examples
    ///
    /// ```
    /// use uncode_ontology::TypeRegistry;
    ///
    /// let reg = TypeRegistry::new();
    /// assert!(reg.get_action("does_not_exist").is_none());
    /// ```
    pub fn new() -> Self {
        Self {
            version: OntologyVersion::new(0, 1, 0),
            entities: HashMap::new(),
            actions: HashMap::new(),
            links: HashMap::new(),
            reasoning_rules: HashMap::new(),
        }
    }

    /// Create with explicit version.
    pub fn with_version(version: OntologyVersion) -> Self {
        Self {
            version,
            entities: HashMap::new(),
            actions: HashMap::new(),
            links: HashMap::new(),
            reasoning_rules: HashMap::new(),
        }
    }

    pub fn register_entity(&mut self, def: EntityDef) {
        self.entities.insert(def.id.clone(), def);
    }

    pub fn register_action(&mut self, def: ActionDef) {
        self.actions.insert(def.name.clone(), def);
    }

    pub fn register_link(&mut self, def: LinkDef) {
        self.links.insert(def.id.clone(), def);
    }

    pub fn register_reasoning_rule(&mut self, rule: ReasoningRule) {
        let id = match &rule {
            ReasoningRule::Traversal { id, .. } | ReasoningRule::Derivation { id, .. } => {
                id.clone()
            }
        };
        self.reasoning_rules.insert(id, rule);
    }

    /// Merge another registry into this one. Conflicting keys are overwritten.
    /// Takes the higher version.
    ///
    /// # Examples
    ///
    /// ```
    /// use uncode_ontology::{ActionDef, EntityCategory, ExecutionCategory, TypeId, TypeRegistry};
    ///
    /// let mut r1 = TypeRegistry::new();
    /// r1.register_action(ActionDef {
    ///     name: "read".into(),
    ///     output_type: TypeId::from("string"),
    ///     description: None,
    ///     category: EntityCategory::Domain,
    ///     execution_category: ExecutionCategory::ReadOnly,
    ///     fields: vec![],
    ///     preconditions: vec![],
    ///     effects: vec![],
    ///     path_fields: vec![],
    /// });
    ///
    /// let mut r2 = TypeRegistry::new();
    /// r2.register_action(ActionDef {
    ///     name: "write".into(),
    ///     output_type: TypeId::from("string"),
    ///     description: None,
    ///     category: EntityCategory::Domain,
    ///     execution_category: ExecutionCategory::ReadOnly,
    ///     fields: vec![],
    ///     preconditions: vec![],
    ///     effects: vec![],
    ///     path_fields: vec![],
    /// });
    ///
    /// let merged = r1.merge(&r2);
    /// assert!(merged.get_action("read").is_some());
    /// assert!(merged.get_action("write").is_some());
    /// ```
    #[must_use]
    pub fn merge(mut self, other: &TypeRegistry) -> Self {
        if other.version > self.version {
            self.version = other.version;
        }
        for (id, entity) in &other.entities {
            self.entities.insert(id.clone(), entity.clone());
        }
        for (name, action) in &other.actions {
            self.actions.insert(name.clone(), action.clone());
        }
        for (id, link) in &other.links {
            self.links.insert(id.clone(), link.clone());
        }
        for (id, rule) in &other.reasoning_rules {
            self.reasoning_rules.insert(id.clone(), rule.clone());
        }
        self
    }

    #[must_use]
    pub fn get_entity(&self, id: &TypeId) -> Option<&EntityDef> {
        self.entities.get(id)
    }

    #[must_use]
    pub fn get_action(&self, name: &str) -> Option<&ActionDef> {
        self.actions.get(name)
    }

    #[must_use]
    pub fn get_link(&self, id: &TypeId) -> Option<&LinkDef> {
        self.links.get(id)
    }

    #[must_use]
    pub fn get_reasoning_rule(&self, id: &TypeId) -> Option<&ReasoningRule> {
        self.reasoning_rules.get(id)
    }

    /// Query reasoning rules that apply to a given entity type.
    #[must_use]
    pub fn reasoning_rules_for_entity(&self, entity_type: &TypeId) -> Vec<&ReasoningRule> {
        self.reasoning_rules
            .values()
            .filter(|rule| match rule {
                ReasoningRule::Traversal { source_type, .. } => source_type == entity_type,
                ReasoningRule::Derivation {
                    entity_type: et, ..
                } => et == entity_type,
            })
            .collect()
    }

    /// Check if this registry's version is compatible with a required version.
    #[must_use]
    pub fn is_version_compatible(&self, required: &OntologyVersion) -> bool {
        self.version.is_compatible_with(required)
    }

    /// Query links where source_type matches the given TypeId.
    #[must_use]
    pub fn links_from(&self, source_type: &TypeId) -> Vec<&LinkDef> {
        self.links
            .values()
            .filter(|l| l.source_type == *source_type)
            .collect()
    }

    /// Query links where target_type matches the given TypeId.
    #[must_use]
    pub fn links_to(&self, target_type: &TypeId) -> Vec<&LinkDef> {
        self.links
            .values()
            .filter(|l| l.target_type == *target_type)
            .collect()
    }

    /// Get the inverse link of the given link id, if one exists.
    #[must_use]
    pub fn inverse_link(&self, id: &TypeId) -> Option<&LinkDef> {
        self.links.get(id).and_then(|link| {
            link.inverse
                .as_ref()
                .and_then(|inv_id| self.links.get(inv_id))
        })
    }

    /// Collect field aliases for actions matching the given category: alias → canonical name.
    #[must_use]
    pub fn field_aliases_by_category(&self, category: EntityCategory) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for action in self.actions.values().filter(|a| a.category == category) {
            for field in &action.fields {
                for alias in &field.aliases {
                    map.insert(alias.clone(), field.name.clone());
                }
            }
        }
        map
    }

    /// Collect field aliases from entities matching the given category: alias → canonical name.
    #[must_use]
    pub fn entity_field_aliases_by_category(
        &self,
        category: EntityCategory,
    ) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for entity in self.entities.values().filter(|e| e.category == category) {
            for field in &entity.fields {
                for alias in &field.aliases {
                    map.insert(alias.clone(), field.name.clone());
                }
            }
        }
        map
    }

    /// Collect all field aliases across all actions: alias → canonical name.
    #[must_use]
    pub fn all_field_aliases(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for action in self.actions.values() {
            for field in &action.fields {
                for alias in &field.aliases {
                    map.insert(alias.clone(), field.name.clone());
                }
            }
        }
        map
    }

    /// Resolve entity with inheritance chain merged.
    ///
    /// If `extends` is set, recursively resolves the parent entity,
    /// prepending parent fields and invariants to the child's own.
    /// Returns a merged EntityDef (not registered — just computed).
    pub fn resolve_entity(&self, type_id: &TypeId) -> Option<EntityDef> {
        let entity = self.get_entity(type_id)?;
        let mut merged = entity.clone();

        if let Some(ref parent_id) = entity.extends {
            if let Some(parent) = self.resolve_entity(parent_id) {
                // Parent fields come first, child fields override
                let mut all_fields = parent.fields;
                for child_field in &merged.fields {
                    all_fields.retain(|f| f.name != child_field.name);
                    all_fields.push(child_field.clone());
                }
                merged.fields = all_fields;

                // Parent invariants come first
                let mut all_invariants = parent.invariants;
                all_invariants.extend(merged.invariants.clone());
                merged.invariants = all_invariants;
            }
        }

        Some(merged)
    }

    /// Query entities by category
    #[must_use]
    pub fn entities_by_category(&self, category: EntityCategory) -> Vec<&EntityDef> {
        self.entities
            .values()
            .filter(|e| e.category == category)
            .collect()
    }

    /// Query actions by category
    #[must_use]
    pub fn actions_by_category(&self, category: EntityCategory) -> Vec<&ActionDef> {
        self.actions
            .values()
            .filter(|a| a.category == category)
            .collect()
    }

    /// Collect default values for actions matching the given category.
    #[must_use]
    pub fn defaults_by_category(&self, category: EntityCategory) -> FieldDefaults {
        let mut map = HashMap::new();
        for action in self.actions.values().filter(|a| a.category == category) {
            for field in &action.fields {
                if let Some(default) = &field.default {
                    map.entry(action.name.clone())
                        .or_insert_with(HashMap::new)
                        .insert(field.name.clone(), default.clone());
                }
            }
        }
        map
    }

    /// Collect all default values: action_name → field_name → default.
    #[must_use]
    pub fn all_defaults(&self) -> FieldDefaults {
        let mut map = HashMap::new();
        for action in self.actions.values() {
            for field in &action.fields {
                if let Some(default) = &field.default {
                    map.entry(action.name.clone())
                        .or_insert_with(HashMap::new)
                        .insert(field.name.clone(), default.clone());
                }
            }
        }
        map
    }
}

impl Default for TypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}
