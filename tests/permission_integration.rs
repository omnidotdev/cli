//! Integration tests for the permission system.

use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::timeout;

use omni_cli::core::agent::{
    InterfaceMessage, PermissionAction, PermissionActor, PermissionClient, PermissionContext,
    PermissionMessage, PermissionResponse,
};

#[tokio::test]
async fn permission_request_flows_to_interface() {
    let (actor, permission_tx) = PermissionActor::new();
    let (interface_tx, mut interface_rx) = mpsc::unbounded_channel();

    // Register interface.
    permission_tx
        .send(PermissionMessage::RegisterInterface { interface_tx })
        .unwrap();

    // Spawn actor.
    tokio::spawn(actor.run());

    // Create client and send request.
    let client = PermissionClient::new("test-session".to_string(), permission_tx);

    // Spawn the request so it runs concurrently.
    let _request_handle = tokio::spawn(async move {
        client
            .request(
                "shell",
                PermissionAction::Execute,
                PermissionContext::Bash {
                    command: "rm -rf /tmp/test".to_string(),
                    working_dir: PathBuf::from("/tmp"),
                },
            )
            .await
    });

    // Should receive interface message.
    let msg = timeout(Duration::from_millis(100), interface_rx.recv())
        .await
        .expect("timeout waiting for interface message")
        .expect("channel closed");

    assert!(matches!(
        msg,
        InterfaceMessage::ShowPermissionDialog {
            tool_name,
            action: PermissionAction::Execute,
            ..
        } if tool_name == "shell"
    ));
}

#[tokio::test]
async fn session_cache_skips_second_request() {
    let (mut actor, permission_tx) = PermissionActor::new();
    let (interface_tx, mut interface_rx) = mpsc::unbounded_channel();

    // Manually process messages to control flow.
    permission_tx
        .send(PermissionMessage::RegisterInterface { interface_tx })
        .unwrap();

    // Process registration.
    if let Some(msg) = actor.inbox.recv().await {
        actor.handle_message(msg);
    }

    let client = PermissionClient::new("test-session".to_string(), permission_tx.clone());

    // First request.
    let client_clone = client.clone();
    let first_request = tokio::spawn(async move {
        client_clone
            .request(
                "shell",
                PermissionAction::Execute,
                PermissionContext::Bash {
                    command: "echo hello".to_string(),
                    working_dir: PathBuf::from("/tmp"),
                },
            )
            .await
    });

    // Process request.
    if let Some(msg) = actor.inbox.recv().await {
        actor.handle_message(msg);
    }

    // Get interface message.
    let msg = interface_rx.recv().await.unwrap();
    let request_id = match msg {
        InterfaceMessage::ShowPermissionDialog { request_id, .. } => request_id,
        _ => panic!("unexpected message"),
    };

    // Respond with AllowForSession.
    actor.respond(
        request_id,
        PermissionResponse::AllowForSession,
        "test-session",
        "shell",
        &PermissionAction::Execute,
    );

    // First request should complete.
    let result = first_request.await.unwrap();
    assert!(result.unwrap());

    // Second request should hit cache (no interface message).
    let client_clone = client.clone();
    let second_request = tokio::spawn(async move {
        client_clone
            .request(
                "shell",
                PermissionAction::Execute,
                PermissionContext::Bash {
                    command: "echo world".to_string(),
                    working_dir: PathBuf::from("/tmp"),
                },
            )
            .await
    });

    // Process second request.
    if let Some(msg) = actor.inbox.recv().await {
        actor.handle_message(msg);
    }

    // Should complete immediately without interface message.
    let result = timeout(Duration::from_millis(100), second_request)
        .await
        .expect("second request should complete quickly")
        .unwrap();
    assert!(result.unwrap());

    // Interface should not have received another message.
    assert!(interface_rx.try_recv().is_err());
}
