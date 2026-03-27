use std::io::Write as _;
use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use omni_cli::{
    Config,
    cli::{AuthCommands, Cli, Commands, ConfigCommands, SessionCommands, SynapseCommands},
    core::session::SessionTarget,
    plugin::{PluginDiscovery, PluginType, install_plugin, run_plugin_command},
};

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    // Set up logging based on verbosity
    let filter = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(filter))
        .init();

    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    // No subcommand = launch TUI
    let Some(command) = cli.command else {
        return omni_cli::tui::run().await;
    };

    match command {
        Commands::Agent {
            prompt,
            r#continue,
            session,
        } => {
            // Fail fast if explicit session ID doesn't exist
            if let Some(ref id) = session {
                let manager = omni_cli::core::session::SessionManager::for_current_project()?;
                manager
                    .find_session(id)
                    .map_err(|_| anyhow::anyhow!("session not found: {id}"))?;
            }

            let target = SessionTarget::from_flags(r#continue, session);
            let mut config = Config::load()?;
            let provider = config.agent.create_provider_with_fallback().await?;
            let mut agent = config.agent.create_agent(provider).await;

            // Load Synapse MCP tools
            if let Some(synapse) = config.agent.create_synapse_client() {
                agent.load_synapse_tools(synapse).await;
            }

            // Wire up Aether usage recording if configured
            if let Some(recorder) = config.agent.create_usage_recorder() {
                agent.set_usage_recorder(recorder);
            }

            // Enable sessions with target
            if let Err(e) = agent.enable_sessions_with_target(target) {
                tracing::warn!("failed to enable sessions: {e}");
            }

            let _response = agent
                .chat(&prompt, |text| {
                    print!("{text}");
                    std::io::stdout().flush().ok();
                })
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;

            println!();
        }

        Commands::Tui {
            r#continue,
            session,
        } => {
            // Fail fast if explicit session ID doesn't exist
            if let Some(ref id) = session {
                let manager = omni_cli::core::session::SessionManager::for_current_project()?;
                manager
                    .find_session(id)
                    .map_err(|_| anyhow::anyhow!("session not found: {id}"))?;
            }

            let target = SessionTarget::from_flags(r#continue, session);
            omni_cli::tui::run_with_target(target).await?;
        }

        Commands::Serve { host, port } => {
            omni_cli::api::serve(&host, port).await?;
        }

        Commands::Config { command } => match command {
            ConfigCommands::Show => {
                let config = Config::load()?;
                println!("{}", toml::to_string_pretty(&config)?);
            }
            ConfigCommands::Path => {
                let path = Config::config_path()?;
                println!("{}", path.display());
            }
            ConfigCommands::GenerateToken => {
                let token = omni_cli::config::ApiConfig::generate_token();
                println!("Generated API token:\n");
                println!("  {token}\n");
                println!("Add to your config.toml:");
                println!("  [api]");
                println!("  token = \"{token}\"\n");
                println!("Or set environment variable:");
                println!("  export OMNI_API_TOKEN=\"{token}\"");
            }
        },

        Commands::Session { command } => {
            handle_session_command(command)?;
        }

        Commands::Synapse { command } => {
            handle_synapse_command(command).await?;
        }

        Commands::Auth { command } => match command {
            AuthCommands::Login => omni_cli::cli::auth::login().await?,
            AuthCommands::Logout => omni_cli::cli::auth::logout()?,
        },

        Commands::Plugins => {
            handle_plugins_command()?;
        }

        Commands::Install { plugins } => {
            for name in &plugins {
                install_plugin(name)?;
            }
        }

        Commands::Uninstall { plugin } => {
            handle_uninstall_command(&plugin)?;
        }

        Commands::External(args) => {
            let code = handle_external_command(&args)?;
            if code != 0 {
                std::process::exit(code);
            }
        }
    }

    Ok(())
}

fn handle_session_command(command: SessionCommands) -> anyhow::Result<()> {
    use omni_cli::core::session::SessionManager;

    let manager = SessionManager::for_current_project()?;

    match command {
        SessionCommands::List { format, limit } => {
            let sessions = manager.list_sessions()?;
            let sessions: Vec<_> = sessions.into_iter().take(limit).collect();

            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&sessions)?);
            } else {
                // Table format - show slug for easy CLI use
                println!("{:<20} {:<30} Created", "Slug", "Title");
                println!("{}", "-".repeat(70));
                for session in sessions {
                    let created = chrono::DateTime::from_timestamp_millis(session.time.created)
                        .map_or_else(
                            || "Unknown".to_string(),
                            |dt| dt.format("%Y-%m-%d %H:%M").to_string(),
                        );
                    let title: String = session.title.chars().take(28).collect();
                    println!("{:<20} {:<30} {}", session.slug, title, created);
                }
            }
        }

        SessionCommands::Export {
            session_id,
            format,
            output,
        } => {
            // Resolve slug or ID to actual session ID
            let session = manager.find_session(&session_id)?;
            let content = if format == "markdown" {
                manager.export_to_markdown(&session.id)?
            } else {
                manager.export_to_json(&session.id)?
            };

            if let Some(path) = output {
                std::fs::write(&path, &content)?;
                println!("Exported session to {path}");
            } else {
                println!("{content}");
            }
        }

        SessionCommands::Share {
            session_id,
            expires,
        } => {
            use omni_cli::core::session::ShareOptions;

            // Resolve slug or ID to actual session ID
            let session = manager.find_session(&session_id)?;
            let ttl_seconds = expires.map(|e| parse_duration(&e)).transpose()?;
            let options = ShareOptions { ttl_seconds };

            let share = manager.create_share(&session.id, options)?;

            println!("Share created!");
            println!();
            println!("  Token:  {}", share.token);
            println!("  Secret: {}", share.secret);
            println!();
            println!("Access via API:");
            println!("  GET http://localhost:7890/api/share/{}", share.token);
            println!();
            if let Some(expires_at) = share.expires_at {
                let expires = chrono::DateTime::from_timestamp_millis(expires_at).map_or_else(
                    || "Unknown".to_string(),
                    |dt| dt.format("%Y-%m-%d %H:%M UTC").to_string(),
                );
                println!("Expires: {expires}");
            } else {
                println!("Expires: Never");
            }
            println!();
            println!("To revoke:");
            println!(
                "  omni session unshare {} --secret {}",
                share.token, share.secret
            );
        }

        SessionCommands::Unshare { token, secret } => {
            manager.revoke_share(&token, &secret)?;
            println!("Share revoked");
        }
    }

    Ok(())
}

async fn handle_synapse_command(command: SynapseCommands) -> anyhow::Result<()> {
    match command {
        SynapseCommands::Status => {
            let config = Config::load()?;
            let synapse_config = config.agent.providers.get("synapse");

            let base_url = synapse_config
                .and_then(|c| c.base_url.as_deref())
                .unwrap_or("http://localhost:6000");

            println!("Synapse endpoint: {base_url}");
            println!();

            // Health check
            let health_url = format!("{base_url}/health");
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()?;

            match client.get(&health_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    println!("  Health:  ok");
                }
                Ok(resp) => {
                    println!("  Health:  unhealthy (HTTP {})", resp.status());
                    return Ok(());
                }
                Err(e) => {
                    println!("  Health:  unreachable ({e})");
                    return Ok(());
                }
            }

            // Model discovery
            let models_url = format!("{base_url}/v1/models");
            match client.get(&models_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let body: serde_json::Value = resp.json().await?;
                    if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
                        println!("  Models:  {} available", data.len());
                        println!();
                        for model in data {
                            if let Some(id) = model.get("id").and_then(|v| v.as_str()) {
                                let owner = model
                                    .get("owned_by")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown");
                                println!("    {id}  ({owner})");
                            }
                        }
                    } else {
                        println!("  Models:  (unexpected response format)");
                    }
                }
                Ok(resp) => {
                    println!("  Models:  failed (HTTP {})", resp.status());
                }
                Err(e) => {
                    println!("  Models:  failed ({e})");
                }
            }
        }
    }

    Ok(())
}

fn handle_plugins_command() -> anyhow::Result<()> {
    let plugins_dir = PluginDiscovery::default_dir()?;
    let discovery = PluginDiscovery::new(plugins_dir);
    let plugins = discovery.discover_all()?;

    if plugins.is_empty() {
        println!("No plugins installed. Use `omni install <name>` to install one.");
        return Ok(());
    }

    println!("{:<16} {:<10} {:<8} Description", "Name", "Version", "Type",);
    println!("{}", "-".repeat(60));

    for plugin in &plugins {
        if let Some(manifest) = &plugin.manifest {
            let type_str = match manifest.plugin_type {
                PluginType::Bin => "bin",
                PluginType::Api => "api",
                PluginType::Launch => "launch",
            };
            println!(
                "{:<16} {:<10} {:<8} {}",
                manifest.name, manifest.version, type_str, manifest.description
            );
        }
    }

    Ok(())
}

fn handle_uninstall_command(name: &str) -> anyhow::Result<()> {
    let base = PluginDiscovery::default_dir()?;
    let target = base.join(name);

    if !target.exists() {
        anyhow::bail!("plugin '{name}' is not installed");
    }

    std::fs::remove_dir_all(&target)?;
    println!("Uninstalled plugin '{name}'");
    Ok(())
}

fn handle_external_command(args: &[String]) -> anyhow::Result<i32> {
    let name = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("no command specified"))?;
    let remaining: Vec<&str> = args[1..].iter().map(String::as_str).collect();

    let plugins_dir = PluginDiscovery::default_dir()?;
    let discovery = PluginDiscovery::new(plugins_dir);

    let plugin = discovery.find(name)?.ok_or_else(|| {
        anyhow::anyhow!("unknown command '{name}'. Run `omni install {name}` to install it")
    })?;

    if let Some(manifest) = &plugin.manifest {
        run_plugin_command(manifest, &remaining)
    } else {
        // PATH-only fallback: run omni-<name> binary
        let bin = format!("omni-{name}");
        let status = std::process::Command::new(&bin)
            .args(&remaining)
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .map_err(|e| anyhow::anyhow!("failed to execute '{bin}': {e}"))?;
        Ok(status.code().unwrap_or(1))
    }
}

/// Parse a duration string (e.g., "1h", "7d") to seconds
fn parse_duration(s: &str) -> anyhow::Result<u64> {
    let s = s.trim();
    if s.is_empty() {
        anyhow::bail!("empty duration");
    }

    let (num, unit) = s.split_at(s.len() - 1);
    let num: u64 = num
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid duration number"))?;

    let seconds = match unit {
        "s" => num,
        "m" => num * 60,
        "h" => num * 3600,
        "d" => num * 86400,
        "w" => num * 604_800,
        _ => anyhow::bail!("invalid duration unit (use s, m, h, d, or w)"),
    };

    Ok(seconds)
}
