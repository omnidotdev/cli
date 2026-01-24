//! Omni CLI - Agentic CLI for the Omni ecosystem.
//!
//! This library provides the core functionality for the Omni CLI, including:
//! - CLI command parsing and execution
//! - Terminal user interface (TUI)
//! - HTTP API for remote access
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐  ┌─────────────┐  ┌─────────────┐
//! │     CLI     │  │     TUI     │  │   HTTP API  │
//! └──────┬──────┘  └──────┬──────┘  └──────┬──────┘
//!        │                │                │
//!        └────────────────┼────────────────┘
//!                         │
//!                  ┌──────┴──────┐
//!                  │    Core     │
//!                  └─────────────┘
//! ```

pub mod api;
pub mod build_info;
pub mod cli;
pub mod config;
pub mod core;
pub mod tui;

pub use config::Config;
pub use core::agent::{
    Agent, PermissionAction, PermissionActor, PermissionClient, PermissionContext, PermissionError,
    PermissionMessage, PermissionResponse,
};
