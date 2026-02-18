//! Knowledge pack resolution and prompt injection
//!
//! Provides a simplified resolver for fetching knowledge packs from Manifold,
//! caching them locally, and selecting relevant chunks for system prompt injection

use std::fmt::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use thiserror::Error;

use crate::config::{
    KnowledgeChunk, KnowledgeConfig, KnowledgePack, KnowledgePackRef, KnowledgePriority,
};

/// Default cache time-to-live (24 hours)
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Default knowledge token budget (rough estimate: 4 chars per token)
const DEFAULT_KNOWLEDGE_TOKEN_BUDGET: usize = 4000;

/// Errors that can occur during knowledge pack resolution
#[derive(Debug, Error)]
pub enum ResolverError {
    /// Failed to fetch pack from Manifold
    #[error("manifold fetch failed: {0}")]
    Fetch(String),

    /// Failed to parse pack content
    #[error("invalid pack format: {0}")]
    Parse(String),

    /// Failed to read or write cache
    #[error("cache error: {0}")]
    Cache(String),

    /// Invalid pack reference format
    #[error("invalid pack ref: {0}")]
    InvalidRef(String),
}

/// Result type for resolver operations
type Result<T> = std::result::Result<T, ResolverError>;

/// Resolve knowledge pack references to full packs via Manifold
///
/// Fetches packs from the Manifold registry and caches them locally
/// to avoid repeated network requests
#[derive(Debug)]
pub struct CliKnowledgeResolver {
    manifold_url: String,
    cache_dir: PathBuf,
    cache_ttl: Duration,
    client: reqwest::Client,
}

impl CliKnowledgeResolver {
    /// Create a new resolver with default TTL (24 hours)
    ///
    /// Uses `~/.cache/omni/knowledge/` as the default cache directory
    ///
    /// # Errors
    ///
    /// Returns an error if the cache directory cannot be determined
    pub fn new(manifold_url: &str) -> std::result::Result<Self, ResolverError> {
        let cache_dir = default_cache_dir()
            .ok_or_else(|| ResolverError::Cache("cannot determine cache directory".to_string()))?;

        Ok(Self {
            manifold_url: manifold_url.trim_end_matches('/').to_string(),
            cache_dir,
            cache_ttl: DEFAULT_CACHE_TTL,
            client: reqwest::Client::new(),
        })
    }

    /// Create a resolver with a custom cache directory
    #[must_use]
    pub fn with_cache_dir(manifold_url: &str, cache_dir: PathBuf) -> Self {
        Self {
            manifold_url: manifold_url.trim_end_matches('/').to_string(),
            cache_dir,
            cache_ttl: DEFAULT_CACHE_TTL,
            client: reqwest::Client::new(),
        }
    }

    /// Resolve a single pack reference to a full knowledge pack
    ///
    /// Checks the local cache first; fetches from Manifold if the cache
    /// is missing or stale
    ///
    /// # Errors
    ///
    /// Returns an error if the pack cannot be fetched or parsed
    pub async fn resolve(&self, pack_ref: &KnowledgePackRef) -> Result<KnowledgePack> {
        // Check cache first
        if let Some(cached) = self.read_cache(pack_ref) {
            tracing::debug!(pack_ref = %pack_ref.pack_ref, "using cached knowledge pack");
            return Ok(cached);
        }

        // Fetch from Manifold
        let pack = self.fetch_from_manifold(pack_ref).await?;

        // Write to cache (log but don't fail on cache write errors)
        if let Err(e) = self.write_cache(pack_ref, &pack) {
            tracing::warn!(
                pack_ref = %pack_ref.pack_ref,
                error = %e,
                "failed to cache knowledge pack"
            );
        }

        Ok(pack)
    }

    /// Resolve all pack references concurrently
    ///
    /// Returns a vec of results in the same order as the input refs
    pub async fn resolve_all(&self, refs: &[KnowledgePackRef]) -> Vec<Result<KnowledgePack>> {
        let futures: Vec<_> = refs.iter().map(|r| self.resolve(r)).collect();
        futures::future::join_all(futures).await
    }

    /// Build the cache file path for a pack reference
    ///
    /// Layout: `{cache_dir}/{namespace}/{pack_name}/{version}.json`
    fn cache_path(&self, pack_ref: &KnowledgePackRef) -> Result<PathBuf> {
        let (namespace, pack_name) = parse_pack_ref(&pack_ref.pack_ref)?;
        let version = pack_ref.version.as_deref().unwrap_or("latest");

        Ok(self
            .cache_dir
            .join(namespace)
            .join(pack_name)
            .join(format!("{version}.json")))
    }

    /// Read a pack from the local cache if it exists and is fresh
    fn read_cache(&self, pack_ref: &KnowledgePackRef) -> Option<KnowledgePack> {
        let path = self.cache_path(pack_ref).ok()?;

        let metadata = std::fs::metadata(&path).ok()?;
        let modified = metadata.modified().ok()?;
        let age = SystemTime::now().duration_since(modified).ok()?;

        if age > self.cache_ttl {
            tracing::debug!(
                pack_ref = %pack_ref.pack_ref,
                age_secs = age.as_secs(),
                "cache entry is stale"
            );
            return None;
        }

        let contents = std::fs::read_to_string(&path).ok()?;

        match serde_json::from_str::<KnowledgePack>(&contents) {
            Ok(pack) => Some(pack),
            Err(e) => {
                tracing::warn!(
                    pack_ref = %pack_ref.pack_ref,
                    error = %e,
                    "corrupt cache entry"
                );
                None
            }
        }
    }

    /// Write a resolved pack to the local cache
    fn write_cache(&self, pack_ref: &KnowledgePackRef, pack: &KnowledgePack) -> Result<()> {
        let path = self.cache_path(pack_ref)?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ResolverError::Cache(format!("failed to create cache dir: {e}")))?;
        }

        let json = serde_json::to_string_pretty(pack)
            .map_err(|e| ResolverError::Cache(format!("failed to serialize pack: {e}")))?;

        std::fs::write(&path, json)
            .map_err(|e| ResolverError::Cache(format!("failed to write cache file: {e}")))?;

        tracing::debug!(
            pack_ref = %pack_ref.pack_ref,
            path = %path.display(),
            "cached knowledge pack"
        );

        Ok(())
    }

    /// Fetch a knowledge pack from the Manifold registry
    async fn fetch_from_manifold(&self, pack_ref: &KnowledgePackRef) -> Result<KnowledgePack> {
        let (namespace, pack_name) = parse_pack_ref(&pack_ref.pack_ref)?;

        let url = format!(
            "{}/@{}/knowledge/{}",
            self.manifold_url, namespace, pack_name
        );

        tracing::debug!(url = %url, "fetching knowledge pack from manifold");

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| ResolverError::Fetch(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ResolverError::Fetch(format!(
                "pack not found: {} ({})",
                pack_ref.pack_ref,
                response.status()
            )));
        }

        let body = response
            .text()
            .await
            .map_err(|e| ResolverError::Fetch(e.to_string()))?;

        serde_json::from_str::<KnowledgePack>(&body)
            .map_err(|e| ResolverError::Parse(format!("{}: {e}", pack_ref.pack_ref)))
    }
}

/// Select relevant knowledge chunks based on user message
///
/// Strategy:
/// 1. All chunks with priority "always" are included unconditionally
/// 2. For "relevant" chunks, match tags against words in the user message
/// 3. Trim to token budget
#[must_use]
pub fn select_knowledge<'a>(
    chunks: &'a [KnowledgeChunk],
    user_message: &str,
    max_tokens: usize,
) -> Vec<&'a KnowledgeChunk> {
    let mut selected: Vec<&KnowledgeChunk> = Vec::new();

    // Always-priority chunks first
    for chunk in chunks {
        if chunk.priority == KnowledgePriority::Always {
            selected.push(chunk);
        }
    }

    // Strip punctuation and split into clean tokens for tag matching
    let message_lower = user_message.to_lowercase();
    let tokens: Vec<String> = message_lower
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|t| !t.is_empty())
        .collect();

    for chunk in chunks {
        if chunk.priority != KnowledgePriority::Relevant {
            continue;
        }

        let matched = chunk.tags.iter().any(|tag| {
            let tag_lower = tag.to_lowercase();
            tokens.contains(&tag_lower)
        });

        if matched {
            selected.push(chunk);
        }
    }

    // Trim to token budget
    trim_to_budget(&mut selected, max_tokens);

    selected
}

/// Format selected knowledge chunks as markdown for prompt injection
#[must_use]
pub fn format_knowledge(chunks: &[&KnowledgeChunk]) -> String {
    if chunks.is_empty() {
        return String::new();
    }

    let sections: Vec<String> = chunks
        .iter()
        .map(|chunk| {
            let topic = chunk.topic.as_deref().unwrap_or("Knowledge");
            let mut section = format!("## {topic}\n{}", chunk.content);
            if !chunk.rules.is_empty() {
                section.push_str("\n\nRules:");
                for rule in &chunk.rules {
                    let _ = write!(section, "\n- {rule}");
                }
            }
            section
        })
        .collect();

    sections.join("\n\n")
}

/// Build the knowledge context string for system prompt injection
///
/// Resolves pack references (if a resolver is provided), merges with inline
/// chunks, selects relevant chunks for the user message, and formats as a
/// `<knowledge>` XML block
#[must_use]
pub fn build_knowledge_context(chunks: &[KnowledgeChunk], user_message: &str) -> String {
    let selected = select_knowledge(chunks, user_message, DEFAULT_KNOWLEDGE_TOKEN_BUDGET);
    let formatted = format_knowledge(&selected);

    if formatted.is_empty() {
        return String::new();
    }

    format!("<knowledge>\n{formatted}\n</knowledge>")
}

/// Resolve all knowledge pack refs and merge chunks with inline knowledge
///
/// Pack chunks are appended after inline chunks. If a pack ref specifies
/// a priority override, all chunks from that pack inherit the override
///
/// # Errors
///
/// Returns an error if the resolver cannot be created
pub async fn resolve_and_merge(
    config: &KnowledgeConfig,
    manifold_url: &str,
) -> std::result::Result<Vec<KnowledgeChunk>, ResolverError> {
    let mut all_chunks = config.inline.clone();

    if config.packs.is_empty() {
        return Ok(all_chunks);
    }

    let resolver = CliKnowledgeResolver::new(manifold_url)?;
    let results = resolver.resolve_all(&config.packs).await;

    for (i, result) in results.into_iter().enumerate() {
        match result {
            Ok(pack) => {
                tracing::info!(
                    name = %pack.name,
                    chunks = pack.chunks.len(),
                    "loaded knowledge pack"
                );

                // Apply priority override from the pack ref if set
                let priority_override = config.packs.get(i).and_then(|r| r.priority);

                for mut chunk in pack.chunks {
                    if let Some(priority) = priority_override {
                        chunk.priority = priority;
                    }
                    all_chunks.push(chunk);
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to resolve knowledge pack");
            }
        }
    }

    Ok(all_chunks)
}

/// Parse a pack ref string into (namespace, `pack_name`)
///
/// Expected format: `@{namespace}/knowledge/{pack_name}`
fn parse_pack_ref(pack_ref: &str) -> Result<(&str, &str)> {
    let trimmed = pack_ref.strip_prefix('@').unwrap_or(pack_ref);

    let parts: Vec<&str> = trimmed.splitn(3, '/').collect();

    match parts.as_slice() {
        [namespace, "knowledge", pack_name] => Ok((namespace, pack_name)),
        _ => Err(ResolverError::InvalidRef(format!(
            "expected @{{namespace}}/knowledge/{{pack_name}}, got: {pack_ref}"
        ))),
    }
}

/// Get the default knowledge cache directory (`~/.cache/omni/knowledge/`)
fn default_cache_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|base| base.cache_dir().join("omni").join("knowledge"))
}

/// Rough token estimation (4 chars per token)
const fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// Trim chunks to fit within a token budget
fn trim_to_budget(chunks: &mut Vec<&KnowledgeChunk>, max_tokens: usize) {
    let mut total_tokens = 0;
    let mut keep = 0;

    for chunk in chunks.iter() {
        let topic_str = chunk.topic.as_deref().unwrap_or("");
        let chunk_tokens = estimate_tokens(&chunk.content) + estimate_tokens(topic_str);
        for rule in &chunk.rules {
            total_tokens += estimate_tokens(rule);
        }
        total_tokens += chunk_tokens;

        if total_tokens > max_tokens && keep > 0 {
            break;
        }
        keep += 1;
    }

    chunks.truncate(keep);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_chunk(topic: &str, tags: &[&str], priority: KnowledgePriority) -> KnowledgeChunk {
        KnowledgeChunk {
            topic: Some(topic.to_string()),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            content: format!("Content about {topic}"),
            rules: vec![],
            priority,
        }
    }

    #[test]
    fn always_chunks_included() {
        let chunks = vec![
            make_chunk("Token Info", &["token"], KnowledgePriority::Always),
            make_chunk("Platform", &["platform"], KnowledgePriority::Relevant),
        ];

        let selected = select_knowledge(&chunks, "random question", 10000);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].topic.as_deref(), Some("Token Info"));
    }

    #[test]
    fn tag_matching_selects_relevant() {
        let chunks = vec![
            make_chunk("Token Info", &["token", "mcg"], KnowledgePriority::Relevant),
            make_chunk("Platform", &["platform"], KnowledgePriority::Relevant),
        ];

        let selected = select_knowledge(&chunks, "tell me about the token", 10000);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].topic.as_deref(), Some("Token Info"));
    }

    #[test]
    fn multiple_tag_matches() {
        let chunks = vec![
            make_chunk("Token", &["token"], KnowledgePriority::Relevant),
            make_chunk("Platform", &["platform"], KnowledgePriority::Relevant),
        ];

        let selected = select_knowledge(&chunks, "token and platform", 10000);
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn always_plus_relevant() {
        let chunks = vec![
            make_chunk("Core", &[], KnowledgePriority::Always),
            make_chunk("Token", &["token"], KnowledgePriority::Relevant),
            make_chunk("Other", &["other"], KnowledgePriority::Relevant),
        ];

        let selected = select_knowledge(&chunks, "what is the token?", 10000);
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].topic.as_deref(), Some("Core"));
        assert_eq!(selected[1].topic.as_deref(), Some("Token"));
    }

    #[test]
    fn no_matches_returns_empty() {
        let chunks = vec![make_chunk("Token", &["token"], KnowledgePriority::Relevant)];

        let selected = select_knowledge(&chunks, "hello world", 10000);
        assert!(selected.is_empty());
    }

    #[test]
    fn tag_matching_strips_punctuation() {
        let chunks = vec![make_chunk("Token", &["mcg"], KnowledgePriority::Relevant)];

        let selected = select_knowledge(&chunks, "what is $mcg?", 10000);
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn case_insensitive_matching() {
        let chunks = vec![make_chunk("Token", &["MCG"], KnowledgePriority::Relevant)];

        let selected = select_knowledge(&chunks, "what is mcg?", 10000);
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn format_knowledge_empty() {
        let formatted = format_knowledge(&[]);
        assert!(formatted.is_empty());
    }

    #[test]
    fn format_knowledge_with_rules() {
        let chunk = KnowledgeChunk {
            topic: Some("Token".to_string()),
            tags: vec![],
            content: "MCG is on Solana".to_string(),
            rules: vec!["Always cite mint address".to_string()],
            priority: KnowledgePriority::Always,
        };

        let formatted = format_knowledge(&[&chunk]);
        assert!(formatted.contains("## Token"));
        assert!(formatted.contains("MCG is on Solana"));
        assert!(formatted.contains("Rules:"));
        assert!(formatted.contains("- Always cite mint address"));
    }

    #[test]
    fn build_knowledge_context_wraps_in_xml() {
        let chunks = vec![make_chunk("Core", &[], KnowledgePriority::Always)];

        let context = build_knowledge_context(&chunks, "anything");
        assert!(context.starts_with("<knowledge>"));
        assert!(context.ends_with("</knowledge>"));
        assert!(context.contains("## Core"));
    }

    #[test]
    fn build_knowledge_context_empty_when_no_matches() {
        let chunks = vec![make_chunk("Token", &["token"], KnowledgePriority::Relevant)];

        let context = build_knowledge_context(&chunks, "unrelated message");
        assert!(context.is_empty());
    }

    #[test]
    fn parse_valid_pack_ref() {
        let (ns, name) = parse_pack_ref("@omni/knowledge/crypto-basics").unwrap();
        assert_eq!(ns, "omni");
        assert_eq!(name, "crypto-basics");
    }

    #[test]
    fn parse_pack_ref_without_at() {
        let (ns, name) = parse_pack_ref("omni/knowledge/crypto-basics").unwrap();
        assert_eq!(ns, "omni");
        assert_eq!(name, "crypto-basics");
    }

    #[test]
    fn parse_invalid_pack_ref() {
        let result = parse_pack_ref("invalid-ref");
        assert!(result.is_err());
    }

    #[test]
    fn resolver_trims_trailing_slash() {
        let resolver = CliKnowledgeResolver::with_cache_dir(
            "https://manifold.omni.dev/",
            PathBuf::from("/tmp/cache"),
        );
        assert_eq!(resolver.manifold_url, "https://manifold.omni.dev");
    }

    #[test]
    fn cache_path_layout() {
        let resolver = CliKnowledgeResolver::with_cache_dir(
            "https://manifold.omni.dev",
            PathBuf::from("/tmp/cache"),
        );

        let pack_ref = KnowledgePackRef {
            pack_ref: "@omni/knowledge/crypto-basics".to_string(),
            version: Some("1.0.0".to_string()),
            priority: None,
        };

        let path = resolver.cache_path(&pack_ref).unwrap();
        assert_eq!(
            path,
            PathBuf::from("/tmp/cache/omni/crypto-basics/1.0.0.json")
        );
    }

    #[test]
    fn cache_path_defaults_to_latest() {
        let resolver = CliKnowledgeResolver::with_cache_dir(
            "https://manifold.omni.dev",
            PathBuf::from("/tmp/cache"),
        );

        let pack_ref = KnowledgePackRef {
            pack_ref: "@omni/knowledge/crypto-basics".to_string(),
            version: None,
            priority: None,
        };

        let path = resolver.cache_path(&pack_ref).unwrap();
        assert_eq!(
            path,
            PathBuf::from("/tmp/cache/omni/crypto-basics/latest.json")
        );
    }
}
