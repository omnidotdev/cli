//! Integration tests for CLI → Synapse connectivity.
//!
//! These tests use a local mock server to simulate Synapse responses
//! without requiring a live gateway.

use std::net::SocketAddr;
use std::time::Duration;

use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use tokio::net::TcpListener;
use tokio::time::timeout;

/// Start a mock Synapse server that responds to /health and /v1/models
async fn start_mock_synapse() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route(
            "/v1/models",
            get(|| async {
                Json(json!({
                    "object": "list",
                    "data": [
                        {
                            "id": "claude-sonnet-4-20250514",
                            "object": "model",
                            "created": 1_700_000_000_u64,
                            "owned_by": "anthropic"
                        },
                        {
                            "id": "gpt-4o",
                            "object": "model",
                            "created": 1_700_000_000_u64,
                            "owned_by": "openai"
                        }
                    ]
                }))
            }),
        );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    (addr, handle)
}

#[tokio::test]
async fn synapse_client_connects_to_mock_server() {
    let (addr, _handle) = start_mock_synapse().await;
    let base_url = format!("http://{addr}");

    let client = synapse_client::SynapseClient::new(&base_url).unwrap();
    let models = client.list_models().await.unwrap();

    assert_eq!(models.len(), 2);
    assert!(models.iter().any(|m| m.id == "claude-sonnet-4-20250514"));
    assert!(models.iter().any(|m| m.id == "gpt-4o"));
}

#[tokio::test]
async fn synapse_client_reports_correct_model_metadata() {
    let (addr, _handle) = start_mock_synapse().await;
    let base_url = format!("http://{addr}");

    let client = synapse_client::SynapseClient::new(&base_url).unwrap();
    let models = client.list_models().await.unwrap();

    let claude = models
        .iter()
        .find(|m| m.id == "claude-sonnet-4-20250514")
        .unwrap();
    assert_eq!(claude.owned_by, "anthropic");
    assert_eq!(claude.object, "model");
}

#[tokio::test]
async fn synapse_client_invalid_url_returns_error() {
    let result = synapse_client::SynapseClient::new("not-a-url");
    assert!(result.is_err());
}

#[tokio::test]
async fn synapse_client_unreachable_server_returns_error() {
    // Use a port that is almost certainly not listening
    let client = synapse_client::SynapseClient::new("http://127.0.0.1:1").unwrap();
    let result = client.list_models().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn config_discovers_models_from_mock_synapse() {
    let (addr, _handle) = start_mock_synapse().await;

    let mut config = omni_cli::config::AgentConfig::default();
    config.providers.insert(
        "synapse".to_string(),
        omni_cli::config::ProviderConfig {
            api_type: agent_core::registry::ProviderApiType::Synapse,
            base_url: Some(format!("http://{addr}")),
            api_key_env: None,
            api_key: None,
        },
    );

    let discovered = config.discover_synapse_models().await;
    assert!(discovered, "should discover models from mock synapse");

    // Models from mock server should be merged into config
    assert!(
        config
            .models
            .iter()
            .any(|m| m.id == "claude-sonnet-4-20250514")
    );
    assert!(config.models.iter().any(|m| m.id == "gpt-4o"));
}

#[tokio::test]
async fn config_fallback_when_synapse_unreachable() {
    let mut config = omni_cli::config::AgentConfig {
        provider: "synapse".to_string(),
        ..Default::default()
    };

    // Point at an unreachable address
    config.providers.insert(
        "synapse".to_string(),
        omni_cli::config::ProviderConfig {
            api_type: agent_core::registry::ProviderApiType::Synapse,
            base_url: Some("http://127.0.0.1:1".to_string()),
            api_key_env: None,
            api_key: None,
        },
    );

    // create_provider_with_fallback should fall back to anthropic
    let result = timeout(
        Duration::from_secs(10),
        config.create_provider_with_fallback(),
    )
    .await
    .expect("should not timeout");

    // Provider should still be created (anthropic fallback)
    // It may fail if ANTHROPIC_API_KEY is not set, but the provider
    // field should have changed to anthropic
    assert_eq!(
        config.provider, "anthropic",
        "should fall back to anthropic"
    );
    // The result depends on whether an API key is available, but
    // the fallback itself should have occurred
    drop(result);
}

#[tokio::test]
async fn config_does_not_fallback_when_synapse_reachable() {
    let (addr, _handle) = start_mock_synapse().await;

    let mut config = omni_cli::config::AgentConfig {
        provider: "synapse".to_string(),
        ..Default::default()
    };

    config.providers.insert(
        "synapse".to_string(),
        omni_cli::config::ProviderConfig {
            api_type: agent_core::registry::ProviderApiType::Synapse,
            base_url: Some(format!("http://{addr}")),
            api_key_env: None,
            api_key: None,
        },
    );

    let _result = config.create_provider_with_fallback().await;

    // Provider should remain synapse since it was reachable
    assert_eq!(config.provider, "synapse", "should stay on synapse");
}

#[tokio::test]
async fn cli_parses_synapse_status_command() {
    use clap::Parser;
    use omni_cli::cli::{Cli, Commands, SynapseCommands};

    let cli = Cli::parse_from(["omni", "synapse", "status"]);
    match cli.command {
        Some(Commands::Synapse { command }) => {
            assert!(matches!(command, SynapseCommands::Status));
        }
        _ => panic!("expected Synapse command"),
    }
}
