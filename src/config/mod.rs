//! Configuration management for the Omni CLI.

mod persona;

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use agent_core::registry::{self, ProviderRegistry, resolve_api_key};
use crate::core::agent::{AgentMode, LlmProvider};
use synapse_client::SynapseClient;

pub use agent_core::permission::{AgentPermissions, PermissionPreset};
pub use agent_core::registry::{ModelInfo, ProviderApiType, ProviderConfig};
pub use persona::{Persona, list_personas, load_persona, personas_dir};

/// Individual agent definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    /// Description shown in agent switcher.
    #[serde(default)]
    pub description: String,

    /// Model override (uses default if not set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Permission defaults for this agent.
    #[serde(default)]
    pub permissions: AgentPermissions,
}

/// Application configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// API configuration.
    pub api: ApiConfig,

    /// TUI configuration.
    pub tui: TuiConfig,

    /// Agent configuration.
    pub agent: AgentConfig,
}

impl Config {
    /// Load configuration from the default path.
    ///
    /// Loads global config first, then merges project-local config if present.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration file cannot be read or parsed.
    pub fn load() -> anyhow::Result<Self> {
        // Load global config
        let global_path = Self::config_path()?;
        let mut config = if global_path.exists() {
            let contents = std::fs::read_to_string(&global_path)?;
            toml::from_str(&contents)?
        } else {
            Self::default()
        };

        // Merge project-local config if it exists
        if let Ok(project_path) = Self::project_config_path() {
            if project_path.exists() {
                let contents = std::fs::read_to_string(&project_path)?;
                let project_config: Self = toml::from_str(&contents)?;
                config.merge(project_config);
            }
        }

        Ok(config)
    }

    /// Get the project-local configuration file path.
    ///
    /// Looks for `.omni/config.toml` in the current directory.
    pub fn project_config_path() -> anyhow::Result<PathBuf> {
        let cwd = std::env::current_dir()?;
        Ok(cwd.join(".omni").join("config.toml"))
    }

    /// Merge another config into this one (project overrides global).
    fn merge(&mut self, other: Self) {
        // Agent model override
        if other.agent.model != AgentConfig::default().model {
            self.agent.model = other.agent.model;
        }
        if other.agent.max_tokens != AgentConfig::default().max_tokens {
            self.agent.max_tokens = other.agent.max_tokens;
        }

        // API config overrides
        if other.api.port != ApiConfig::default().port {
            self.api.port = other.api.port;
        }
        if other.api.host != ApiConfig::default().host {
            self.api.host = other.api.host;
        }
    }

    /// Get the configuration file path.
    ///
    /// # Errors
    ///
    /// Returns an error if the config directory cannot be determined.
    pub fn config_path() -> anyhow::Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    /// Get the config directory path (`~/.config/omni/cli/`).
    ///
    /// # Errors
    ///
    /// Returns an error if the config directory cannot be determined.
    pub fn config_dir() -> anyhow::Result<PathBuf> {
        let base = directories::BaseDirs::new()
            .ok_or_else(|| anyhow::anyhow!("could not determine config directory"))?;

        Ok(base.config_dir().join("omni").join("cli"))
    }

    /// Get the data directory path (`~/.local/share/omni/cli/`).
    ///
    /// # Errors
    ///
    /// Returns an error if the data directory cannot be determined.
    pub fn data_dir() -> anyhow::Result<PathBuf> {
        let base = directories::BaseDirs::new()
            .ok_or_else(|| anyhow::anyhow!("could not determine data directory"))?;

        Ok(base.data_dir().join("omni").join("cli"))
    }

    /// Get the conversation history file path.
    ///
    /// # Errors
    ///
    /// Returns an error if the data directory cannot be determined.
    pub fn history_path() -> anyhow::Result<PathBuf> {
        Ok(Self::data_dir()?.join("conversation.json"))
    }

    /// Get the state file path for persisting runtime state
    fn state_path() -> anyhow::Result<PathBuf> {
        Ok(Self::data_dir()?.join("state.json"))
    }

    /// Save the current agent mode
    ///
    /// # Errors
    ///
    /// Returns an error if the state file cannot be written.
    pub fn save_mode(mode: AgentMode) -> anyhow::Result<()> {
        let path = Self::state_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mode_str = match mode {
            AgentMode::Build => "build",
            AgentMode::Plan => "plan",
        };

        let state = serde_json::json!({ "mode": mode_str });
        std::fs::write(&path, serde_json::to_string_pretty(&state)?)?;
        Ok(())
    }

    /// Load the saved agent mode
    ///
    /// Returns the default mode (Build) if no state file exists or parsing fails.
    #[must_use]
    pub fn load_mode() -> AgentMode {
        let Ok(path) = Self::state_path() else {
            return AgentMode::default();
        };

        let Ok(contents) = std::fs::read_to_string(&path) else {
            return AgentMode::default();
        };

        let Ok(state) = serde_json::from_str::<serde_json::Value>(&contents) else {
            return AgentMode::default();
        };

        match state.get("mode").and_then(|v| v.as_str()) {
            Some("plan") => AgentMode::Plan,
            _ => AgentMode::Build,
        }
    }
}

/// API server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiConfig {
    /// Host to bind to.
    pub host: String,

    /// Port to bind to.
    pub port: u16,

    /// API token for authentication (optional, but required for remote access).
    /// Can also be set via `OMNI_API_TOKEN` environment variable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 7890,
            token: None,
        }
    }
}

impl ApiConfig {
    /// Get the API token, preferring env var over config file.
    #[must_use]
    pub fn token(&self) -> Option<String> {
        std::env::var("OMNI_API_TOKEN")
            .ok()
            .or_else(|| self.token.clone())
    }

    /// Generate a new random API token.
    #[must_use]
    pub fn generate_token() -> String {
        use rand::Rng;
        let mut rng = rand::rng();
        let bytes: [u8; 32] = rng.random();
        format!("omni_{}", hex::encode(bytes))
    }
}

/// TUI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TuiConfig {
    /// Enable mouse support.
    pub mouse: bool,

    /// Show ecosystem tips on welcome screen.
    pub tips: bool,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            mouse: true,
            tips: true,
        }
    }
}

/// Agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    /// Active provider name (key in providers table).
    pub provider: String,

    /// Model to use.
    pub model: String,

    /// Maximum tokens in response.
    pub max_tokens: u32,

    /// Persona name to use (default: "orin").
    pub persona: String,

    /// Default agent to use on startup.
    pub default_agent: String,

    /// Provider definitions.
    #[serde(default = "AgentConfig::default_providers")]
    pub providers: HashMap<String, ProviderConfig>,

    /// Agent definitions.
    #[serde(default = "AgentConfig::default_agents")]
    pub agents: HashMap<String, AgentDefinition>,

    /// Known models with provider associations.
    #[serde(default = "AgentConfig::default_models")]
    pub models: Vec<ModelInfo>,
}

impl AgentConfig {
    /// Get the default model definitions.
    fn default_models() -> Vec<ModelInfo> {
        registry::default_models()
    }

    /// Look up the provider for a model.
    ///
    /// First checks the models registry (case-insensitive), then falls back to prefix detection.
    #[must_use]
    pub fn provider_for_model(&self, model_id: &str) -> Option<&str> {
        let model_lower = model_id.to_lowercase();
        // Check models registry (case-insensitive)
        if let Some(info) = self
            .models
            .iter()
            .find(|m| m.id.to_lowercase() == model_lower)
        {
            return Some(&info.provider);
        }
        // Fallback: detect by prefix
        registry::detect_provider_by_prefix(model_id)
    }

    /// Create a provider by name.
    ///
    /// # Errors
    ///
    /// Returns error if the provider is unknown or required API key is missing.
    pub fn create_provider_by_name(&self, name: &str) -> anyhow::Result<Box<dyn LlmProvider>> {
        let config = self.providers.get(name).ok_or_else(|| {
            anyhow::anyhow!("unknown provider '{name}', check [agent.providers] config")
        })?;

        Self::build_registry().create_provider(name, config)
    }

    /// Get the default agent definitions.
    fn default_agents() -> HashMap<String, AgentDefinition> {
        let mut agents = HashMap::new();

        agents.insert(
            "build".to_string(),
            AgentDefinition {
                description: "Full access for implementation".to_string(),
                model: None,
                permissions: AgentPermissions::default(),
            },
        );

        agents.insert(
            "plan".to_string(),
            AgentDefinition {
                description: "Read-only exploration for planning".to_string(),
                model: None,
                permissions: AgentPermissions::plan_mode(),
            },
        );

        agents
    }

    /// Get the current agent definition.
    #[must_use]
    pub fn current_agent(&self, agent_name: &str) -> Option<&AgentDefinition> {
        self.agents.get(agent_name)
    }

    /// Get the model for a specific agent (falls back to default model).
    #[must_use]
    pub fn model_for_agent(&self, agent_name: &str) -> &str {
        self.agents
            .get(agent_name)
            .and_then(|a| a.model.as_deref())
            .unwrap_or(&self.model)
    }

    /// Get the default provider configurations.
    ///
    /// Get the default provider configurations
    fn default_providers() -> HashMap<String, ProviderConfig> {
        registry::default_providers()
    }

    /// Discover models from synapse and merge into the model registry.
    ///
    /// Returns `true` if synapse was reachable and models were discovered.
    pub async fn discover_synapse_models(&mut self) -> bool {
        let Some(synapse_config) = self.providers.get("synapse") else {
            return false;
        };

        let base_url = synapse_config
            .base_url
            .as_deref()
            .unwrap_or("http://localhost:6000");

        let Ok(client) = SynapseClient::new(base_url) else {
            return false;
        };

        let client = if let Some(key) = resolve_api_key(synapse_config) {
            client.with_api_key(key)
        } else {
            client
        };

        match client.list_models().await {
            Ok(models) => {
                for model in models {
                    if !self.models.iter().any(|m| m.id == model.id) {
                        self.models.push(ModelInfo {
                            id: model.id,
                            provider: "synapse".to_string(),
                        });
                    }
                }
                true
            }
            Err(e) => {
                tracing::warn!("synapse model discovery failed: {e}");
                false
            }
        }
    }

    /// Build a provider registry with the synapse factory registered
    fn build_registry() -> ProviderRegistry {
        let mut registry = ProviderRegistry::new();

        registry.register_factory(
            "synapse",
            Box::new(|_name, config| {
                let base_url = config
                    .base_url
                    .as_deref()
                    .unwrap_or("http://localhost:6000");
                let client = SynapseClient::new(base_url)
                    .map_err(|e| anyhow::anyhow!("failed to create Synapse client: {e}"))?;
                let client = if let Some(key) = resolve_api_key(config) {
                    client.with_api_key(key)
                } else {
                    tracing::warn!(
                        "no Synapse API key configured; set SYNAPSE_API_KEY or \
                         add api_key under [agent.providers.synapse] in config"
                    );
                    client
                };
                Ok(Box::new(client) as Box<dyn LlmProvider>)
            }),
        );

        registry
    }

    /// Create the configured LLM provider.
    ///
    /// # Errors
    ///
    /// Returns error if the provider is unknown or required API key is missing.
    pub fn create_provider(&self) -> anyhow::Result<Box<dyn LlmProvider>> {
        let config = self.providers.get(&self.provider).ok_or_else(|| {
            anyhow::anyhow!(
                "unknown provider '{}', check [agent.providers] config",
                self.provider
            )
        })?;

        Self::build_registry().create_provider(&self.provider, config)
    }

    /// Create the configured provider with synapse fallback.
    ///
    /// If synapse is the active provider and discovery failed (unreachable),
    /// falls back to anthropic.
    ///
    /// # Errors
    ///
    /// Returns error if neither the primary nor fallback provider can be created.
    pub async fn create_provider_with_fallback(&mut self) -> anyhow::Result<Box<dyn LlmProvider>> {
        let synapse_reachable = self.discover_synapse_models().await;

        if self.provider == "synapse" && !synapse_reachable {
            tracing::warn!(
                "synapse is unreachable, falling back to anthropic"
            );
            self.provider = "anthropic".to_string();
        }

        self.create_provider()
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 8192,
            persona: "orin".to_string(),
            default_agent: "build".to_string(),
            providers: Self::default_providers(),
            agents: Self::default_agents(),
            models: Self::default_models(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_providers_exist() {
        let config = AgentConfig::default();
        assert!(config.providers.contains_key("anthropic"));
        assert!(config.providers.contains_key("openai"));
        assert!(config.providers.contains_key("ollama"));
    }

    #[test]
    fn default_provider_is_anthropic() {
        let config = AgentConfig::default();
        assert_eq!(config.provider, "anthropic");
    }

    #[test]
    fn ollama_has_base_url() {
        let config = AgentConfig::default();
        let ollama = config.providers.get("ollama").unwrap();
        assert_eq!(
            ollama.base_url,
            Some("http://localhost:11434/v1".to_string())
        );
    }

    #[test]
    fn resolve_api_key_from_direct_value() {
        let config = ProviderConfig {
            api_type: ProviderApiType::OpenAi,
            base_url: None,
            api_key_env: None,
            api_key: Some("sk-direct".to_string()),
        };
        assert_eq!(
            resolve_api_key(&config),
            Some("sk-direct".to_string())
        );
    }

    #[test]
    fn unknown_provider_returns_error() {
        let config = AgentConfig {
            provider: "nonexistent".to_string(),
            ..Default::default()
        };
        let result = config.create_provider();
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("unknown provider"));
    }

    #[test]
    fn default_agents_exist() {
        let config = AgentConfig::default();
        assert!(config.agents.contains_key("build"));
        assert!(config.agents.contains_key("plan"));
    }

    #[test]
    fn default_agent_is_build() {
        let config = AgentConfig::default();
        assert_eq!(config.default_agent, "build");
    }

    #[test]
    fn plan_agent_has_deny_permissions() {
        let config = AgentConfig::default();
        let plan = config.agents.get("plan").unwrap();
        assert_eq!(plan.permissions.edit, PermissionPreset::Deny);
        assert_eq!(plan.permissions.write, PermissionPreset::Deny);
        assert_eq!(plan.permissions.bash_write, PermissionPreset::Deny);
        assert_eq!(plan.permissions.bash_read, PermissionPreset::Allow);
    }

    #[test]
    fn build_agent_has_ask_permissions() {
        let config = AgentConfig::default();
        let build = config.agents.get("build").unwrap();
        assert_eq!(build.permissions.edit, PermissionPreset::Ask);
        assert_eq!(build.permissions.write, PermissionPreset::Ask);
    }

    #[test]
    fn model_for_agent_uses_override() {
        let mut config = AgentConfig::default();
        config.agents.get_mut("plan").unwrap().model = Some("custom-model".to_string());
        assert_eq!(config.model_for_agent("plan"), "custom-model");
        assert_eq!(config.model_for_agent("build"), config.model);
    }

    #[test]
    fn kimi_provider_exists() {
        let config = AgentConfig::default();
        assert!(config.providers.contains_key("kimi"));
    }

    #[test]
    fn kimi_has_correct_base_url() {
        let config = AgentConfig::default();
        let kimi = config.providers.get("kimi").unwrap();
        assert_eq!(
            kimi.base_url,
            Some("https://api.moonshot.cn/v1".to_string())
        );
        assert_eq!(kimi.api_type, ProviderApiType::OpenAi);
    }

    #[test]
    fn provider_for_model_detects_kimi() {
        let config = AgentConfig::default();
        assert_eq!(config.provider_for_model("kimi-k2.5"), Some("kimi"));
        assert_eq!(config.provider_for_model("moonshot-v1-128k"), Some("kimi"));
        assert_eq!(config.provider_for_model("KIMI-K2.5"), Some("kimi"));
    }

    #[test]
    fn synapse_provider_uses_named_type() {
        let config = AgentConfig::default();
        let synapse = config.providers.get("synapse").unwrap();
        assert_eq!(synapse.api_type, ProviderApiType::Synapse);
    }
}
