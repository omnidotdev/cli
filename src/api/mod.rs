//! HTTP API server for the Omni CLI.
//!
//! TODO: Replace utoipa-generated spec with ODK-generated spec from `~/projects/omni/odk`
//! once CLI API schema is defined in ODK. See: <https://github.com/omnidotdev/odk>

// Allow clippy lint triggered by utoipa's OpenApi derive macro
#![allow(clippy::needless_for_each)]

use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::{get, post},
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use axum::extract::Path;

use crate::config::Config;
use crate::core::session::{ExportedSession, SessionManager, ShareOptions};
use crate::core::{Agent, TaskResult};

/// Shared application state.
pub struct AppState {
    /// The agent (if configured).
    pub agent: Option<Agent>,

    /// History of executed tasks.
    pub history: Vec<TaskResult>,

    /// API token for authentication (if configured).
    pub token: Option<String>,
}

impl AppState {
    fn new() -> Self {
        let config = Config::load().unwrap_or_default();
        let agent = config.agent.create_provider().ok().map(|provider| {
            Agent::with_context(provider, &config.agent.model, config.agent.max_tokens, None)
        });

        Self {
            agent,
            history: Vec::new(),
            token: config.api.token(),
        }
    }
}

type SharedState = Arc<RwLock<AppState>>;

/// `OpenAPI` documentation.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Omni CLI API",
        description = "HTTP API for the Omni CLI agentic interface",
        version = "0.1.0",
        license(name = "MIT")
    ),
    paths(health, execute_agent, get_history),
    components(schemas(AgentRequest, AgentResponse, TaskResult))
)]
struct ApiDoc;

/// Authentication middleware.
///
/// Validates the `Authorization: Bearer <token>` header if a token is configured.
async fn auth_middleware(
    State(state): State<SharedState>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let state = state.read().await;

    // If no token configured, allow all requests (localhost-only mode)
    let Some(ref expected_token) = state.token else {
        drop(state);
        return next.run(request).await;
    };

    // Check Authorization header
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match auth_header {
        Some(token) if token == expected_token => {
            drop(state);
            next.run(request).await
        }
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "unauthorized",
                "message": "Missing or invalid Authorization header. Use: Bearer <token>"
            })),
        )
            .into_response(),
    }
}

/// Start the HTTP API server.
///
/// # Errors
///
/// Returns an error if the server fails to bind or start.
pub async fn serve(host: &str, port: u16) -> anyhow::Result<()> {
    let state: SharedState = Arc::new(RwLock::new(AppState::new()));

    // Check if auth is enabled
    let auth_enabled = state.read().await.token.is_some();

    // Protected routes (require auth if token configured)
    let protected_routes = Router::new()
        .route("/api/agent", post(execute_agent))
        .route("/api/agent/stream", post(execute_agent_stream))
        .route("/api/history", get(get_history))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/health", get(health))
        .route("/api/share/:token", get(get_shared_session))
        .merge(SwaggerUi::new("/api/docs").url("/api/openapi.json", ApiDoc::openapi()));

    // Share routes (some require auth)
    let share_routes = Router::new()
        .route("/api/share", post(create_share))
        .route("/api/share/:token", axum::routing::delete(delete_share))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let app = Router::new()
        .merge(protected_routes)
        .merge(share_routes)
        .merge(public_routes)
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    if auth_enabled {
        tracing::info!(addr = %addr, "starting HTTP API server (auth enabled)");
    } else {
        tracing::warn!(addr = %addr, "starting HTTP API server (NO AUTH - localhost only recommended)");
    }

    axum::serve(listener, app).await?;

    Ok(())
}

/// Health check endpoint.
#[utoipa::path(
    get,
    path = "/health",
    responses((status = 200, description = "Service healthy", body = String))
)]
async fn health() -> &'static str {
    "ok"
}

/// Request body for agent execution.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AgentRequest {
    /// The prompt or task to execute.
    pub prompt: String,
}

/// Request body for creating a share.
#[derive(Debug, Deserialize)]
pub struct CreateShareRequest {
    /// Session ID to share.
    pub session_id: String,
    /// TTL in seconds (optional).
    pub ttl_seconds: Option<u64>,
}

/// Response body for share creation.
#[derive(Debug, Serialize)]
pub struct CreateShareResponse {
    /// Share token for the URL.
    pub token: String,
    /// Secret for modification/deletion.
    pub secret: String,
    /// Full share URL.
    pub url: String,
}

/// Request body for deleting a share.
#[derive(Debug, Deserialize)]
pub struct DeleteShareRequest {
    /// Secret for authorization.
    pub secret: String,
}

/// Response body for agent execution.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AgentResponse {
    /// Whether the task succeeded.
    pub success: bool,
    /// The task output.
    pub output: String,
}

/// SSE event for streaming responses.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    /// Text chunk.
    #[serde(rename = "text")]
    Text { content: String },
    /// Stream completed.
    #[serde(rename = "done")]
    Done,
    /// Error occurred.
    #[serde(rename = "error")]
    Error { message: String },
}

/// Execute an agentic task.
#[utoipa::path(
    post,
    path = "/api/agent",
    request_body = AgentRequest,
    responses(
        (status = 200, description = "Task executed", body = AgentResponse),
        (status = 503, description = "No API key configured")
    )
)]
async fn execute_agent(
    State(state): State<SharedState>,
    Json(req): Json<AgentRequest>,
) -> Result<Json<AgentResponse>, (StatusCode, String)> {
    let mut state = state.write().await;

    let Some(ref mut agent) = state.agent else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "No API key configured".to_string(),
        ));
    };

    let mut output = String::new();

    let result = agent
        .chat(&req.prompt, |text| {
            output.push_str(text);
        })
        .await;

    match result {
        Ok(_) => {
            let task_result = TaskResult {
                success: true,
                output: output.clone(),
            };
            state.history.push(task_result);

            Ok(Json(AgentResponse {
                success: true,
                output,
            }))
        }
        Err(e) => {
            let error_output = e.to_string();
            let task_result = TaskResult {
                success: false,
                output: error_output.clone(),
            };
            state.history.push(task_result);

            Ok(Json(AgentResponse {
                success: false,
                output: error_output,
            }))
        }
    }
}

/// Get task execution history.
#[utoipa::path(
    get,
    path = "/api/history",
    responses((status = 200, description = "Task history", body = Vec<TaskResult>))
)]
async fn get_history(State(state): State<SharedState>) -> Json<Vec<TaskResult>> {
    let state = state.read().await;
    Json(state.history.clone())
}

/// Execute an agentic task with SSE streaming.
async fn execute_agent_stream(
    State(state): State<SharedState>,
    Json(req): Json<AgentRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    let mut state_guard = state.write().await;

    let Some(agent) = state_guard.agent.take() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "No API key configured".to_string(),
        ));
    };

    drop(state_guard);

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let state_clone = state.clone();

    tokio::spawn(async move {
        let mut agent = agent;

        let result = agent
            .chat(&req.prompt, |text| {
                let event = StreamEvent::Text {
                    content: text.to_string(),
                };
                let _ = tx.send(event);
            })
            .await;

        match result {
            Ok(_) => {
                let _ = tx.send(StreamEvent::Done);
            }
            Err(e) => {
                let _ = tx.send(StreamEvent::Error {
                    message: e.to_string(),
                });
            }
        }

        // Return agent to state
        let mut state_guard = state_clone.write().await;
        state_guard.agent = Some(agent);
    });

    let stream = UnboundedReceiverStream::new(rx).map(|event| {
        let data = serde_json::to_string(&event).unwrap_or_default();
        Ok(Event::default().data(data))
    });

    Ok(Sse::new(stream))
}

/// Create a share token for a session.
async fn create_share(
    headers: HeaderMap,
    Json(req): Json<CreateShareRequest>,
) -> Result<Json<CreateShareResponse>, (StatusCode, String)> {
    let manager = SessionManager::for_current_project()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let options = ShareOptions {
        ttl_seconds: req.ttl_seconds,
    };

    let share = manager
        .create_share(&req.session_id, options)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    // Build URL from request headers
    let host = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:3000");
    let protocol = if host.starts_with("localhost") || host.starts_with("127.0.0.1") {
        "http"
    } else {
        "https"
    };

    Ok(Json(CreateShareResponse {
        token: share.token.clone(),
        secret: share.secret,
        url: format!("{protocol}://{host}/api/share/{}", share.token),
    }))
}

/// Get a shared session (public, no auth required).
async fn get_shared_session(
    Path(token): Path<String>,
) -> Result<Json<ExportedSession>, (StatusCode, String)> {
    let manager = SessionManager::for_current_project()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let exported = manager
        .get_shared_session(&token)
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    Ok(Json(exported))
}

/// Delete a share token.
async fn delete_share(
    Path(token): Path<String>,
    Json(req): Json<DeleteShareRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let manager = SessionManager::for_current_project()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    manager
        .revoke_share(&token, &req.secret)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    fn create_test_state(token: Option<String>) -> SharedState {
        Arc::new(RwLock::new(AppState {
            agent: None,
            history: Vec::new(),
            token,
        }))
    }

    fn create_test_router(state: SharedState) -> Router {
        let protected_routes = Router::new()
            .route("/api/agent", post(execute_agent))
            .route("/api/history", get(get_history))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            ));

        let public_routes = Router::new().route("/health", get(health));

        Router::new()
            .merge(protected_routes)
            .merge(public_routes)
            .with_state(state)
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let state = create_test_state(None);
        let app = create_test_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn auth_middleware_allows_request_when_no_token_configured() {
        let state = create_test_state(None);
        let app = create_test_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/history")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn auth_middleware_rejects_request_without_token() {
        let state = create_test_state(Some("secret-token".to_string()));
        let app = create_test_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/history")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn auth_middleware_rejects_invalid_token() {
        let state = create_test_state(Some("secret-token".to_string()));
        let app = create_test_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/history")
                    .header("Authorization", "Bearer wrong-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn auth_middleware_allows_valid_token() {
        let state = create_test_state(Some("secret-token".to_string()));
        let app = create_test_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/history")
                    .header("Authorization", "Bearer secret-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn history_returns_empty_list_initially() {
        let state = create_test_state(None);
        let app = create_test_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/history")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let history: Vec<TaskResult> = serde_json::from_slice(&body).unwrap();
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn agent_request_returns_503_when_no_agent() {
        let state = create_test_state(None);
        let app = create_test_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/agent")
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{"prompt": "test"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn stream_event_serializes_correctly() {
        let text_event = StreamEvent::Text {
            content: "hello".to_string(),
        };
        let json = serde_json::to_string(&text_event).unwrap();
        assert!(json.contains(r#""type":"text""#));
        assert!(json.contains(r#""content":"hello""#));

        let done_event = StreamEvent::Done;
        let json = serde_json::to_string(&done_event).unwrap();
        assert!(json.contains(r#""type":"done""#));

        let error_event = StreamEvent::Error {
            message: "oops".to_string(),
        };
        let json = serde_json::to_string(&error_event).unwrap();
        assert!(json.contains(r#""type":"error""#));
        assert!(json.contains(r#""message":"oops""#));
    }

    #[test]
    fn agent_request_deserializes_correctly() {
        let json = r#"{"prompt": "do something"}"#;
        let req: AgentRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.prompt, "do something");
    }

    #[test]
    fn agent_response_serializes_correctly() {
        let response = AgentResponse {
            success: true,
            output: "done".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains(r#""success":true"#));
        assert!(json.contains(r#""output":"done""#));
    }
}
