//! Knowledge pack resolution and prompt injection
//!
//! Thin wrapper around `agent_core::knowledge` shared infrastructure.
//! Re-exports types and functions for use within the CLI

// Re-export everything consumers need
pub use agent_core::knowledge::{
    Embedder, KnowledgePackResolver, ResolverError, build_knowledge_context, build_retrieval_query,
    format_knowledge, resolve_and_merge, select_knowledge, select_knowledge_with_embeddings,
};

#[cfg(test)]
mod tests {
    use agent_core::knowledge::{KnowledgeChunk, KnowledgePriority};

    use super::*;

    fn make_chunk(topic: &str, tags: &[&str], priority: KnowledgePriority) -> KnowledgeChunk {
        KnowledgeChunk {
            topic: Some(topic.to_string()),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            content: format!("Content about {topic}"),
            rules: vec![],
            priority,
            embedding: None,
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
    fn bm25_selects_relevant() {
        let chunks = vec![
            KnowledgeChunk {
                topic: Some("Token Info".to_string()),
                tags: vec!["token".to_string(), "mcg".to_string()],
                content: "MCG is a Solana token".to_string(),
                rules: vec![],
                priority: KnowledgePriority::Relevant,
                embedding: None,
            },
            KnowledgeChunk {
                topic: Some("Platform".to_string()),
                tags: vec!["platform".to_string()],
                content: "Omni is a developer platform".to_string(),
                rules: vec![],
                priority: KnowledgePriority::Relevant,
                embedding: None,
            },
        ];

        let selected = select_knowledge(&chunks, "tell me about the MCG token", 10000);
        assert!(!selected.is_empty());
        assert_eq!(selected[0].topic.as_deref(), Some("Token Info"));
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
}
