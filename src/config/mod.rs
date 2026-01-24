//! Configuration management for the Omni CLI.

mod persona;

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::agent::{AgentMode, AnthropicProvider, LlmProvider, OpenAiProvider};

pub use persona::{Persona, list_personas, load_persona, personas_dir};

/// Provider API type.
///
/// Determines which API format to use for communication.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderApiType {
    /// Anthropic Messages API (unique streaming format).
    #[default]
    Anthropic,
    /// `OpenAI` Chat Completions API (also used by compatible providers)
    OpenAi,
}

/// Permission action for agent operations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionPreset {
    /// Always allow without prompting.
    Allow,
    /// Always deny.
    Deny,
    /// Ask user each time.
    #[default]
    Ask,
}

/// Permission configuration for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentPermissions {
    /// File editing permission.
    pub edit: PermissionPreset,
    /// File writing (new files) permission.
    pub write: PermissionPreset,
    /// Destructive shell commands permission.
    pub bash_write: PermissionPreset,
    /// Read-only shell commands permission.
    pub bash_read: PermissionPreset,
    /// File reading permission.
    pub read: PermissionPreset,
    /// Web search permission.
    pub web_search: PermissionPreset,
    /// Code search permission.
    pub code_search: PermissionPreset,
}

impl Default for AgentPermissions {
    fn default() -> Self {
        Self {
            edit: PermissionPreset::Ask,
            write: PermissionPreset::Ask,
            bash_write: PermissionPreset::Ask,
            bash_read: PermissionPreset::Allow,
            read: PermissionPreset::Allow,
            web_search: PermissionPreset::Ask,
            code_search: PermissionPreset::Ask,
        }
    }
}

impl AgentPermissions {
    /// Permissions for read-only plan mode.
    #[must_use]
    pub const fn plan_mode() -> Self {
        Self {
            edit: PermissionPreset::Deny,
            write: PermissionPreset::Deny,
            bash_write: PermissionPreset::Deny,
            bash_read: PermissionPreset::Allow,
            read: PermissionPreset::Allow,
            web_search: PermissionPreset::Ask,
            code_search: PermissionPreset::Ask,
        }
    }
}

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

/// Individual provider configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// API type (anthropic or openai-compatible).
    #[serde(rename = "type", default)]
    pub api_type: ProviderApiType,

    /// Base URL override (for OpenAI-compatible providers).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    /// Environment variable name for API key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,

    /// Direct API key (discouraged, prefer `api_key_env`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
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
}

impl AgentConfig {
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
    fn default_providers() -> HashMap<String, ProviderConfig> {
        let mut providers = HashMap::new();

        providers.insert(
            "anthropic".to_string(),
            ProviderConfig {
                api_type: ProviderApiType::Anthropic,
                base_url: None,
                api_key_env: Some("ANTHROPIC_API_KEY".to_string()),
                api_key: None,
            },
        );

        providers.insert(
            "openai".to_string(),
            ProviderConfig {
                api_type: ProviderApiType::OpenAi,
                base_url: None,
                api_key_env: Some("OPENAI_API_KEY".to_string()),
                api_key: None,
            },
        );

        providers.insert(
            "ollama".to_string(),
            ProviderConfig {
                api_type: ProviderApiType::OpenAi,
                base_url: Some("http://localhost:11434/v1".to_string()),
                api_key_env: None,
                api_key: None,
            },
        );

        providers
    }

    /// Resolve API key for a provider config.
    fn resolve_api_key(config: &ProviderConfig) -> Option<String> {
        // First try env var
        if let Some(env_name) = &config.api_key_env {
            if let Ok(key) = std::env::var(env_name) {
                return Some(key);
            }
        }
        // Fall back to direct key
        config.api_key.clone()
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

        match config.api_type {
            ProviderApiType::Anthropic => {
                let key = Self::resolve_api_key(config).ok_or_else(|| {
                    anyhow::anyhow!("API key not set for provider '{}'", self.provider)
                })?;
                Ok(Box::new(AnthropicProvider::new(key)?))
            }
            ProviderApiType::OpenAi => {
                let api_key = Self::resolve_api_key(config);
                let base_url = config.base_url.clone();
                Ok(Box::new(OpenAiProvider::with_config(api_key, base_url)?))
            }
        }
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
            AgentConfig::resolve_api_key(&config),
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
}
