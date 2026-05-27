//! Instance registry — runtime entity instances for traversal reasoning.
//!
//! Where `TypeRegistry` answers "what types exist?", `InstanceRegistry` answers
//! "what concrete entities exist at runtime?" This enables instance-level traversal
//! (e.g., "which File instances are in this Workspace?") as opposed to type-level
//! traversal ("does a Workspace→File link exist?").

use std::collections::HashMap;

use crate::TypeRegistry;
use crate::types::TypeId;

/// A runtime instance of an ontology entity type.
///
/// Maps `EntityDef.fields` to actual values. The `id` is unique within its `type_id`.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use uncode_ontology::{EntityInstance, TypeId};
///
/// let file = EntityInstance {
///     type_id: TypeId::from("File"),
///     id: "src/main.rs".into(),
///     fields: {
///         let mut m = HashMap::new();
///         m.insert("path".into(), serde_json::json!("src/main.rs"));
///         m.insert("exists".into(), serde_json::json!(true));
///         m
///     },
/// };
/// assert_eq!(file.id, "src/main.rs");
/// ```
#[derive(Debug, Clone)]
pub struct EntityInstance {
    pub type_id: TypeId,
    pub id: String,
    pub fields: HashMap<String, serde_json::Value>,
}

impl EntityInstance {
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&serde_json::Value> {
        self.fields.get(name)
    }
}

/// A registry holding all runtime entity instances, indexed by `(type_id, id)`.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use uncode_ontology::{EntityInstance, InstanceRegistry, TypeId};
///
/// let mut reg = InstanceRegistry::new();
/// reg.insert(EntityInstance {
///     type_id: TypeId::from("File"),
///     id: "src/main.rs".into(),
///     fields: HashMap::new(),
/// });
/// assert!(reg.get(&TypeId::from("File"), "src/main.rs").is_some());
/// ```
#[derive(Debug, Clone, Default)]
pub struct InstanceRegistry {
    instances: HashMap<(TypeId, String), EntityInstance>,
    by_type: HashMap<TypeId, Vec<String>>,
}

impl InstanceRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            instances: HashMap::new(),
            by_type: HashMap::new(),
        }
    }

    /// Insert or overwrite an instance. Idempotent — no duplicate in `by_type`.
    pub fn insert(&mut self, instance: EntityInstance) {
        let type_id = instance.type_id.clone();
        let id = instance.id.clone();
        let type_entry = self.by_type.entry(type_id.clone()).or_default();
        if !type_entry.contains(&id) {
            type_entry.push(id.clone());
        }
        self.instances.insert((type_id, id), instance);
    }

    /// Remove and return an instance by `(type_id, id)`.
    pub fn remove(&mut self, type_id: &TypeId, id: &str) -> Option<EntityInstance> {
        let key = (type_id.clone(), id.to_string());
        let removed = self.instances.remove(&key)?;
        if let Some(ids) = self.by_type.get_mut(type_id) {
            ids.retain(|i| i != id);
            if ids.is_empty() {
                self.by_type.remove(type_id);
            }
        }
        Some(removed)
    }

    #[must_use]
    pub fn get(&self, type_id: &TypeId, id: &str) -> Option<&EntityInstance> {
        let key = (type_id.clone(), id.to_string());
        self.instances.get(&key)
    }

    #[must_use]
    pub fn contains(&self, type_id: &TypeId, id: &str) -> bool {
        let key = (type_id.clone(), id.to_string());
        self.instances.contains_key(&key)
    }

    /// Return all instances of a given type.
    #[must_use]
    pub fn list_by_type(&self, type_id: &TypeId) -> Vec<&EntityInstance> {
        self.by_type.get(type_id).map_or(Vec::new(), |ids| {
            ids.iter()
                .filter_map(|id| self.instances.get(&(type_id.clone(), id.clone())))
                .collect()
        })
    }

    /// Return all instances of a given type that match the predicate.
    #[must_use]
    pub fn filter(
        &self,
        type_id: &TypeId,
        mut predicate: impl FnMut(&&EntityInstance) -> bool,
    ) -> Vec<&EntityInstance> {
        self.list_by_type(type_id)
            .into_iter()
            .filter(|inst| predicate(inst))
            .collect()
    }

    /// Number of instances across all types.
    #[must_use]
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Type-level traversal: follow a `LinkDef` via default field-match strategy.
    ///
    /// Assumes that target-type instances have a field named after the source type
    /// (lowercased) whose value equals `source_id`.
    ///
    /// This works for `Provider_provides_LLM` (LLM has a `provider` field) but
    /// **not** for `Workspace_contains_File` (File has no `workspace` field).
    /// Custom-resolution links should use [`filter()`][Self::filter] directly.
    #[must_use]
    pub fn traverse_typed(
        &self,
        type_registry: &TypeRegistry,
        link_id: &TypeId,
        source_id: &str,
    ) -> Vec<&EntityInstance> {
        let Some(link) = type_registry.get_link(link_id) else {
            return Vec::new();
        };
        let source_field = link.source_type.to_lowercase();
        self.filter(&link.target_type, |inst| {
            inst.fields
                .get(&source_field)
                .and_then(|v| v.as_str())
                .is_some_and(|v| v == source_id)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_file(path: &str) -> EntityInstance {
        let mut fields = HashMap::new();
        fields.insert("path".into(), serde_json::json!(path));
        fields.insert("exists".into(), serde_json::json!(true));
        EntityInstance {
            type_id: TypeId::from("File"),
            id: path.into(),
            fields,
        }
    }

    fn make_llm(model_id: &str, provider: &str) -> EntityInstance {
        let mut fields = HashMap::new();
        fields.insert("model_id".into(), serde_json::json!(model_id));
        fields.insert("provider".into(), serde_json::json!(provider));
        EntityInstance {
            type_id: TypeId::from("LLM"),
            id: model_id.into(),
            fields,
        }
    }

    // ── Insert / Get / Remove ──

    #[test]
    fn test_insert_and_get() {
        let mut reg = InstanceRegistry::new();
        reg.insert(make_file("src/main.rs"));
        let inst = reg.get(&TypeId::from("File"), "src/main.rs").unwrap();
        assert_eq!(inst.type_id, TypeId::from("File"));
        assert_eq!(inst.field("path").unwrap().as_str().unwrap(), "src/main.rs");
    }

    #[test]
    fn test_get_nonexistent() {
        let reg = InstanceRegistry::new();
        assert!(reg.get(&TypeId::from("File"), "nope").is_none());
    }

    #[test]
    fn test_insert_overwrites() {
        let mut reg = InstanceRegistry::new();
        reg.insert(make_file("src/lib.rs"));

        let mut updated = make_file("src/lib.rs");
        updated
            .fields
            .insert("exists".into(), serde_json::json!(false));
        reg.insert(updated);

        let inst = reg.get(&TypeId::from("File"), "src/lib.rs").unwrap();
        assert_eq!(inst.field("exists").unwrap(), &serde_json::json!(false));
        // by_type should not have duplicates
        assert_eq!(reg.list_by_type(&TypeId::from("File")).len(), 1);
    }

    #[test]
    fn test_remove() {
        let mut reg = InstanceRegistry::new();
        reg.insert(make_file("src/main.rs"));
        reg.insert(make_file("src/lib.rs"));

        let removed = reg.remove(&TypeId::from("File"), "src/main.rs").unwrap();
        assert_eq!(removed.id, "src/main.rs");

        assert!(reg.get(&TypeId::from("File"), "src/main.rs").is_none());
        assert!(reg.get(&TypeId::from("File"), "src/lib.rs").is_some());
        assert_eq!(reg.list_by_type(&TypeId::from("File")).len(), 1);
    }

    #[test]
    fn test_remove_last_cleans_by_type() {
        let mut reg = InstanceRegistry::new();
        reg.insert(make_file("only.rs"));
        reg.remove(&TypeId::from("File"), "only.rs");
        assert_eq!(reg.list_by_type(&TypeId::from("File")).len(), 0);
    }

    // ── List / Filter ──

    #[test]
    fn test_list_by_type() {
        let mut reg = InstanceRegistry::new();
        reg.insert(make_file("src/main.rs"));
        reg.insert(make_llm("deepseek-v3", "deepseek"));

        let files = reg.list_by_type(&TypeId::from("File"));
        assert_eq!(files.len(), 1);

        let llms = reg.list_by_type(&TypeId::from("LLM"));
        assert_eq!(llms.len(), 1);
    }

    #[test]
    fn test_filter() {
        let mut reg = InstanceRegistry::new();
        reg.insert(make_file("src/main.rs"));
        reg.insert(make_file("tests/foo.rs"));
        reg.insert(make_file("Cargo.toml"));

        let src_files = reg.filter(&TypeId::from("File"), |inst| inst.id.starts_with("src/"));
        assert_eq!(src_files.len(), 1);
        assert_eq!(src_files[0].id, "src/main.rs");
    }

    #[test]
    fn test_len_and_is_empty() {
        let mut reg = InstanceRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);

        reg.insert(make_file("a.rs"));
        assert!(!reg.is_empty());
        assert_eq!(reg.len(), 1);

        reg.insert(make_file("b.rs"));
        assert_eq!(reg.len(), 2);

        reg.remove(&TypeId::from("File"), "a.rs");
        assert_eq!(reg.len(), 1);
    }

    // ── Traverse ──

    #[test]
    fn test_traverse_typed_provider_to_llm() {
        let mut reg = InstanceRegistry::new();
        reg.insert(make_llm("deepseek-v3", "deepseek"));
        reg.insert(make_llm("deepseek-r1", "deepseek"));
        reg.insert(make_llm("gpt-4o", "openai"));

        let ontology = crate::builtin::full_ontology();

        let results = reg.traverse_typed(
            &ontology,
            &TypeId::from("Provider_provides_LLM"),
            "deepseek",
        );
        assert_eq!(results.len(), 2);
        let ids: Vec<&str> = results.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"deepseek-v3"));
        assert!(ids.contains(&"deepseek-r1"));
    }

    #[test]
    fn test_traverse_typed_no_match() {
        let mut reg = InstanceRegistry::new();
        reg.insert(make_llm("gpt-4o", "openai"));

        let ontology = crate::builtin::full_ontology();

        let results = reg.traverse_typed(
            &ontology,
            &TypeId::from("Provider_provides_LLM"),
            "deepseek",
        );
        assert!(results.is_empty());
    }

    #[test]
    fn test_traverse_typed_nonexistent_link() {
        let reg = InstanceRegistry::new();
        let ontology = crate::builtin::full_ontology();
        let results = reg.traverse_typed(&ontology, &TypeId::from("nonexistent_link"), "anything");
        assert!(results.is_empty());
    }
}
