//! Unified LLM provider using the `llm` crate.
//!
//! Wraps multiple providers (Anthropic, `OpenAI`, Google, Groq, Mistral)
//! behind a common interface.

use async_trait::async_trait;
use futures::StreamExt;
use llm::builder::{LLMBackend, LLMBuilder};
use llm::chat::{ChatMessage, FunctionTool, StreamChunk, Tool as LlmTool};
use llm::{FunctionCall, LLMProvider, ToolCall as LlmToolCall};

use crate::error::{AgentError, Result};
use crate::provider::{CompletionEvent, CompletionRequest, CompletionStream, LlmProvider};
use crate::types::{Content, ContentBlock, Message, Role, StopReason, Tool};

/// Unified LLM provider supporting multiple backends.
pub struct UnifiedProvider {
    inner: Box<dyn LLMProvider>,
    name: &'static str,
}

impl std::fmt::Debug for UnifiedProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnifiedProvider")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl UnifiedProvider {
    /// Create a new Anthropic provider.
    pub fn anthropic(api_key: impl Into<String>) -> Result<Self> {
        let api_key = api_key.into();
        if api_key.is_empty() {
            return Err(AgentError::ApiKeyMissing);
        }

        let provider = LLMBuilder::new()
            .backend(LLMBackend::Anthropic)
            .api_key(api_key)
            .build()
            .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

        Ok(Self {
            inner: provider,
            name: "anthropic",
        })
    }

    /// Create a new `OpenAI` provider.
    pub fn openai(api_key: Option<String>, base_url: Option<String>) -> Result<Self> {
        let mut builder = LLMBuilder::new().backend(LLMBackend::OpenAI);

        if let Some(key) = api_key {
            builder = builder.api_key(key);
        }

        if let Some(url) = base_url {
            builder = builder.base_url(url);
        }

        let provider = builder
            .build()
            .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

        Ok(Self {
            inner: provider,
            name: "openai",
        })
    }

    /// Create a new Google Gemini provider.
    pub fn google(api_key: impl Into<String>) -> Result<Self> {
        let api_key = api_key.into();
        if api_key.is_empty() {
            return Err(AgentError::ApiKeyMissing);
        }

        let provider = LLMBuilder::new()
            .backend(LLMBackend::Google)
            .api_key(api_key)
            .build()
            .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

        Ok(Self {
            inner: provider,
            name: "google",
        })
    }

    /// Create a new Groq provider.
    pub fn groq(api_key: impl Into<String>) -> Result<Self> {
        let api_key = api_key.into();
        if api_key.is_empty() {
            return Err(AgentError::ApiKeyMissing);
        }

        let provider = LLMBuilder::new()
            .backend(LLMBackend::Groq)
            .api_key(api_key)
            .build()
            .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

        Ok(Self {
            inner: provider,
            name: "groq",
        })
    }

    /// Create a new Mistral provider.
    pub fn mistral(api_key: impl Into<String>) -> Result<Self> {
        let api_key = api_key.into();
        if api_key.is_empty() {
            return Err(AgentError::ApiKeyMissing);
        }

        let provider = LLMBuilder::new()
            .backend(LLMBackend::Mistral)
            .api_key(api_key)
            .build()
            .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

        Ok(Self {
            inner: provider,
            name: "mistral",
        })
    }
}

/// Convert our messages to the llm crate format.
fn convert_messages(messages: &[Message], system: Option<&str>) -> Vec<ChatMessage> {
    let mut result = Vec::new();

    // Add system message if present (as a user message with system context)
    if let Some(sys) = system {
        result.push(
            ChatMessage::user()
                .content(format!("[System]\n{sys}"))
                .build(),
        );
    }

    for msg in messages {
        match &msg.content {
            Content::Text(text) => {
                let chat_msg = match msg.role {
                    Role::User => ChatMessage::user().content(text.clone()).build(),
                    Role::Assistant => ChatMessage::assistant().content(text.clone()).build(),
                };
                result.push(chat_msg);
            }
            Content::Blocks(blocks) => {
                // Collect text parts and tool interactions
                let mut text_parts = Vec::new();
                let mut tool_uses = Vec::new();
                let mut tool_results = Vec::new();

                for block in blocks {
                    match block {
                        ContentBlock::Text { text } => {
                            text_parts.push(text.clone());
                        }
                        ContentBlock::ToolUse { id, name, input } => {
                            tool_uses.push(LlmToolCall {
                                id: id.clone(),
                                call_type: "function".to_string(),
                                function: FunctionCall {
                                    name: name.clone(),
                                    arguments: serde_json::to_string(input).unwrap_or_default(),
                                },
                            });
                        }
                        ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            is_error,
                        } => {
                            tool_results.push(LlmToolCall {
                                id: tool_use_id.clone(),
                                call_type: "function".to_string(),
                                function: FunctionCall {
                                    name: if is_error.unwrap_or(false) {
                                        "error".to_string()
                                    } else {
                                        "result".to_string()
                                    },
                                    arguments: content.clone(),
                                },
                            });
                        }
                    }
                }

                // Emit assistant message with tool uses
                if !tool_uses.is_empty() {
                    let text = if text_parts.is_empty() {
                        String::new()
                    } else {
                        text_parts.join("")
                    };
                    result.push(
                        ChatMessage::assistant()
                            .content(text)
                            .tool_use(tool_uses)
                            .build(),
                    );
                } else if !text_parts.is_empty() {
                    let text = text_parts.join("");
                    let chat_msg = match msg.role {
                        Role::User => ChatMessage::user().content(text).build(),
                        Role::Assistant => ChatMessage::assistant().content(text).build(),
                    };
                    result.push(chat_msg);
                }

                // Emit tool results as user message
                if !tool_results.is_empty() {
                    result.push(ChatMessage::user().tool_result(tool_results).build());
                }
            }
        }
    }

    result
}

/// Convert our tools to llm crate format.
fn convert_tools(tools: &[Tool]) -> Vec<LlmTool> {
    tools
        .iter()
        .map(|t| LlmTool {
            tool_type: "function".to_string(),
            function: FunctionTool {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.input_schema.clone(),
            },
        })
        .collect()
}

#[async_trait]
impl LlmProvider for UnifiedProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream> {
        let messages = convert_messages(&request.messages, request.system.as_deref());
        let tools = request.tools.as_ref().map(|t| convert_tools(t));

        let stream_result = self
            .inner
            .chat_stream_with_tools(&messages, tools.as_deref())
            .await
            .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

        let stream = async_stream::stream! {
            let mut current_text = String::new();
            let text_block_index = 0_usize;

            futures::pin_mut!(stream_result);

            while let Some(chunk) = stream_result.next().await {
                match chunk {
                    Ok(StreamChunk::Text(text)) => {
                        current_text.push_str(&text);
                        yield Ok(CompletionEvent::TextDelta(text));
                    }
                    Ok(StreamChunk::ToolUseStart { index, id, name }) => {
                        yield Ok(CompletionEvent::ToolUseStart {
                            index: text_block_index + 1 + index,
                            id,
                            name,
                        });
                    }
                    Ok(StreamChunk::ToolUseInputDelta { index, partial_json }) => {
                        yield Ok(CompletionEvent::ToolInputDelta {
                            index: text_block_index + 1 + index,
                            partial_json,
                        });
                    }
                    Ok(StreamChunk::ToolUseComplete { index, tool_call }) => {
                        let input = serde_json::from_str(&tool_call.function.arguments)
                            .unwrap_or(serde_json::Value::Null);
                        yield Ok(CompletionEvent::ContentBlockDone {
                            index: text_block_index + 1 + index,
                            block: ContentBlock::ToolUse {
                                id: tool_call.id,
                                name: tool_call.function.name,
                                input,
                            },
                        });
                    }
                    Ok(StreamChunk::Done { stop_reason }) => {
                        // Emit text block done if we have text
                        if !current_text.is_empty() {
                            yield Ok(CompletionEvent::ContentBlockDone {
                                index: text_block_index,
                                block: ContentBlock::Text { text: current_text.clone() },
                            });
                        }

                        let reason = match stop_reason.as_str() {
                            "end_turn" | "stop" => Some(StopReason::EndTurn),
                            "tool_use" | "tool_calls" => Some(StopReason::ToolUse),
                            "max_tokens" | "length" => Some(StopReason::MaxTokens),
                            _ => None,
                        };

                        yield Ok(CompletionEvent::Done {
                            stop_reason: reason,
                            usage: None,
                        });
                    }
                    Err(e) => {
                        yield Ok(CompletionEvent::Error(e.to_string()));
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_requires_api_key() {
        let result = UnifiedProvider::anthropic("");
        assert!(result.is_err());
    }

    #[test]
    fn google_requires_api_key() {
        let result = UnifiedProvider::google("");
        assert!(result.is_err());
    }

    #[test]
    fn groq_requires_api_key() {
        let result = UnifiedProvider::groq("");
        assert!(result.is_err());
    }

    #[test]
    fn mistral_requires_api_key() {
        let result = UnifiedProvider::mistral("");
        assert!(result.is_err());
    }
}
