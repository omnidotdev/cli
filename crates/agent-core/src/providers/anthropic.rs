//! Anthropic (Claude) provider implementation.

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};

use crate::error::{AgentError, Result};
use crate::provider::{CompletionEvent, CompletionRequest, CompletionStream, LlmProvider};
use crate::types::{ContentBlock, Delta, MessagesRequest, StreamEvent};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";

/// Anthropic (Claude) LLM provider.
#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    http: reqwest::Client,
    api_key: String,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider.
    ///
    /// # Errors
    ///
    /// Returns error if API key is empty.
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        let api_key = api_key.into();
        if api_key.is_empty() {
            return Err(AgentError::ApiKeyMissing);
        }

        Ok(Self {
            http: reqwest::Client::new(),
            api_key,
        })
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&self.api_key).map_err(|_| AgentError::ApiKeyMissing)?,
        );
        headers.insert("anthropic-version", HeaderValue::from_static(API_VERSION));

        // Convert to Anthropic-specific request format
        let anthropic_request = MessagesRequest {
            model: request.model,
            max_tokens: request.max_tokens,
            messages: request.messages,
            system: request.system,
            tools: request.tools,
            stream: true,
        };

        let response = self
            .http
            .post(API_URL)
            .headers(headers)
            .json(&anthropic_request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let message = response.text().await.unwrap_or_default();
            return Err(AgentError::Api {
                status: status.as_u16(),
                message,
            });
        }

        let byte_stream = response.bytes_stream();

        let stream = async_stream::stream! {
            let mut buffer = String::new();
            let mut current_blocks: Vec<ContentBlock> = Vec::new();

            futures::pin_mut!(byte_stream);

            while let Some(chunk) = byte_stream.next().await {
                let chunk = chunk?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Process complete SSE events
                while let Some((event_opt, remainder)) = parse_sse_event(&buffer) {
                    buffer = remainder;

                    let Some(event) = event_opt else {
                        continue;
                    };

                    // Convert Anthropic events to generic completion events
                    match event {
                        StreamEvent::ContentBlockStart { index, content_block } => {
                            // Ensure we have space
                            while current_blocks.len() <= index {
                                current_blocks.push(ContentBlock::Text { text: String::new() });
                            }
                            current_blocks[index] = content_block.clone();

                            if let ContentBlock::ToolUse { id, name, .. } = content_block {
                                yield Ok(CompletionEvent::ToolUseStart { index, id, name });
                            }
                        }

                        StreamEvent::ContentBlockDelta { index, delta } => {
                            match delta {
                                Delta::TextDelta { text } => {
                                    // Update accumulated block
                                    if let Some(ContentBlock::Text { text: t }) = current_blocks.get_mut(index) {
                                        t.push_str(&text);
                                    }
                                    yield Ok(CompletionEvent::TextDelta(text));
                                }
                                Delta::InputJsonDelta { partial_json } => {
                                    yield Ok(CompletionEvent::ToolInputDelta { index, partial_json });
                                }
                            }
                        }

                        StreamEvent::ContentBlockStop { index } => {
                            if let Some(block) = current_blocks.get(index).cloned() {
                                yield Ok(CompletionEvent::ContentBlockDone { index, block });
                            }
                        }

                        StreamEvent::MessageDelta { delta, usage } => {
                            yield Ok(CompletionEvent::Done {
                                stop_reason: delta.stop_reason,
                                usage: Some(usage),
                            });
                        }

                        StreamEvent::Error { error } => {
                            yield Ok(CompletionEvent::Error(error.message));
                        }

                        _ => {}
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }
}

/// Parse a single SSE event from the buffer.
///
/// Returns the parsed event (if any) and the remaining buffer content.
fn parse_sse_event(buffer: &str) -> Option<(Option<StreamEvent>, String)> {
    // Find double newline (end of event)
    let end = buffer.find("\n\n")?;
    let event_str = &buffer[..end];
    let remainder = buffer[end + 2..].to_string();

    // Parse event
    let mut data = None;

    for line in event_str.lines() {
        if let Some(rest) = line.strip_prefix("data: ") {
            data = Some(rest.to_string());
        }
    }

    // Skip non-data events
    let Some(data) = data else {
        return Some((None, remainder));
    };

    // Parse JSON
    match serde_json::from_str::<StreamEvent>(&data) {
        Ok(event) => Some((Some(event), remainder)),
        Err(e) => {
            tracing::debug!(data = %data, error = %e, "failed to parse event");
            Some((None, remainder))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_requires_api_key() {
        let result = AnthropicProvider::new("");
        assert!(result.is_err());
    }

    #[test]
    fn provider_accepts_valid_key() {
        let result = AnthropicProvider::new("test-key");
        assert!(result.is_ok());
    }
}
