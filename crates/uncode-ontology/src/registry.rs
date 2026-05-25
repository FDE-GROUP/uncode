//! Type registry — the central store for all ontology definitions.

use std::collections::HashMap;

use crate::types::{ActionDef, EntityCategory, EntityDef, LinkDef, TypeId};

/// Central type registry holding all entity, action, and link definitions.
#[derive(Debug, Clone)]
pub struct TypeRegistry {
    pub entities: HashMap<TypeId, EntityDef>,
    pub actions: HashMap<String, ActionDef>,
    pub links: HashMap<TypeId, LinkDef>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
            actions: HashMap::new(),
            links: HashMap::new(),
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

    /// Merge another registry into this one. Conflicting keys are overwritten.
    pub fn merge(mut self, other: TypeRegistry) -> Self {
        for (id, entity) in other.entities {
            self.entities.insert(id, entity);
        }
        for (name, action) in other.actions {
            self.actions.insert(name, action);
        }
        for (id, link) in other.links {
            self.links.insert(id, link);
        }
        self
    }

    pub fn get_entity(&self, id: &TypeId) -> Option<&EntityDef> {
        self.entities.get(id)
    }

    pub fn get_action(&self, name: &str) -> Option<&ActionDef> {
        self.actions.get(name)
    }

    pub fn get_link(&self, id: &TypeId) -> Option<&LinkDef> {
        self.links.get(id)
    }

    /// Query links where source_type matches the given TypeId.
    pub fn links_from(&self, source_type: &TypeId) -> Vec<&LinkDef> {
        self.links
            .values()
            .filter(|l| l.source_type == *source_type)
            .collect()
    }

    /// Query links where target_type matches the given TypeId.
    pub fn links_to(&self, target_type: &TypeId) -> Vec<&LinkDef> {
        self.links
            .values()
            .filter(|l| l.target_type == *target_type)
            .collect()
    }

    /// Get the inverse link of the given link id, if one exists.
    pub fn inverse_link(&self, id: &TypeId) -> Option<&LinkDef> {
        self.links.get(id).and_then(|link| {
            link.inverse
                .as_ref()
                .and_then(|inv_id| self.links.get(inv_id))
        })
    }

    /// Collect field aliases for actions matching the given category: alias → canonical name.
    pub fn field_aliases_by_category(
        &self,
        category: EntityCategory,
    ) -> HashMap<String, String> {
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

    /// Query entities by category
    pub fn entities_by_category(&self, category: EntityCategory) -> Vec<&EntityDef> {
        self.entities
            .values()
            .filter(|e| e.category == category)
            .collect()
    }

    /// Query actions by category
    pub fn actions_by_category(&self, category: EntityCategory) -> Vec<&ActionDef> {
        self.actions
            .values()
            .filter(|a| a.category == category)
            .collect()
    }

    /// Collect default values for actions matching the given category.
    pub fn defaults_by_category(
        &self,
        category: EntityCategory,
    ) -> HashMap<String, HashMap<String, serde_json::Value>> {
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
    pub fn all_defaults(&self) -> HashMap<String, HashMap<String, serde_json::Value>> {
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
