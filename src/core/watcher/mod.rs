//! File watching for reactive context updates
//!
//! Uses the notify crate to watch for file changes and emit events

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// File change event
#[derive(Debug, Clone)]
pub struct FileEvent {
    /// Path to the changed file
    pub path: PathBuf,
    /// Type of change
    pub kind: FileEventKind,
}

/// Type of file change
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileEventKind {
    /// File was created
    Created,
    /// File was modified
    Modified,
    /// File was deleted
    Deleted,
}

/// File watcher configuration
#[derive(Debug, Clone)]
pub struct WatcherConfig {
    /// Additional patterns to ignore (beyond gitignore)
    pub ignore_patterns: Vec<String>,
    /// Debounce duration for events
    pub debounce: Duration,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            ignore_patterns: vec![
                // Default ignores
                ".git".to_string(),
                "node_modules".to_string(),
                "target".to_string(),
                ".cache".to_string(),
            ],
            debounce: Duration::from_millis(100),
        }
    }
}

/// File watcher for a directory
pub struct FileWatcher {
    watcher: RecommendedWatcher,
    receiver: mpsc::Receiver<Result<Event, notify::Error>>,
    gitignore: Option<Gitignore>,
    config: WatcherConfig,
    root: PathBuf,
}

impl FileWatcher {
    /// Create a new file watcher for a directory
    ///
    /// # Errors
    ///
    /// Returns error if watcher initialization fails
    pub fn new(root: impl AsRef<Path>, config: WatcherConfig) -> anyhow::Result<Self> {
        let root = root.as_ref().to_path_buf();
        let (tx, rx) = mpsc::channel();

        let watcher_config = Config::default()
            .with_poll_interval(config.debounce)
            .with_compare_contents(false);

        let watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            watcher_config,
        )?;

        // Load gitignore if present
        let gitignore = Self::load_gitignore(&root);

        Ok(Self {
            watcher,
            receiver: rx,
            gitignore,
            config,
            root,
        })
    }

    fn load_gitignore(root: &Path) -> Option<Gitignore> {
        let gitignore_path = root.join(".gitignore");
        if !gitignore_path.exists() {
            return None;
        }

        let mut builder = GitignoreBuilder::new(root);
        if builder.add(&gitignore_path).is_some() {
            return None;
        }

        builder.build().ok()
    }

    /// Start watching the root directory
    ///
    /// # Errors
    ///
    /// Returns error if watching fails
    pub fn watch(&mut self) -> anyhow::Result<()> {
        self.watcher.watch(&self.root, RecursiveMode::Recursive)?;
        tracing::info!(root = %self.root.display(), "started file watcher");
        Ok(())
    }

    /// Stop watching
    ///
    /// # Errors
    ///
    /// Returns error if unwatching fails
    pub fn unwatch(&mut self) -> anyhow::Result<()> {
        self.watcher.unwatch(&self.root)?;
        tracing::info!(root = %self.root.display(), "stopped file watcher");
        Ok(())
    }

    /// Poll for file events (non-blocking)
    #[must_use]
    pub fn poll(&self) -> Vec<FileEvent> {
        let mut events = Vec::new();

        while let Ok(result) = self.receiver.try_recv() {
            if let Ok(event) = result {
                for file_event in self.process_event(event) {
                    events.push(file_event);
                }
            }
        }

        events
    }

    /// Wait for next file event (blocking)
    ///
    /// # Errors
    ///
    /// Returns error if channel is disconnected
    pub fn recv(&self) -> anyhow::Result<Vec<FileEvent>> {
        let result = self
            .receiver
            .recv()
            .map_err(|_| anyhow::anyhow!("Watcher channel disconnected"))?;

        match result {
            Ok(event) => Ok(self.process_event(event)),
            Err(e) => Err(anyhow::anyhow!("Watch error: {e}")),
        }
    }

    /// Wait for next file event with timeout
    ///
    /// # Errors
    ///
    /// Returns error if channel is disconnected
    pub fn recv_timeout(&self, timeout: Duration) -> anyhow::Result<Vec<FileEvent>> {
        match self.receiver.recv_timeout(timeout) {
            Ok(Ok(event)) => Ok(self.process_event(event)),
            Ok(Err(e)) => Err(anyhow::anyhow!("Watch error: {e}")),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(Vec::new()),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(anyhow::anyhow!("Watcher channel disconnected"))
            }
        }
    }

    fn process_event(&self, event: Event) -> Vec<FileEvent> {
        let kind = match event.kind {
            EventKind::Create(_) => FileEventKind::Created,
            EventKind::Modify(_) => FileEventKind::Modified,
            EventKind::Remove(_) => FileEventKind::Deleted,
            _ => return Vec::new(),
        };

        event
            .paths
            .into_iter()
            .filter(|path| !self.is_ignored(path))
            .map(|path| FileEvent { path, kind })
            .collect()
    }

    fn is_ignored(&self, path: &Path) -> bool {
        // Check gitignore
        if let Some(ref gitignore) = self.gitignore {
            let is_dir = path.is_dir();
            if gitignore.matched(path, is_dir).is_ignore() {
                return true;
            }
        }

        // Check config ignores
        let path_str = path.to_string_lossy();
        for pattern in &self.config.ignore_patterns {
            if path_str.contains(pattern) {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_common_ignores() {
        let config = WatcherConfig::default();
        assert!(config.ignore_patterns.contains(&".git".to_string()));
        assert!(config.ignore_patterns.contains(&"node_modules".to_string()));
        assert!(config.ignore_patterns.contains(&"target".to_string()));
    }

    #[test]
    fn file_event_kind_equality() {
        assert_eq!(FileEventKind::Created, FileEventKind::Created);
        assert_ne!(FileEventKind::Created, FileEventKind::Modified);
    }
}
