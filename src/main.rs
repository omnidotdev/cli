use std::io::Write as _;
use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use omni_cli::{
    Config,
    cli::{Cli, Commands, ConfigCommands, SessionCommands},
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
        Commands::Agent { prompt } => {
            let config = Config::load()?;
            let provider = config.agent.create_provider()?;
            let mut agent = omni_cli::core::Agent::with_context(
                provider,
                &config.agent.model,
                config.agent.max_tokens,
                None,
            );

            let _response = agent
                .chat(&prompt, |text| {
                    print!("{text}");
                    std::io::stdout().flush().ok();
                })
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;

            println!();
        }

        Commands::Tui => {
            omni_cli::tui::run().await?;
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
    }

    Ok(())
}

fn handle_session_command(command: SessionCommands) -> anyhow::Result<()> {
    use omni_cli::core::project::Project;
    use omni_cli::core::session::SessionManager;
    use omni_cli::core::storage::Storage;

    let data_dir = Config::data_dir()?;
    let storage = Storage::with_root(data_dir);
    let project = Project::detect(&std::env::current_dir()?)?;
    let manager = SessionManager::new(storage, project);

    match command {
        SessionCommands::List { format, limit } => {
            let sessions = manager.list_sessions()?;
            let sessions: Vec<_> = sessions.into_iter().take(limit).collect();

            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&sessions)?);
            } else {
                // Table format
                println!("{:<36} {:<30} Created", "ID", "Title");
                println!("{}", "-".repeat(80));
                for session in sessions {
                    let created = chrono::DateTime::from_timestamp_millis(session.time.created)
                        .map_or_else(
                            || "Unknown".to_string(),
                            |dt| dt.format("%Y-%m-%d %H:%M").to_string(),
                        );
                    let title: String = session.title.chars().take(28).collect();
                    println!("{:<36} {:<30} {}", session.id, title, created);
                }
            }
        }

        SessionCommands::Export {
            session_id,
            format,
            output,
        } => {
            let content = if format == "markdown" {
                manager.export_to_markdown(&session_id)?
            } else {
                manager.export_to_json(&session_id)?
            };

            if let Some(path) = output {
                std::fs::write(&path, &content)?;
                println!("Exported session to {path}");
            } else {
                println!("{content}");
            }
        }
    }

    Ok(())
}
