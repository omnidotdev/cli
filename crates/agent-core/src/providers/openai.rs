//! `OpenAI` provider implementation.
//!
//! Provides streaming completions via the `OpenAI` Chat Completions API.

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};

use crate::error::{AgentError, Result};
use crate::provider::{CompletionEvent, CompletionRequest, CompletionStream, LlmProvider};
use crate::types::{Content, ContentBlock, Message, Role, StopReason, Tool};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// LLM provider for the `OpenAI` API and compatible endpoints.
#[derive(Debug, Clone)]
pub struct OpenAiProvider {
    http: reqwest::Client,
    api_key: Option<String>,
    base_url: String,
}

impl OpenAiProvider {
    /// Create a new provider instance.
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
            api_key: Some(api_key),
            base_url: DEFAULT_BASE_URL.to_string(),
        })
    }

    /// Create a provider with optional API key and base URL.
    ///
    /// Use this for OpenAI-compatible providers that may not require an API key
    /// (e.g., local Ollama) or use a different endpoint.
    ///
    /// # Errors
    ///
    /// Returns error if configuration is invalid.
    pub fn with_config(api_key: Option<String>, base_url: Option<String>) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::new(),
            api_key,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
        })
    }
}

// OpenAI request types

#[derive(Debug, Serialize)]
struct OpenAiRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiTool>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize)]
struct OpenAiMessage {
    role: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<OpenAiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCallRequest>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OpenAiContent {
    Text(String),
}

#[derive(Debug, Serialize)]
struct OpenAiToolCallRequest {
    id: String,
    #[serde(rename = "type")]
    call_type: &'static str,
    function: OpenAiFunctionCall,
}

#[derive(Debug, Serialize)]
struct OpenAiFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    tool_type: &'static str,
    function: OpenAiFunction,
}

#[derive(Debug, Serialize)]
struct OpenAiFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

// OpenAI response types for SSE parsing

#[derive(Debug, Deserialize)]
struct OpenAiChunk {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    delta: OpenAiDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct OpenAiDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiToolCallDelta {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenAiFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct OpenAiFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

/// Convert our messages to the format expected by the chat completions API.
fn convert_messages(messages: &[Message], system: Option<&str>) -> Vec<OpenAiMessage> {
    let mut result = Vec::new();

    // Add system message if present
    if let Some(sys) = system {
        result.push(OpenAiMessage {
            role: "system",
            content: Some(OpenAiContent::Text(sys.to_string())),
            tool_calls: None,
            tool_call_id: None,
        });
    }

    for msg in messages {
        match &msg.content {
            Content::Text(text) => {
                let role = match msg.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                };
                result.push(OpenAiMessage {
                    role,
                    content: Some(OpenAiContent::Text(text.clone())),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
            Content::Blocks(blocks) => {
                // Handle mixed content blocks
                let mut text_parts = Vec::new();
                let mut tool_calls = Vec::new();
                let mut tool_results = Vec::new();

                for block in blocks {
                    match block {
                        ContentBlock::Text { text } => {
                            text_parts.push(text.clone());
                        }
                        ContentBlock::ToolUse { id, name, input } => {
                            tool_calls.push(OpenAiToolCallRequest {
                                id: id.clone(),
                                call_type: "function",
                                function: OpenAiFunctionCall {
                                    name: name.clone(),
                                    arguments: serde_json::to_string(input).unwrap_or_default(),
                                },
                            });
                        }
                        ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            ..
                        } => {
                            tool_results.push((tool_use_id.clone(), content.clone()));
                        }
                    }
                }

                // Emit assistant message with tool calls if any
                if !tool_calls.is_empty() {
                    let content = if text_parts.is_empty() {
                        None
                    } else {
                        Some(OpenAiContent::Text(text_parts.join("")))
                    };
                    result.push(OpenAiMessage {
                        role: "assistant",
                        content,
                        tool_calls: Some(tool_calls),
                        tool_call_id: None,
                    });
                } else if !text_parts.is_empty() {
                    let role = match msg.role {
                        Role::User => "user",
                        Role::Assistant => "assistant",
                    };
                    result.push(OpenAiMessage {
                        role,
                        content: Some(OpenAiContent::Text(text_parts.join(""))),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }

                // Emit tool result messages
                for (tool_use_id, content) in tool_results {
                    result.push(OpenAiMessage {
                        role: "tool",
                        content: Some(OpenAiContent::Text(content)),
                        tool_calls: None,
                        tool_call_id: Some(tool_use_id),
                    });
                }
            }
        }
    }

    result
}

/// Convert our tools to the function calling format.
fn convert_tools(tools: &[Tool]) -> Vec<OpenAiTool> {
    tools
        .iter()
        .map(|t| OpenAiTool {
            tool_type: "function",
            function: OpenAiFunction {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.input_schema.clone(),
            },
        })
        .collect()
}

/// Parse a single SSE event from the buffer.
///
/// Returns the parsed chunk (if any) and the remaining buffer content.
fn parse_sse_event(buffer: &str) -> Option<(Option<OpenAiChunk>, String)> {
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

    // Handle [DONE] marker
    if data.trim() == "[DONE]" {
        return Some((None, remainder));
    }

    // Parse JSON
    match serde_json::from_str::<OpenAiChunk>(&data) {
        Ok(chunk) => Some((Some(chunk), remainder)),
        Err(e) => {
            tracing::debug!(data = %data, error = %e, "failed to parse OpenAI event");
            Some((None, remainder))
        }
    }
}

/// Convert finish reason to our stop reason.
fn convert_stop_reason(reason: &str) -> Option<StopReason> {
    match reason {
        "stop" => Some(StopReason::EndTurn),
        "tool_calls" => Some(StopReason::ToolUse),
        "length" => Some(StopReason::MaxTokens),
        _ => None,
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &'static str {
        "openai"
    }

    #[allow(clippy::too_many_lines)]
    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        // Add authorization header if API key is present
        if let Some(api_key) = &self.api_key {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {api_key}"))
                    .map_err(|_| AgentError::ApiKeyMissing)?,
            );
        }

        let openai_tools = request.tools.as_ref().map(|t| convert_tools(t));

        let openai_request = OpenAiRequest {
            model: request.model,
            max_tokens: request.max_tokens,
            messages: convert_messages(&request.messages, request.system.as_deref()),
            tools: openai_tools,
            stream: true,
            stream_options: Some(StreamOptions {
                include_usage: true,
            }),
        };

        let url = format!("{}/chat/completions", self.base_url);
        let response = self
            .http
            .post(&url)
            .headers(headers)
            .json(&openai_request)
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
            // Track tool calls being built: index -> (id, name, arguments)
            let mut pending_tool_calls: std::collections::HashMap<usize, (String, String, String)> =
                std::collections::HashMap::new();
            let text_block_index = 0_usize;
            let mut current_text = String::new();

            futures::pin_mut!(byte_stream);

            while let Some(chunk) = byte_stream.next().await {
                let chunk = chunk?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Process complete SSE events
                while let Some((chunk_opt, remainder)) = parse_sse_event(&buffer) {
                    buffer = remainder;

                    let Some(chunk) = chunk_opt else {
                        continue;
                    };

                    for choice in chunk.choices {
                        // Handle text content
                        if let Some(text) = choice.delta.content {
                            if !text.is_empty() {
                                current_text.push_str(&text);
                                yield Ok(CompletionEvent::TextDelta(text));
                            }
                        }

                        // Handle tool calls
                        if let Some(tool_calls) = choice.delta.tool_calls {
                            for tc in tool_calls {
                                let entry = pending_tool_calls.entry(tc.index).or_insert_with(|| {
                                    (String::new(), String::new(), String::new())
                                });

                                // Update id if present
                                if let Some(id) = tc.id {
                                    entry.0 = id;
                                }

                                // Update function info if present
                                if let Some(func) = tc.function {
                                    if let Some(name) = func.name {
                                        entry.1.clone_from(&name);
                                        // Emit tool use start when we get the name
                                        let tool_index = text_block_index + 1 + tc.index;
                                        yield Ok(CompletionEvent::ToolUseStart {
                                            index: tool_index,
                                            id: entry.0.clone(),
                                            name,
                                        });
                                    }
                                    if let Some(args) = func.arguments {
                                        entry.2.push_str(&args);
                                        let tool_index = text_block_index + 1 + tc.index;
                                        yield Ok(CompletionEvent::ToolInputDelta {
                                            index: tool_index,
                                            partial_json: args,
                                        });
                                    }
                                }
                            }
                        }

                        // Handle finish reason
                        if let Some(reason) = choice.finish_reason {
                            // Emit text block done if we have text
                            if !current_text.is_empty() {
                                yield Ok(CompletionEvent::ContentBlockDone {
                                    index: text_block_index,
                                    block: ContentBlock::Text { text: current_text.clone() },
                                });
                            }

                            // Emit tool blocks done
                            for (idx, (id, name, args)) in &pending_tool_calls {
                                let tool_index = text_block_index + 1 + idx;
                                let input = serde_json::from_str(args).unwrap_or(serde_json::Value::Null);
                                yield Ok(CompletionEvent::ContentBlockDone {
                                    index: tool_index,
                                    block: ContentBlock::ToolUse {
                                        id: id.clone(),
                                        name: name.clone(),
                                        input,
                                    },
                                });
                            }

                            let stop_reason = convert_stop_reason(&reason);
                            yield Ok(CompletionEvent::Done { stop_reason, usage: None });
                        }
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
    fn provider_requires_api_key() {
        let result = OpenAiProvider::new("");
        assert!(result.is_err());
    }

    #[test]
    fn provider_accepts_valid_key() {
        let result = OpenAiProvider::new("test-key");
        assert!(result.is_ok());
    }

    #[test]
    fn provider_name_is_openai() {
        let provider = OpenAiProvider::new("test-key").unwrap();
        assert_eq!(provider.name(), "openai");
    }

    #[test]
    fn with_config_uses_default_base_url() {
        let provider = OpenAiProvider::with_config(Some("key".to_string()), None).unwrap();
        assert_eq!(provider.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn with_config_custom_base_url() {
        let provider = OpenAiProvider::with_config(
            Some("key".to_string()),
            Some("http://localhost:8080/v1".to_string()),
        )
        .unwrap();
        assert_eq!(provider.base_url, "http://localhost:8080/v1");
    }

    #[test]
    fn with_config_no_api_key() {
        let provider = OpenAiProvider::with_config(None, None).unwrap();
        assert!(provider.api_key.is_none());
    }

    #[test]
    fn convert_tools_produces_function_type() {
        let tools = vec![Tool {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }];

        let openai_tools = convert_tools(&tools);
        assert_eq!(openai_tools.len(), 1);
        assert_eq!(openai_tools[0].tool_type, "function");
        assert_eq!(openai_tools[0].function.name, "test_tool");
    }

    #[test]
    fn convert_messages_adds_system() {
        let messages = vec![Message {
            role: Role::User,
            content: Content::Text("Hello".to_string()),
        }];

        let openai_messages = convert_messages(&messages, Some("You are helpful"));
        assert_eq!(openai_messages.len(), 2);
        assert_eq!(openai_messages[0].role, "system");
        assert_eq!(openai_messages[1].role, "user");
    }

    #[test]
    fn convert_stop_reason_maps_correctly() {
        assert_eq!(convert_stop_reason("stop"), Some(StopReason::EndTurn));
        assert_eq!(convert_stop_reason("tool_calls"), Some(StopReason::ToolUse));
        assert_eq!(convert_stop_reason("length"), Some(StopReason::MaxTokens));
        assert_eq!(convert_stop_reason("unknown"), None);
    }

    #[test]
    fn parse_sse_event_handles_done() {
        let buffer = "data: [DONE]\n\n";
        let result = parse_sse_event(buffer);
        assert!(result.is_some());
        let (chunk, remainder) = result.unwrap();
        assert!(chunk.is_none());
        assert!(remainder.is_empty());
    }
}
