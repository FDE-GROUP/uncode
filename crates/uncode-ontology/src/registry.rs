//! Type registry — the central store for all ontology definitions.

use std::collections::HashMap;

use crate::types::{ActionDef, EntityDef, TypeId};

/// Central type registry holding all entity and action definitions.
#[derive(Debug, Clone)]
pub struct TypeRegistry {
    pub entities: HashMap<TypeId, EntityDef>,
    pub actions: HashMap<String, ActionDef>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
            actions: HashMap::new(),
        }
    }

    pub fn register_entity(&mut self, def: EntityDef) {
        self.entities.insert(def.id.clone(), def);
    }

    pub fn register_action(&mut self, def: ActionDef) {
        self.actions.insert(def.name.clone(), def);
    }

    pub fn get_entity(&self, id: &TypeId) -> Option<&EntityDef> {
        self.entities.get(id)
    }

    pub fn get_action(&self, name: &str) -> Option<&ActionDef> {
        self.actions.get(name)
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
