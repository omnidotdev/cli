//! Filesystem-backed storage for sessions, messages, and parts.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::Config;

/// Storage errors.
#[derive(Debug, Error)]
pub enum StorageError {
    /// Resource not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// IO error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Storage result type.
pub type Result<T> = std::result::Result<T, StorageError>;

/// Storage backend for persisting data.
pub struct Storage {
    root: PathBuf,
}

impl Storage {
    /// Create a new storage instance at the default location.
    ///
    /// # Errors
    ///
    /// Returns error if data directory cannot be determined.
    pub fn new() -> anyhow::Result<Self> {
        let root = Config::data_dir()?.join("storage");
        Ok(Self { root })
    }

    /// Create a storage instance at a custom location.
    #[must_use]
    pub const fn with_root(root: PathBuf) -> Self {
        Self { root }
    }

    /// Get the storage root path.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Build a path from key segments.
    fn path(&self, key: &[&str]) -> PathBuf {
        let mut path = self.root.clone();
        for segment in key {
            path.push(segment);
        }
        path.set_extension("json");
        path
    }

    /// Read a value from storage.
    ///
    /// # Errors
    ///
    /// Returns error if file doesn't exist or cannot be parsed.
    pub fn read<T>(&self, key: &[&str]) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let path = self.path(key);

        if !path.exists() {
            return Err(StorageError::NotFound(path.display().to_string()));
        }

        let contents = std::fs::read_to_string(&path)?;
        let value: T = serde_json::from_str(&contents)?;
        Ok(value)
    }

    /// Write a value to storage.
    ///
    /// # Errors
    ///
    /// Returns error if file cannot be written.
    pub fn write<T>(&self, key: &[&str], value: &T) -> Result<()>
    where
        T: Serialize,
    {
        let path = self.path(key);

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(value)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    /// Update a value in storage.
    ///
    /// # Errors
    ///
    /// Returns error if file doesn't exist or cannot be updated.
    pub fn update<T, F>(&self, key: &[&str], f: F) -> Result<T>
    where
        T: for<'de> Deserialize<'de> + Serialize,
        F: FnOnce(&mut T),
    {
        let mut value: T = self.read(key)?;
        f(&mut value);
        self.write(key, &value)?;
        Ok(value)
    }

    /// Remove a value from storage.
    ///
    /// # Errors
    ///
    /// Returns error if file cannot be removed.
    pub fn remove(&self, key: &[&str]) -> Result<()> {
        let path = self.path(key);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// List all keys under a prefix.
    ///
    /// Returns keys as vectors of path segments (without .json extension).
    ///
    /// # Errors
    ///
    /// Returns error if directory cannot be read.
    pub fn list(&self, prefix: &[&str]) -> Result<Vec<Vec<String>>> {
        let mut dir = self.root.clone();
        for segment in prefix {
            dir.push(segment);
        }

        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        Self::list_recursive(&dir, prefix, &mut results)?;

        results.sort();
        Ok(results)
    }

    fn list_recursive(dir: &Path, prefix: &[&str], results: &mut Vec<Vec<String>>) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Recursively list subdirectories
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                let mut new_prefix: Vec<&str> = prefix.to_vec();
                // Use leaked string to maintain lifetime (acceptable for path traversal)
                let leaked: &'static str = Box::leak(name.clone().into_boxed_str());
                new_prefix.push(leaked);
                Self::list_recursive(&path, &new_prefix, results)?;
            } else if path.extension().is_some_and(|e| e == "json") {
                // Add file key (without .json extension)
                let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                let mut key: Vec<String> = prefix.iter().map(|s| (*s).to_string()).collect();
                key.push(stem);
                results.push(key);
            }
        }
        Ok(())
    }

    /// Check if a key exists.
    #[must_use]
    pub fn exists(&self, key: &[&str]) -> bool {
        self.path(key).exists()
    }

    /// List and deserialize all items under a prefix
    ///
    /// # Errors
    ///
    /// Returns error if reading or deserialization fails
    pub fn list_prefix<T>(&self, prefix: &str) -> Result<Vec<T>>
    where
        T: serde::de::DeserializeOwned,
    {
        let keys = self.list(&[prefix])?;
        let mut items = Vec::new();

        for key in keys {
            let key_refs: Vec<&str> = key.iter().map(String::as_str).collect();
            if let Ok(item) = self.read(&key_refs) {
                items.push(item);
            }
        }

        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestData {
        id: String,
        value: i32,
    }

    fn temp_storage() -> (Storage, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::with_root(dir.path().to_path_buf());
        (storage, dir)
    }

    #[test]
    fn write_and_read() {
        let (storage, _dir) = temp_storage();
        let data = TestData {
            id: "test".to_string(),
            value: 42,
        };

        storage.write(&["test", "data"], &data).unwrap();
        let read: TestData = storage.read(&["test", "data"]).unwrap();
        assert_eq!(data, read);
    }

    #[test]
    fn read_not_found() {
        let (storage, _dir) = temp_storage();
        let result: Result<TestData> = storage.read(&["nonexistent"]);
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }

    #[test]
    fn update() {
        let (storage, _dir) = temp_storage();
        let data = TestData {
            id: "test".to_string(),
            value: 42,
        };

        storage.write(&["test", "data"], &data).unwrap();
        let updated: TestData = storage
            .update(&["test", "data"], |d: &mut TestData| {
                d.value = 100;
            })
            .unwrap();

        assert_eq!(updated.value, 100);

        let read: TestData = storage.read(&["test", "data"]).unwrap();
        assert_eq!(read.value, 100);
    }

    #[test]
    fn remove() {
        let (storage, _dir) = temp_storage();
        let data = TestData {
            id: "test".to_string(),
            value: 42,
        };

        storage.write(&["test", "data"], &data).unwrap();
        assert!(storage.exists(&["test", "data"]));

        storage.remove(&["test", "data"]).unwrap();
        assert!(!storage.exists(&["test", "data"]));
    }

    #[test]
    fn list() {
        let (storage, _dir) = temp_storage();

        storage
            .write(
                &["session", "proj1", "ses1"],
                &TestData {
                    id: "ses1".to_string(),
                    value: 1,
                },
            )
            .unwrap();
        storage
            .write(
                &["session", "proj1", "ses2"],
                &TestData {
                    id: "ses2".to_string(),
                    value: 2,
                },
            )
            .unwrap();

        let keys = storage.list(&["session", "proj1"]).unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.iter().any(|k| k.last() == Some(&"ses1".to_string())));
        assert!(keys.iter().any(|k| k.last() == Some(&"ses2".to_string())));
    }
}
