use std::collections::HashMap;
use std::time::Duration;

use crate::config::{AgentConfig, ModelInfo};
use crate::core::keychain;

pub async fn fetch_provider_models(config: &AgentConfig) -> Vec<(String, Vec<ModelInfo>)> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    let mut results: HashMap<String, Vec<ModelInfo>> = HashMap::new();

    for (provider_name, provider_config) in &config.providers {
        let env_key = provider_config
            .api_key_env
            .as_ref()
            .and_then(|env| std::env::var(env).ok());
        let has_env_key = env_key.is_some();
        let keychain_key = if has_env_key {
            None
        } else {
            keychain::get_api_key(provider_name)
        };
        let has_keychain_key = keychain_key.is_some();
        let is_local = provider_config.base_url.is_some() && provider_config.api_key_env.is_none();

        if !has_env_key && !has_keychain_key && !is_local {
            continue;
        }

        let api_key = env_key.or(keychain_key);

        if let Some(base_url) = &provider_config.base_url {
            if let Ok(models) =
                fetch_openai_compatible_models(&client, base_url, api_key.as_deref(), provider_name)
                    .await
            {
                if !models.is_empty() {
                    results.insert(provider_name.clone(), models);
                    continue;
                }
            }
        }

        let curated: Vec<ModelInfo> = config
            .models
            .iter()
            .filter(|m| &m.provider == provider_name)
            .cloned()
            .collect();

        if !curated.is_empty() {
            results.insert(provider_name.clone(), curated);
        }
    }

    let mut sorted: Vec<(String, Vec<ModelInfo>)> = results.into_iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    sorted
}

async fn fetch_openai_compatible_models(
    client: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
    provider_name: &str,
) -> Result<Vec<ModelInfo>, ()> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));

    let mut request = client.get(&url);
    if let Some(key) = api_key {
        request = request.header("Authorization", format!("Bearer {key}"));
    }

    let response = request.send().await.map_err(|_| ())?;

    if !response.status().is_success() {
        return Err(());
    }

    let json: serde_json::Value = response.json().await.map_err(|_| ())?;

    let models = json
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    m.get("id").and_then(|id| id.as_str()).map(|id| ModelInfo {
                        id: id.to_string(),
                        provider: provider_name.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_openrouter_models() {
        let api_key = std::env::var("OPENROUTER_API_KEY").ok();
        if api_key.is_none() {
            println!("OPENROUTER_API_KEY not set, skipping");
            return;
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();

        let result = fetch_openai_compatible_models(
            &client,
            "https://openrouter.ai/api/v1",
            api_key.as_deref(),
            "openrouter",
        )
        .await;

        match result {
            Ok(models) => {
                println!("Fetched {} models", models.len());
                for m in models.iter().take(5) {
                    println!("  - {}", m.id);
                }
                assert!(!models.is_empty());
            }
            Err(()) => {
                panic!("Failed to fetch models");
            }
        }
    }
}
