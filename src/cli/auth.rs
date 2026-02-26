//! Authentication commands — login and logout for cloud Synapse.

use anyhow::{Context, Result};

/// Default Gatekeeper URL (HIDRA Gatekeeper, Better Auth).
const DEFAULT_AUTH_URL: &str = "https://auth.omni.dev";

/// Authenticate with the cloud Synapse using email and password.
///
/// Stores the resulting access token in `~/.config/omni/cli/config.toml`.
///
/// # Errors
///
/// Returns an error if the login request fails or the response cannot be parsed.
pub async fn login() -> Result<()> {
    let auth_url = std::env::var("OMNI_AUTH_URL").unwrap_or_else(|_| DEFAULT_AUTH_URL.to_string());

    let email = {
        print!("Email: ");
        std::io::Write::flush(&mut std::io::stdout())?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        input.trim().to_string()
    };

    let password = rpassword::prompt_password("Password: ").context("failed to read password")?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{auth_url}/api/auth/sign-in/email"))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .context("login request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("login failed ({status}): {body}");
    }

    let data: serde_json::Value = resp.json().await.context("invalid response")?;

    let token = data["token"]
        .as_str()
        .context("no token in response — check OMNI_AUTH_URL")?;

    let mut config = crate::config::Config::load()?;
    config.auth.access_token = Some(token.to_string());
    config.save().context("failed to save config")?;

    println!("Logged in as {email}");
    Ok(())
}

/// Clear the stored access token, logging out from cloud Synapse.
///
/// # Errors
///
/// Returns an error if the config cannot be read or written.
pub fn logout() -> Result<()> {
    let mut config = crate::config::Config::load()?;
    config.auth.access_token = None;
    config.save().context("failed to save config")?;
    println!("Logged out");
    Ok(())
}
