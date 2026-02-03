use crate::config::{AgentConfig, Config};
use crate::core::keychain;
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

    keychain::store_api_key(&provider, &api_key)?;

    let provider_config = default_providers
        .get(&provider)
        .cloned()
        .unwrap_or_default();

    Config::save_provider(&provider, &provider_config)?;

    println!("Stored API key in system keychain");
    println!("Successfully configured provider '{provider}'");

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
    use crate::config::ProviderConfig;

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

    #[test]
    fn test_full_onboarding_flow() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let anthropic_config = ProviderConfig {
            api_type: crate::config::ProviderApiType::Anthropic,
            base_url: None,
            api_key_env: Some("ANTHROPIC_API_KEY".to_string()),
        };

        let result = Config::save_provider_to_path("anthropic", &anthropic_config, &config_path);
        assert!(result.is_ok(), "save_provider_to_path should succeed");

        assert!(
            config_path.exists(),
            "Config file should exist after auth login"
        );

        let contents = std::fs::read_to_string(&config_path).unwrap();
        let loaded: toml::Value = toml::from_str(&contents).unwrap();

        let providers = loaded
            .get("agent")
            .and_then(|a| a.get("providers"))
            .and_then(|p| p.as_table())
            .expect("Should have agent.providers table");

        assert!(
            providers.contains_key("anthropic"),
            "Should contain anthropic provider"
        );

        let config_str = std::fs::read_to_string(&config_path).unwrap();
        let config: Config = toml::from_str(&config_str).unwrap();
        assert!(
            config.agent.providers.contains_key("anthropic"),
            "Loaded config should have anthropic provider"
        );
    }

    #[test]
    fn test_ollama_no_key_required() {
        let default_config = AgentConfig::default();

        assert!(
            default_config.providers.contains_key("ollama"),
            "Ollama should be in default providers"
        );

        let ollama_config = default_config
            .providers
            .get("ollama")
            .expect("Ollama provider should exist");

        assert!(
            ollama_config.api_key_env.is_none(),
            "Ollama should not require api_key_env"
        );

        assert!(
            ollama_config.base_url.is_some(),
            "Ollama should have base_url configured"
        );
        assert_eq!(
            ollama_config.base_url.as_ref().unwrap(),
            "http://localhost:11434/v1"
        );
    }

    #[test]
    fn test_provider_switching() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let anthropic_config = ProviderConfig {
            api_type: crate::config::ProviderApiType::Anthropic,
            base_url: None,
            api_key_env: Some("ANTHROPIC_API_KEY".to_string()),
        };

        Config::save_provider_to_path("anthropic", &anthropic_config, &config_path).unwrap();

        let openai_config = ProviderConfig {
            api_type: crate::config::ProviderApiType::OpenAi,
            base_url: Some("https://api.openai.com/v1".to_string()),
            api_key_env: Some("OPENAI_API_KEY".to_string()),
        };

        Config::save_provider_to_path("openai", &openai_config, &config_path).unwrap();

        let contents = std::fs::read_to_string(&config_path).unwrap();
        let loaded: toml::Value = toml::from_str(&contents).unwrap();

        let providers = loaded
            .get("agent")
            .and_then(|a| a.get("providers"))
            .and_then(|p| p.as_table())
            .expect("Should have agent.providers table");

        assert!(
            providers.contains_key("anthropic"),
            "Should contain anthropic provider"
        );
        assert!(
            providers.contains_key("openai"),
            "Should contain openai provider"
        );

        let openai_entry = providers
            .get("openai")
            .and_then(|a| a.as_table())
            .expect("openai should be a table");

        assert_eq!(
            openai_entry.get("base_url").and_then(|k| k.as_str()),
            Some("https://api.openai.com/v1")
        );
    }

    #[test]
    fn test_unknown_provider_rejected() {
        let args = LoginArgs {
            provider: Some("nonexistent_provider".to_string()),
            api_key: Some("sk-test".to_string()),
            skip_test: true,
        };

        let result = auth_login(args);

        assert!(result.is_err(), "Should reject unknown provider");
        let err = result.err().unwrap();
        assert!(
            err.to_string().contains("Unknown provider"),
            "Error should mention unknown provider"
        );
    }
}
