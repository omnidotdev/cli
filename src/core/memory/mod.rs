//! Persistent memory system for storing facts across sessions.
//!
//! Memory items are stored per-project and can be:
//! - User preferences (how they like code formatted, etc.)
//! - Project facts (architecture decisions, patterns used)
//! - Learned corrections (things the agent got wrong and was corrected on)

use agent_core::memory;
pub use agent_core::memory::{MemoryCategory, MemoryItem};
use chrono::Utc;

use super::project::Project;
use super::storage::Storage;

/// Memory store for a project
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct MemoryStore {
    /// All memory items
    pub items: Vec<MemoryItem>,
}

/// Memory manager for persistent facts
pub struct MemoryManager {
    storage: Storage,
    project: Project,
}

impl MemoryManager {
    /// Create a new memory manager
    #[must_use]
    pub const fn new(storage: Storage, project: Project) -> Self {
        Self { storage, project }
    }

    /// Create a memory manager for the current project
    ///
    /// # Errors
    ///
    /// Returns error if project detection or storage initialization fails.
    pub fn for_current_project() -> anyhow::Result<Self> {
        let storage = Storage::new()?;
        let project = Project::detect(&std::env::current_dir()?)?;
        Ok(Self::new(storage, project))
    }

    fn load_store(&self) -> anyhow::Result<MemoryStore> {
        match self.storage.read(&["memory", &self.project.id]) {
            Ok(store) => Ok(store),
            Err(super::storage::StorageError::NotFound(_)) => Ok(MemoryStore::default()),
            Err(e) => Err(e.into()),
        }
    }

    fn save_store(&self, store: &MemoryStore) -> anyhow::Result<()> {
        self.storage.write(&["memory", &self.project.id], store)?;
        Ok(())
    }

    /// Add a new memory item
    ///
    /// # Errors
    ///
    /// Returns error if storage fails.
    pub fn add(&self, item: MemoryItem) -> anyhow::Result<String> {
        let mut store = self.load_store()?;
        let id = item.id.clone();
        store.items.push(item);
        self.save_store(&store)?;
        Ok(id)
    }

    /// Get a memory item by ID
    ///
    /// # Errors
    ///
    /// Returns error if storage fails.
    pub fn get(&self, id: &str) -> anyhow::Result<Option<MemoryItem>> {
        let mut store = self.load_store()?;

        let item = store.items.iter_mut().find(|i| i.id == id);

        if let Some(item) = item {
            item.accessed_at = Utc::now();
            item.access_count += 1;
            let result = item.clone();
            self.save_store(&store)?;
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    /// List all memory items, optionally filtered by category
    ///
    /// # Errors
    ///
    /// Returns error if storage fails.
    pub fn list(&self, category: Option<MemoryCategory>) -> anyhow::Result<Vec<MemoryItem>> {
        let store = self.load_store()?;

        let items = if let Some(cat) = category {
            store
                .items
                .into_iter()
                .filter(|i| i.category == cat)
                .collect()
        } else {
            store.items
        };

        Ok(items)
    }

    /// Search memories by content
    ///
    /// # Errors
    ///
    /// Returns error if storage fails.
    pub fn search(&self, query: &str) -> anyhow::Result<Vec<MemoryItem>> {
        let store = self.load_store()?;
        let query_lower = query.to_lowercase();

        let items: Vec<_> = store
            .items
            .into_iter()
            .filter(|i| {
                i.content.to_lowercase().contains(&query_lower)
                    || i.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect();

        Ok(items)
    }

    /// Get memories relevant for context injection
    ///
    /// Returns pinned items plus recently accessed items, up to `max_items`.
    ///
    /// # Errors
    ///
    /// Returns error if storage fails.
    pub fn get_context(&self, max_items: usize) -> anyhow::Result<Vec<MemoryItem>> {
        let store = self.load_store()?;

        // Separate pinned and unpinned
        let (pinned, mut unpinned): (Vec<_>, Vec<_>) =
            store.items.into_iter().partition(|i| i.pinned);

        // Sort unpinned by access recency
        unpinned.sort_by(|a, b| b.accessed_at.cmp(&a.accessed_at));

        // Combine: pinned first, then recent
        let mut result = pinned;
        let remaining = max_items.saturating_sub(result.len());
        result.extend(unpinned.into_iter().take(remaining));

        Ok(result)
    }

    /// Delete a memory item
    ///
    /// # Errors
    ///
    /// Returns error if storage fails.
    pub fn delete(&self, id: &str) -> anyhow::Result<bool> {
        let mut store = self.load_store()?;
        let len_before = store.items.len();
        store.items.retain(|i| i.id != id);
        let deleted = store.items.len() < len_before;
        self.save_store(&store)?;
        Ok(deleted)
    }

    /// Update a memory item
    ///
    /// # Errors
    ///
    /// Returns error if item not found or storage fails.
    pub fn update(
        &self,
        id: &str,
        content: Option<String>,
        pinned: Option<bool>,
    ) -> anyhow::Result<()> {
        let mut store = self.load_store()?;

        let item = store
            .items
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or_else(|| anyhow::anyhow!("memory not found: {id}"))?;

        if let Some(c) = content {
            item.content = c;
        }
        if let Some(p) = pinned {
            item.pinned = p;
        }

        self.save_store(&store)?;
        Ok(())
    }

    /// Format memories for system prompt injection
    #[must_use]
    pub fn format_for_prompt(items: &[MemoryItem]) -> String {
        memory::format_for_prompt(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_item_creation() {
        let item = MemoryItem::new(
            "User prefers tabs over spaces".to_string(),
            MemoryCategory::Preference,
        );

        assert!(item.id.starts_with("mem_"));
        assert_eq!(item.category, MemoryCategory::Preference);
        assert!(!item.pinned);
    }

    #[test]
    fn memory_item_with_tags() {
        let item = MemoryItem::new("Use axum for HTTP".to_string(), MemoryCategory::Fact)
            .with_tag("rust")
            .with_tag("http")
            .pinned();

        assert_eq!(item.tags, vec!["rust", "http"]);
        assert!(item.pinned);
    }

    #[test]
    fn format_for_prompt_empty() {
        let output = MemoryManager::format_for_prompt(&[]);
        assert!(output.is_empty());
    }

    #[test]
    fn format_for_prompt_items() {
        let items = vec![
            MemoryItem::new("User prefers vim".to_string(), MemoryCategory::Preference),
            MemoryItem::new("Uses tokio runtime".to_string(), MemoryCategory::Fact),
        ];

        let output = MemoryManager::format_for_prompt(&items);
        assert!(output.contains("<memory>"));
        assert!(output.contains("[preference] User prefers vim"));
        assert!(output.contains("[fact] Uses tokio runtime"));
    }
}
