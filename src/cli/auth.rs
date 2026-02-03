use crate::config::{AgentConfig, Config, ProviderConfig};
use dialoguer::{theme::ColorfulTheme, Password, Select};

use super::LoginArgs;

pub fn auth_login(args: LoginArgs) -> anyhow::Result<()> {
    let default_config = AgentConfig::default();
    let default_providers = &default_config.providers;
    let provider_names: Vec<&str> = default_providers.keys().map(String::as_str).collect();

    let provider = match args.provider {
        Some(p) => {
            if !default_providers.contains_key(&p) {
                anyhow::bail!(
                    "Unknown provider '{}'. Available providers: {}",
                    p,
                    provider_names.join(", ")
                );
            }
            p
        }
        None => prompt_provider_selection(&provider_names)?,
    };

    let api_key = match args.api_key {
        Some(key) => key,
        None => prompt_api_key(&provider)?,
    };

    let default_config = default_providers
        .get(&provider)
        .cloned()
        .unwrap_or_default();

    let provider_config = ProviderConfig {
        api_key: Some(api_key),
        ..default_config
    };

    Config::save_provider(&provider, &provider_config)?;

    println!("Successfully saved credentials for provider '{provider}'");

    Ok(())
}

fn prompt_provider_selection(providers: &[&str]) -> anyhow::Result<String> {
    let mut sorted_providers: Vec<&str> = providers.to_vec();
    sorted_providers.sort_unstable();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a provider")
        .items(&sorted_providers)
        .default(0)
        .interact()?;

    Ok(sorted_providers[selection].to_string())
}

fn prompt_api_key(provider: &str) -> anyhow::Result<String> {
    let api_key = Password::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Enter API key for {provider}"))
        .interact()?;

    if api_key.is_empty() {
        anyhow::bail!("API key cannot be empty");
    }

    Ok(api_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_login_validates_provider() {
        let args = LoginArgs {
            provider: Some("unknown_provider".to_string()),
            api_key: Some("sk-test".to_string()),
            skip_test: true,
        };

        let result = auth_login(args);

        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("Unknown provider"));
        assert!(err.to_string().contains("Available providers"));
    }

    #[test]
    fn test_auth_login_accepts_valid_provider() {
        let default_config = AgentConfig::default();
        let valid_providers: Vec<&str> = default_config
            .providers
            .keys()
            .map(String::as_str)
            .collect();

        for provider in valid_providers {
            assert!(
                default_config.providers.contains_key(provider),
                "Provider {provider} should be valid"
            );
        }
    }

    #[test]
    fn test_default_providers_include_common_ones() {
        let default_config = AgentConfig::default();

        assert!(default_config.providers.contains_key("anthropic"));
        assert!(default_config.providers.contains_key("openai"));
        assert!(default_config.providers.contains_key("ollama"));
        assert!(default_config.providers.contains_key("groq"));
    }
}
