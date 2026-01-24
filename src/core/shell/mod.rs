//! Natural language to shell command translation.

mod whitelist;

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use futures::StreamExt;

use crate::core::agent::{
    AgentError, CompletionEvent, CompletionRequest, Content, LlmProvider, Message, Result, Role,
};

pub use whitelist::is_whitelisted;

/// Marker returned by the LLM when the prompt is not a shell task.
const NOT_A_SHELL_TASK: &str = "NOT_A_SHELL_TASK";

/// System prompt for shell command generation.
const SHELL_SYSTEM_PROMPT: &str = r#"You are a shell command generator. Convert natural language to executable shell commands.

Rules:
- Output ONLY the command, nothing else
- No explanations, no markdown, no code fences
- Use standard POSIX-compatible commands when possible
- Prefer simple, readable commands over clever one-liners
- Use the user's current working directory context
- If the request is ambiguous, make a reasonable assumption

If the request is clearly NOT a shell task (e.g., "explain this code", "write a function", "review my PR"), respond with exactly:
NOT_A_SHELL_TASK

Examples:
- "list files" → ls -la
- "find large files" → find . -type f -size +100M
- "what's using port 3000" → lsof -i :3000
- "disk usage by folder" → du -sh */ | sort -hr
- "explain recursion" → NOT_A_SHELL_TASK"#;

/// Result of shell command generation.
#[derive(Debug)]
pub enum ShellResult {
    /// A command was generated.
    Command(String),
    /// The prompt is not a shell task.
    NotAShellTask,
}

/// Result of command execution.
#[derive(Debug)]
pub struct ExecutionResult {
    /// The exit code of the command.
    pub exit_code: i32,
    /// The combined stdout/stderr output.
    pub output: String,
}

/// Generate a shell command from a natural language prompt.
///
/// # Errors
///
/// Returns an error if the LLM call fails.
pub async fn generate_command(
    provider: &dyn LlmProvider,
    model: &str,
    prompt: &str,
) -> Result<ShellResult> {
    let cwd = std::env::current_dir().map_or_else(|_| ".".to_string(), |p| p.display().to_string());

    let user_message = format!("Current directory: {cwd}\n\nRequest: {prompt}");

    let request = CompletionRequest {
        model: model.to_string(),
        max_tokens: 256,
        messages: vec![Message {
            role: Role::User,
            content: Content::Text(user_message),
        }],
        system: Some(SHELL_SYSTEM_PROMPT.to_string()),
        tools: None,
    };

    let stream = provider.stream(request).await?;
    futures::pin_mut!(stream);

    let mut response = String::new();

    while let Some(event) = stream.next().await {
        let event = event?;
        if let CompletionEvent::TextDelta(text) = event {
            response.push_str(&text);
        }
    }

    let response = response.trim();

    if response == NOT_A_SHELL_TASK {
        Ok(ShellResult::NotAShellTask)
    } else {
        // Clean up any markdown formatting if the model adds it
        let command = response
            .trim_start_matches("```bash")
            .trim_start_matches("```sh")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        Ok(ShellResult::Command(command.to_string()))
    }
}

/// Execute a shell command and stream output.
///
/// # Errors
///
/// Returns an error if the command fails to spawn.
///
/// # Panics
///
/// Panics if stdout or stderr cannot be captured (should not happen with `Stdio::piped()`).
pub fn execute_command(command: &str) -> std::io::Result<ExecutionResult> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().expect("stdout should be captured");
    let stderr = child.stderr.take().expect("stderr should be captured");

    let mut output = String::new();

    // Read stdout and stderr interleaved (simplified: read stdout then stderr)
    let stdout_reader = BufReader::new(stdout);
    for line in stdout_reader.lines() {
        let line = line?;
        println!("{line}");
        output.push_str(&line);
        output.push('\n');
    }

    let stderr_reader = BufReader::new(stderr);
    for line in stderr_reader.lines() {
        let line = line?;
        eprintln!("{line}");
        output.push_str(&line);
        output.push('\n');
    }

    let status = child.wait()?;
    let exit_code = status.code().unwrap_or(-1);

    Ok(ExecutionResult { exit_code, output })
}

/// Prompt the user for confirmation.
///
/// Returns true if the user confirms.
#[must_use]
pub fn prompt_confirmation() -> bool {
    print!("Execute? [y/N] ");
    std::io::stdout().flush().ok();

    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_ok() {
        let input = input.trim().to_lowercase();
        input == "y" || input == "yes"
    } else {
        false
    }
}

/// Run the shell mode flow.
///
/// # Errors
///
/// Returns an error if command generation or execution fails.
pub async fn run(
    provider: Box<dyn LlmProvider>,
    model: &str,
    prompt: &str,
    skip_confirmation: bool,
    dry_run: bool,
) -> Result<()> {
    // Generate command
    let result = generate_command(provider.as_ref(), model, prompt).await?;

    match result {
        ShellResult::NotAShellTask => {
            println!("This looks like a coding task. Try: omni agent \"{prompt}\"");
        }
        ShellResult::Command(command) => {
            println!("Command: {command}");
            println!();

            if dry_run {
                return Ok(());
            }

            let should_execute = if skip_confirmation || is_whitelisted(&command) {
                true
            } else {
                prompt_confirmation()
            };

            if should_execute {
                match execute_command(&command) {
                    Ok(result) => {
                        if result.exit_code != 0 {
                            eprintln!("\n[exit code {}]", result.exit_code);
                        }
                    }
                    Err(e) => {
                        return Err(AgentError::ToolExecution(format!("Failed to execute: {e}")));
                    }
                }
            } else {
                println!("Cancelled.");
            }
        }
    }

    Ok(())
}
