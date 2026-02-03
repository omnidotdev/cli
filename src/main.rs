use std::io::Write as _;
use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use omni_cli::{
    Config,
    cli::{AuthCommands, Cli, Commands, ConfigCommands, SessionCommands},
    core::session::SessionTarget,
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
    // Bare prompt = shell mode
    if let Some(prompt) = cli.prompt {
        if cli.command.is_some() {
            anyhow::bail!("Cannot use both a prompt and a subcommand");
        }
        let config = Config::load()?;
        let provider = config.agent.create_provider()?;
        return omni_cli::core::shell::run(
            provider,
            &config.agent.model,
            &prompt,
            cli.yes,
            cli.dry_run,
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e}"));
    }

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
            let config = Config::load()?;
            let provider = config.agent.create_provider()?;
            let mut agent = omni_cli::core::Agent::with_context(
                provider,
                &config.agent.model,
                config.agent.max_tokens,
                None,
            );

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

        Commands::Auth { command } => {
            handle_auth_command(command)?;
        }
    }

    Ok(())
}

fn handle_auth_command(command: AuthCommands) -> anyhow::Result<()> {
    match command {
        AuthCommands::Login(args) => {
            omni_cli::cli::auth::auth_login(args)?;
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
