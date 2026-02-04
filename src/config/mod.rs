//! Configuration management for the Omni CLI.

mod persona;

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::agent::{
    AgentMode, AnthropicProvider, LlmProvider, OpenAiProvider, UnifiedProvider,
};
use crate::core::keychain;

pub use persona::{Persona, list_personas, load_persona, personas_dir};

/// Model information with provider association.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier (e.g., "claude-sonnet-4-20250514", "gpt-4o")
    pub id: String,
    /// Provider name (e.g., "anthropic", "openai")
    pub provider: String,
}

/// Provider API type.
///
/// Determines which API format to use for communication.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderApiType {
    /// Anthropic Messages API (unique streaming format)
    #[default]
    Anthropic,
    /// `OpenAI` Chat Completions API (also used by compatible providers)
    OpenAi,
    /// Google Gemini API
    Google,
    /// Groq API
    Groq,
    /// Mistral API
    Mistral,
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
    #[serde(rename = "type", default)]
    pub api_type: ProviderApiType,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
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
        if let Ok(xdg_config_home) = std::env::var("XDG_CONFIG_HOME") {
            return Ok(PathBuf::from(xdg_config_home).join("omni").join("cli"));
        }

        if cfg!(target_os = "macos") {
            if let Ok(home) = std::env::var("HOME") {
                return Ok(PathBuf::from(home).join(".config").join("omni").join("cli"));
            }
        }

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

    /// Save a provider configuration to the config file
    ///
    /// # Errors
    ///
    /// Returns an error if the config file cannot be written.
    pub fn save_provider(name: &str, config: &ProviderConfig) -> anyhow::Result<()> {
        Self::save_provider_to_path(name, config, &Self::config_path()?)
    }

    pub fn save_provider_to_path(
        name: &str,
        config: &ProviderConfig,
        path: &PathBuf,
    ) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut config_value = if path.exists() {
            let contents = std::fs::read_to_string(path)?;
            toml::from_str::<toml::Value>(&contents)?
        } else {
            toml::Value::Table(toml::map::Map::new())
        };

        let config_table = config_value
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("config root must be a table"))?;

        let agent_table = config_table
            .entry("agent")
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("agent section must be a table"))?;

        let providers_table = agent_table
            .entry("providers")
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("providers section must be a table"))?;

        let provider_value = toml::Value::try_from(config)?;
        providers_table.insert(name.to_string(), provider_value);

        let toml_string = toml::to_string_pretty(&config_value)?;
        std::fs::write(path, toml_string)?;
        Ok(())
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
        vec![
            // Anthropic
            ModelInfo {
                id: "claude-sonnet-4-20250514".to_string(),
                provider: "anthropic".to_string(),
            },
            ModelInfo {
                id: "claude-opus-4-20250514".to_string(),
                provider: "anthropic".to_string(),
            },
            ModelInfo {
                id: "claude-3-5-haiku-20241022".to_string(),
                provider: "anthropic".to_string(),
            },
            // OpenAI
            ModelInfo {
                id: "gpt-4o".to_string(),
                provider: "openai".to_string(),
            },
            ModelInfo {
                id: "gpt-4-turbo".to_string(),
                provider: "openai".to_string(),
            },
            ModelInfo {
                id: "gpt-3.5-turbo".to_string(),
                provider: "openai".to_string(),
            },
            ModelInfo {
                id: "o1".to_string(),
                provider: "openai".to_string(),
            },
            ModelInfo {
                id: "o1-mini".to_string(),
                provider: "openai".to_string(),
            },
            // Groq (fast inference)
            ModelInfo {
                id: "llama-3.3-70b-versatile".to_string(),
                provider: "groq".to_string(),
            },
            ModelInfo {
                id: "llama-3.1-8b-instant".to_string(),
                provider: "groq".to_string(),
            },
            ModelInfo {
                id: "mixtral-8x7b-32768".to_string(),
                provider: "groq".to_string(),
            },
            // Google
            ModelInfo {
                id: "gemini-2.0-flash".to_string(),
                provider: "google".to_string(),
            },
            ModelInfo {
                id: "gemini-1.5-pro".to_string(),
                provider: "google".to_string(),
            },
            // Mistral
            ModelInfo {
                id: "mistral-large-latest".to_string(),
                provider: "mistral".to_string(),
            },
            ModelInfo {
                id: "codestral-latest".to_string(),
                provider: "mistral".to_string(),
            },
            // Together
            ModelInfo {
                id: "meta-llama/Llama-3.3-70B-Instruct-Turbo".to_string(),
                provider: "together".to_string(),
            },
            ModelInfo {
                id: "Qwen/Qwen2.5-Coder-32B-Instruct".to_string(),
                provider: "together".to_string(),
            },
            // Kimi (Moonshot AI)
            ModelInfo {
                id: "kimi-k2.5".to_string(),
                provider: "kimi".to_string(),
            },
            ModelInfo {
                id: "moonshot-v1-128k".to_string(),
                provider: "kimi".to_string(),
            },
            ModelInfo {
                id: "moonshot-v1-32k".to_string(),
                provider: "kimi".to_string(),
            },
        ]
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
        // Fallback: detect by prefix (case-insensitive)
        if model_lower.starts_with("claude") {
            Some("anthropic")
        } else if model_lower.starts_with("gpt") || model_lower.starts_with("o1") {
            Some("openai")
        } else if model_lower.starts_with("kimi") || model_lower.starts_with("moonshot") {
            Some("kimi")
        } else {
            None
        }
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

        let missing_key_error = || {
            anyhow::anyhow!(
                "No API key configured for provider '{name}'.\n\n\
                 Run `omni auth login` to configure your credentials."
            )
        };

        match config.api_type {
            ProviderApiType::Anthropic => {
                let key = Self::resolve_api_key(name, config).ok_or_else(missing_key_error)?;
                Ok(Box::new(AnthropicProvider::new(key)?))
            }
            ProviderApiType::OpenAi => {
                let api_key = Self::resolve_api_key(name, config);
                let base_url = config.base_url.clone();
                Ok(Box::new(OpenAiProvider::with_config(
                    api_key, base_url, name,
                )?))
            }
            ProviderApiType::Google => {
                let key = Self::resolve_api_key(name, config).ok_or_else(missing_key_error)?;
                Ok(Box::new(UnifiedProvider::google(key)?))
            }
            ProviderApiType::Groq => {
                let key = Self::resolve_api_key(name, config).ok_or_else(missing_key_error)?;
                Ok(Box::new(UnifiedProvider::groq(key)?))
            }
            ProviderApiType::Mistral => {
                let key = Self::resolve_api_key(name, config).ok_or_else(missing_key_error)?;
                Ok(Box::new(UnifiedProvider::mistral(key)?))
            }
        }
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
    fn default_providers() -> HashMap<String, ProviderConfig> {
        let mut providers = HashMap::new();

        providers.insert(
            "anthropic".to_string(),
            ProviderConfig {
                api_type: ProviderApiType::Anthropic,
                base_url: None,
                api_key_env: Some("ANTHROPIC_API_KEY".to_string()),
            },
        );

        providers.insert(
            "openai".to_string(),
            ProviderConfig {
                api_type: ProviderApiType::OpenAi,
                base_url: None,
                api_key_env: Some("OPENAI_API_KEY".to_string()),
            },
        );

        providers.insert(
            "ollama".to_string(),
            ProviderConfig {
                api_type: ProviderApiType::OpenAi,
                base_url: Some("http://localhost:11434/v1".to_string()),
                api_key_env: None,
            },
        );

        providers.insert(
            "lmstudio".to_string(),
            ProviderConfig {
                api_type: ProviderApiType::OpenAi,
                base_url: Some("http://localhost:1234/v1".to_string()),
                api_key_env: None,
            },
        );

        providers.insert(
            "groq".to_string(),
            ProviderConfig {
                api_type: ProviderApiType::Groq,
                base_url: None,
                api_key_env: Some("GROQ_API_KEY".to_string()),
            },
        );

        providers.insert(
            "google".to_string(),
            ProviderConfig {
                api_type: ProviderApiType::Google,
                base_url: None,
                api_key_env: Some("GOOGLE_API_KEY".to_string()),
            },
        );

        providers.insert(
            "mistral".to_string(),
            ProviderConfig {
                api_type: ProviderApiType::Mistral,
                base_url: None,
                api_key_env: Some("MISTRAL_API_KEY".to_string()),
            },
        );

        providers.insert(
            "openrouter".to_string(),
            ProviderConfig {
                api_type: ProviderApiType::OpenAi,
                base_url: Some("https://openrouter.ai/api/v1".to_string()),
                api_key_env: Some("OPENROUTER_API_KEY".to_string()),
            },
        );

        providers.insert(
            "together".to_string(),
            ProviderConfig {
                api_type: ProviderApiType::OpenAi,
                base_url: Some("https://api.together.xyz/v1".to_string()),
                api_key_env: Some("TOGETHER_API_KEY".to_string()),
            },
        );

        providers.insert(
            "kimi".to_string(),
            ProviderConfig {
                api_type: ProviderApiType::OpenAi,
                base_url: Some("https://api.moonshot.cn/v1".to_string()),
                api_key_env: Some("MOONSHOT_API_KEY".to_string()),
            },
        );

        providers
    }

    pub(crate) fn resolve_api_key(provider_name: &str, config: &ProviderConfig) -> Option<String> {
        // Check environment variable first (avoids Keychain prompts during development,
        // since each `cargo build` produces a new binary that macOS sees as untrusted)
        if let Some(env_name) = &config.api_key_env {
            if let Ok(key) = std::env::var(env_name) {
                return Some(key);
            }
        }

        // Fall back to Keychain (for users who ran `omni auth login`)
        keychain::get_api_key(provider_name)
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

        let provider_name = &self.provider;
        let missing_key_error = || {
            anyhow::anyhow!(
                "No API key configured for provider '{provider_name}'.\n\n\
                 Run `omni auth login` to configure your credentials."
            )
        };

        match config.api_type {
            ProviderApiType::Anthropic => {
                let key =
                    Self::resolve_api_key(provider_name, config).ok_or_else(missing_key_error)?;
                Ok(Box::new(AnthropicProvider::new(key)?))
            }
            ProviderApiType::OpenAi => {
                let api_key = Self::resolve_api_key(provider_name, config);
                let base_url = config.base_url.clone();
                Ok(Box::new(OpenAiProvider::with_config(
                    api_key,
                    base_url,
                    provider_name,
                )?))
            }
            ProviderApiType::Google => {
                let key =
                    Self::resolve_api_key(provider_name, config).ok_or_else(missing_key_error)?;
                Ok(Box::new(UnifiedProvider::google(key)?))
            }
            ProviderApiType::Groq => {
                let key =
                    Self::resolve_api_key(provider_name, config).ok_or_else(missing_key_error)?;
                Ok(Box::new(UnifiedProvider::groq(key)?))
            }
            ProviderApiType::Mistral => {
                let key =
                    Self::resolve_api_key(provider_name, config).ok_or_else(missing_key_error)?;
                Ok(Box::new(UnifiedProvider::mistral(key)?))
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
    fn test_save_provider_creates_config() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let provider_config = ProviderConfig {
            api_type: ProviderApiType::Anthropic,
            base_url: None,
            api_key_env: Some("TEST_API_KEY".to_string()),
        };

        Config::save_provider_to_path("test_provider", &provider_config, &config_path).unwrap();

        assert!(config_path.exists(), "Config file should be created");

        let contents = std::fs::read_to_string(&config_path).unwrap();
        let loaded: toml::Value = toml::from_str(&contents).unwrap();

        let providers = loaded
            .get("agent")
            .and_then(|a| a.get("providers"))
            .and_then(|p| p.as_table())
            .expect("Should have agent.providers table");

        assert!(
            providers.contains_key("test_provider"),
            "Should contain test_provider"
        );
    }

    #[test]
    fn test_save_provider_preserves_existing() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let initial_config = r#"
[agent]
model = "claude-sonnet-4-20250514"
provider = "anthropic"

[tui]
mouse = true
tips = false

[agent.providers.anthropic]
type = "anthropic"
api_key_env = "ANTHROPIC_API_KEY"
"#;
        std::fs::write(&config_path, initial_config).unwrap();

        let new_provider = ProviderConfig {
            api_type: ProviderApiType::OpenAi,
            base_url: Some("https://api.example.com".to_string()),
            api_key_env: Some("EXAMPLE_API_KEY".to_string()),
        };

        Config::save_provider_to_path("example", &new_provider, &config_path).unwrap();

        let contents = std::fs::read_to_string(&config_path).unwrap();
        let loaded: toml::Value = toml::from_str(&contents).unwrap();

        assert_eq!(
            loaded
                .get("agent")
                .and_then(|a| a.get("model"))
                .and_then(|m| m.as_str()),
            Some("claude-sonnet-4-20250514")
        );
        assert_eq!(
            loaded
                .get("agent")
                .and_then(|a| a.get("provider"))
                .and_then(|p| p.as_str()),
            Some("anthropic")
        );

        assert_eq!(
            loaded
                .get("tui")
                .and_then(|t| t.get("mouse"))
                .and_then(toml::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            loaded
                .get("tui")
                .and_then(|t| t.get("tips"))
                .and_then(toml::Value::as_bool),
            Some(false)
        );

        let providers = loaded
            .get("agent")
            .and_then(|a| a.get("providers"))
            .and_then(|p| p.as_table())
            .expect("Should have agent.providers table");

        assert!(
            providers.contains_key("anthropic"),
            "Should preserve anthropic provider"
        );
        assert!(
            providers.contains_key("example"),
            "Should add example provider"
        );
    }

    #[test]
    fn test_env_var_takes_precedence_in_resolve_api_key() {
        // This test verifies the resolution order is env -> keychain
        // The keychain is mocked in tests (returns None), so we only test env var path
        // We use HOME which is guaranteed to be set in test environments
        let config = ProviderConfig {
            api_type: ProviderApiType::Anthropic,
            base_url: None,
            api_key_env: Some("HOME".to_string()),
        };

        // resolve_api_key should return the HOME env var value
        let result = AgentConfig::resolve_api_key("test-provider", &config);

        // HOME should be set in test environment, so result should be Some
        assert!(result.is_some(), "HOME env var should be set");

        // Verify it matches the actual HOME value
        let home_value = std::env::var("HOME").ok();
        assert_eq!(result, home_value);
    }
}
