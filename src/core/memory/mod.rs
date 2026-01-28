//! Persistent memory system for storing facts across sessions.
//!
//! Memory items are stored per-project and can be:
//! - User preferences (how they like code formatted, etc.)
//! - Project facts (architecture decisions, patterns used)
//! - Learned corrections (things the agent got wrong and was corrected on)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use super::project::Project;
use super::storage::Storage;

/// Memory item categories
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    /// User preferences and coding style
    Preference,
    /// Project-specific facts and decisions
    ProjectFact,
    /// Corrections from user feedback
    Correction,
    /// General learned information
    General,
}

impl std::fmt::Display for MemoryCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preference => write!(f, "preference"),
            Self::ProjectFact => write!(f, "project_fact"),
            Self::Correction => write!(f, "correction"),
            Self::General => write!(f, "general"),
        }
    }
}

/// A single memory item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    /// Unique identifier
    pub id: String,
    /// The memory content
    pub content: String,
    /// Category of memory
    pub category: MemoryCategory,
    /// When this was created
    pub created_at: DateTime<Utc>,
    /// When this was last accessed
    pub accessed_at: DateTime<Utc>,
    /// Number of times this memory was retrieved
    pub access_count: u32,
    /// Optional tags for filtering
    #[serde(default)]
    pub tags: Vec<String>,
    /// Whether this memory is pinned (always included)
    #[serde(default)]
    pub pinned: bool,
}

impl MemoryItem {
    /// Create a new memory item
    #[must_use]
    pub fn new(content: String, category: MemoryCategory) -> Self {
        let now = Utc::now();
        Self {
            id: format!("mem_{}", Ulid::new()),
            content,
            category,
            created_at: now,
            accessed_at: now,
            access_count: 0,
            tags: Vec::new(),
            pinned: false,
        }
    }

    /// Add a tag to this memory
    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Mark this memory as pinned
    #[must_use]
    pub const fn pinned(mut self) -> Self {
        self.pinned = true;
        self
    }
}

/// Memory store for a project
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
        if items.is_empty() {
            return String::new();
        }

        use std::fmt::Write;
        let mut output = String::from("<memory>\n");
        output.push_str("The following are facts I've learned about this project and user:\n\n");

        for item in items {
            let _ = writeln!(output, "- [{}] {}", item.category, item.content);
        }

        output.push_str("</memory>\n");
        output
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
        let item = MemoryItem::new("Use axum for HTTP".to_string(), MemoryCategory::ProjectFact)
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
            MemoryItem::new(
                "Uses tokio runtime".to_string(),
                MemoryCategory::ProjectFact,
            ),
        ];

        let output = MemoryManager::format_for_prompt(&items);
        assert!(output.contains("<memory>"));
        assert!(output.contains("[preference] User prefers vim"));
        assert!(output.contains("[project_fact] Uses tokio runtime"));
    }
}
