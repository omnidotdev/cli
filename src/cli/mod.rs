//! CLI command parsing and execution.

use clap::{Parser, Subcommand};

/// Omni CLI - Agentic CLI for the Omni ecosystem.
#[derive(Parser)]
#[command(name = "omni")]
#[command(about = "Agentic CLI for the Omni ecosystem")]
#[command(version)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Natural language shell command (shell mode).
    ///
    /// Converts natural language to shell commands and executes them.
    /// Safe commands auto-execute; others require confirmation.
    pub prompt: Option<String>,

    /// Skip confirmation for all commands.
    #[arg(short, long)]
    pub yes: bool,

    /// Show command only, don't execute.
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Increase logging verbosity.
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Execute an agentic task.
    #[command(visible_alias = "a")]
    Agent {
        /// The prompt or task to execute.
        prompt: String,

        /// Continue the most recent session.
        #[arg(short, long, conflicts_with = "session")]
        r#continue: bool,

        /// Resume a specific session by ID.
        #[arg(short, long, conflicts_with = "continue")]
        session: Option<String>,
    },

    /// Start the TUI interface.
    Tui {
        /// Continue the most recent session.
        #[arg(short, long, conflicts_with = "session")]
        r#continue: bool,

        /// Resume a specific session by ID.
        #[arg(short, long, conflicts_with = "continue")]
        session: Option<String>,
    },

    /// Start the HTTP API server.
    Serve {
        /// Host to bind to.
        #[arg(short = 'H', long, default_value = "127.0.0.1")]
        host: String,

        /// Port to bind to.
        #[arg(short, long, default_value = "7890")]
        port: u16,
    },

    /// Manage configuration.
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Manage sessions.
    Session {
        #[command(subcommand)]
        command: SessionCommands,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show the current configuration.
    Show,

    /// Show the configuration file path.
    Path,

    /// Generate a new API token for remote access.
    GenerateToken,
}

#[derive(Subcommand)]
pub enum SessionCommands {
    /// List all sessions.
    List {
        /// Output format (table or json).
        #[arg(short, long, default_value = "table")]
        format: String,

        /// Limit number of sessions shown.
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Export a session.
    Export {
        /// Session ID to export.
        session_id: String,

        /// Output format (json or markdown).
        #[arg(short, long, default_value = "json")]
        format: String,

        /// Output file path (stdout if not specified).
        #[arg(short, long)]
        output: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_parses_no_args() {
        let cli = Cli::parse_from(["omni"]);
        assert_eq!(cli.verbose, 0);
        assert!(cli.command.is_none());
    }

    #[test]
    fn cli_parses_verbose_flag() {
        let cli = Cli::parse_from(["omni", "-v"]);
        assert_eq!(cli.verbose, 1);

        let cli = Cli::parse_from(["omni", "-vv"]);
        assert_eq!(cli.verbose, 2);

        let cli = Cli::parse_from(["omni", "-vvv"]);
        assert_eq!(cli.verbose, 3);
    }

    #[test]
    fn cli_parses_agent_command() {
        let cli = Cli::parse_from(["omni", "agent", "do something"]);
        match cli.command {
            Some(Commands::Agent { prompt, .. }) => {
                assert_eq!(prompt, "do something");
            }
            _ => panic!("expected Agent command"),
        }
    }

    #[test]
    fn cli_parses_agent_alias() {
        let cli = Cli::parse_from(["omni", "a", "do something"]);
        match cli.command {
            Some(Commands::Agent { prompt, .. }) => {
                assert_eq!(prompt, "do something");
            }
            _ => panic!("expected Agent command"),
        }
    }

    #[test]
    fn cli_parses_tui_command() {
        let cli = Cli::parse_from(["omni", "tui"]);
        assert!(matches!(cli.command, Some(Commands::Tui { .. })));
    }

    #[test]
    fn cli_parses_tui_continue() {
        let cli = Cli::parse_from(["omni", "tui", "--continue"]);
        match cli.command {
            Some(Commands::Tui {
                r#continue,
                session,
            }) => {
                assert!(r#continue);
                assert!(session.is_none());
            }
            _ => panic!("expected Tui command"),
        }
    }

    #[test]
    fn cli_parses_tui_session() {
        let cli = Cli::parse_from(["omni", "tui", "-s", "ses_123"]);
        match cli.command {
            Some(Commands::Tui {
                r#continue,
                session,
            }) => {
                assert!(!r#continue);
                assert_eq!(session, Some("ses_123".to_string()));
            }
            _ => panic!("expected Tui command"),
        }
    }

    #[test]
    fn cli_parses_agent_continue() {
        let cli = Cli::parse_from(["omni", "agent", "-c", "do more"]);
        match cli.command {
            Some(Commands::Agent {
                prompt,
                r#continue,
                session,
            }) => {
                assert_eq!(prompt, "do more");
                assert!(r#continue);
                assert!(session.is_none());
            }
            _ => panic!("expected Agent command"),
        }
    }

    #[test]
    fn cli_parses_serve_with_defaults() {
        let cli = Cli::parse_from(["omni", "serve"]);
        match cli.command {
            Some(Commands::Serve { host, port }) => {
                assert_eq!(host, "127.0.0.1");
                assert_eq!(port, 7890);
            }
            _ => panic!("expected Serve command"),
        }
    }

    #[test]
    fn cli_parses_serve_with_custom_host_port() {
        let cli = Cli::parse_from(["omni", "serve", "-H", "0.0.0.0", "-p", "8080"]);
        match cli.command {
            Some(Commands::Serve { host, port }) => {
                assert_eq!(host, "0.0.0.0");
                assert_eq!(port, 8080);
            }
            _ => panic!("expected Serve command"),
        }
    }

    #[test]
    fn cli_parses_config_show() {
        let cli = Cli::parse_from(["omni", "config", "show"]);
        match cli.command {
            Some(Commands::Config { command }) => {
                assert!(matches!(command, ConfigCommands::Show));
            }
            _ => panic!("expected Config command"),
        }
    }

    #[test]
    fn cli_parses_config_path() {
        let cli = Cli::parse_from(["omni", "config", "path"]);
        match cli.command {
            Some(Commands::Config { command }) => {
                assert!(matches!(command, ConfigCommands::Path));
            }
            _ => panic!("expected Config command"),
        }
    }

    #[test]
    fn cli_parses_config_generate_token() {
        let cli = Cli::parse_from(["omni", "config", "generate-token"]);
        match cli.command {
            Some(Commands::Config { command }) => {
                assert!(matches!(command, ConfigCommands::GenerateToken));
            }
            _ => panic!("expected Config command"),
        }
    }

    #[test]
    fn cli_verbose_is_global() {
        let cli = Cli::parse_from(["omni", "-v", "tui"]);
        assert_eq!(cli.verbose, 1);
        assert!(matches!(cli.command, Some(Commands::Tui { .. })));

        // Also works after subcommand
        let cli = Cli::parse_from(["omni", "tui", "-v"]);
        assert_eq!(cli.verbose, 1);
    }

    #[test]
    fn cli_debug_assert() {
        // Verify the CLI is correctly configured
        Cli::command().debug_assert();
    }

    #[test]
    fn cli_parses_session_list() {
        let cli = Cli::parse_from(["omni", "session", "list"]);
        match cli.command {
            Some(Commands::Session { command }) => {
                assert!(matches!(command, SessionCommands::List { .. }));
            }
            _ => panic!("expected Session command"),
        }
    }

    #[test]
    fn cli_parses_session_export() {
        let cli = Cli::parse_from(["omni", "session", "export", "abc123", "-f", "markdown"]);
        match cli.command {
            Some(Commands::Session { command }) => match command {
                SessionCommands::Export {
                    session_id,
                    format,
                    output,
                } => {
                    assert_eq!(session_id, "abc123");
                    assert_eq!(format, "markdown");
                    assert!(output.is_none());
                }
                _ => panic!("expected Export command"),
            },
            _ => panic!("expected Session command"),
        }
    }
}
