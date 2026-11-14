//! Authentication commands: connect the CLI to cloud Synapse with an API key.
//!
//! The Synapse gateway authenticates every request with a `synapse_` API key,
//! which also decides who is billed (the key's workspace). `omni auth login`
//! stores such a key on the `synapse` provider; it never asks for a password.

use std::io::{BufRead, IsTerminal};
use std::time::Duration;

use anyhow::{Context, Result};

use crate::config::{Config, normalize_synapse_key};

/// Where users create a Synapse API key
pub const SYNAPSE_KEYS_URL: &str = "https://synapse.omni.dev/dashboard/keys";

/// Outcome of checking a key against the gateway
#[derive(Debug, PartialEq, Eq)]
enum KeyCheck {
    /// The gateway accepted the key
    Valid,
    /// The gateway rejected the key (401/403)
    Rejected,
    /// The gateway answered with another status
    Unexpected(u16),
}

/// Classify the gateway's answer to an authenticated model listing
const fn classify_status(status: u16) -> KeyCheck {
    match status {
        200..=299 => KeyCheck::Valid,
        401 | 403 => KeyCheck::Rejected,
        other => KeyCheck::Unexpected(other),
    }
}

/// Ask the gateway to list models with the key (cheap, authenticated, no spend)
async fn check_key(gateway_url: &str, key: &str) -> Result<KeyCheck> {
    let url = format!("{}/v1/models", gateway_url.trim_end_matches('/'));
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?
        .get(&url)
        .bearer_auth(key)
        .send()
        .await
        .with_context(|| format!("could not reach the Synapse gateway at {gateway_url}"))?;
    Ok(classify_status(response.status().as_u16()))
}

/// Read the key from a hidden prompt, or from stdin when it is not a terminal
/// (so `pbpaste | omni auth login` works). Never from argv, which would leak
/// the key into shell history and process listings
fn read_key() -> Result<String> {
    if std::io::stdin().is_terminal() {
        println!("Create a Synapse API key at {SYNAPSE_KEYS_URL}");
        rpassword::prompt_password("Synapse API key: ").context("failed to read the API key")
    } else {
        let mut line = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut line)
            .context("failed to read the API key from stdin")?;
        Ok(line)
    }
}

/// Connect the CLI to cloud Synapse with a Synapse API key.
///
/// Validates the key against the gateway, stores it on the `synapse`
/// provider in `~/.config/omni/cli/config.toml` (owner-only permissions), and
/// makes Synapse the active provider. Usage is billed to the key's workspace.
///
/// # Errors
///
/// Returns an error if the key is malformed, rejected by the gateway, or the
/// config cannot be written.
pub async fn login() -> Result<()> {
    let key = normalize_synapse_key(&read_key()?)?;

    let mut config = Config::load_global()?;
    let gateway_url = config.synapse_gateway_url();

    match check_key(&gateway_url, &key).await? {
        KeyCheck::Valid => {}
        KeyCheck::Rejected => anyhow::bail!(
            "the Synapse gateway rejected this key; create a new one at {SYNAPSE_KEYS_URL}"
        ),
        KeyCheck::Unexpected(status) => {
            anyhow::bail!("the Synapse gateway answered {status}; try again shortly")
        }
    }

    config.set_synapse_key(key);
    save_private(&config)?;

    println!(
        "Logged in to Synapse. The CLI now uses Synapse (model {}); usage is billed to the key's workspace",
        config.agent.model
    );
    if std::env::var_os("SYNAPSE_API_KEY").is_some() {
        println!("Note: SYNAPSE_API_KEY is set in your environment and takes precedence");
    }
    Ok(())
}

/// Remove the stored Synapse API key.
///
/// # Errors
///
/// Returns an error if the config cannot be read or written.
pub fn logout() -> Result<()> {
    let mut config = Config::load_global()?;
    config.clear_synapse_key();
    save_private(&config)?;
    println!("Logged out of Synapse");
    Ok(())
}

/// Save the global config, readable only by its owner since it holds a key
fn save_private(config: &Config) -> Result<()> {
    config.save().context("failed to save config")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = Config::config_path()?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .context("failed to restrict config file permissions")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_status_classification() {
        assert_eq!(classify_status(200), KeyCheck::Valid);
        assert_eq!(classify_status(401), KeyCheck::Rejected);
        assert_eq!(classify_status(403), KeyCheck::Rejected);
        assert_eq!(classify_status(503), KeyCheck::Unexpected(503));
    }

    #[test]
    fn keys_url_points_at_the_synapse_app() {
        assert_eq!(SYNAPSE_KEYS_URL, "https://synapse.omni.dev/dashboard/keys");
    }
}
