//! Automatic project context gathering for AI agents.
//!
//! Gathers environment, git, project, and instruction context to inject into
//! the system prompt, providing the agent with awareness of the local codebase.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

/// Gathered project context for injection into system prompts.
#[derive(Debug, Clone, Default)]
pub struct ProjectContext {
    /// Current working directory.
    pub working_dir: PathBuf,
    /// Platform (linux, macos, windows).
    pub platform: String,
    /// Current date.
    pub date: String,
    /// Whether this is a git repository.
    pub is_git_repo: bool,
    /// Current git branch.
    pub git_branch: Option<String>,
    /// Git status summary (modified, untracked, staged counts).
    pub git_status: Option<GitStatus>,
    /// Detected project type.
    pub project_type: Option<ProjectType>,
    /// Instruction content from CLAUDE.md files.
    pub instructions: Vec<InstructionSource>,
    /// File tree of the project (depth-limited).
    pub file_tree: Option<String>,
}

/// Git repository status.
#[derive(Debug, Clone, Default)]
pub struct GitStatus {
    /// Number of modified files.
    pub modified: usize,
    /// Number of untracked files.
    pub untracked: usize,
    /// Number of staged files.
    pub staged: usize,
    /// Recent commit summaries (last 5).
    pub recent_commits: Vec<String>,
}

/// Detected project type.
#[derive(Debug, Clone)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Unknown,
}

/// Source of instruction content.
#[derive(Debug, Clone)]
pub struct InstructionSource {
    /// Path to the instruction file.
    pub path: PathBuf,
    /// Content of the file.
    pub content: String,
}

impl ProjectContext {
    /// Gather all project context from the current directory.
    #[must_use]
    pub fn gather() -> Self {
        let working_dir = std::env::current_dir().unwrap_or_default();
        Self::gather_from(&working_dir)
    }

    /// Gather project context from a specific directory.
    #[must_use]
    pub fn gather_from(dir: &Path) -> Self {
        let platform = detect_platform();
        let date = format_date();
        let is_git_repo = is_git_repository(dir);
        let git_branch = if is_git_repo {
            get_git_branch(dir)
        } else {
            None
        };
        let git_status = if is_git_repo {
            Some(get_git_status(dir))
        } else {
            None
        };
        let project_type = detect_project_type(dir);
        let instructions = gather_instructions(dir);
        let file_tree = generate_file_tree(dir, 3); // depth 3

        Self {
            working_dir: dir.to_path_buf(),
            platform,
            date,
            is_git_repo,
            git_branch,
            git_status,
            project_type,
            instructions,
            file_tree,
        }
    }

    /// Build a context string for injection into system prompts.
    #[must_use]
    pub fn to_prompt_context(&self) -> String {
        let mut parts = Vec::new();

        // Environment section
        parts.push(format!(
            "<environment>\nWorking directory: {}\nPlatform: {}\nDate: {}\nGit repository: {}\n</environment>",
            self.working_dir.display(),
            self.platform,
            self.date,
            if self.is_git_repo { "yes" } else { "no" }
        ));

        // Git section
        if let Some(branch) = &self.git_branch {
            let mut git_info = format!("<git>\nBranch: {branch}");

            if let Some(status) = &self.git_status {
                if status.modified > 0 || status.untracked > 0 || status.staged > 0 {
                    let _ = write!(
                        git_info,
                        "\nStatus: {} modified, {} untracked, {} staged",
                        status.modified, status.untracked, status.staged
                    );
                }

                if !status.recent_commits.is_empty() {
                    git_info.push_str("\n\nRecent commits:");
                    for commit in &status.recent_commits {
                        let _ = write!(git_info, "\n  {commit}");
                    }
                }
            }

            git_info.push_str("\n</git>");
            parts.push(git_info);
        }

        // Project type section
        if let Some(project_type) = &self.project_type {
            let type_str = match project_type {
                ProjectType::Rust => "Rust (Cargo)",
                ProjectType::Node => "Node.js (npm/bun)",
                ProjectType::Python => "Python",
                ProjectType::Go => "Go",
                ProjectType::Unknown => "Unknown",
            };
            parts.push(format!("<project-type>{type_str}</project-type>"));
        }

        // File tree section
        if let Some(tree) = &self.file_tree {
            parts.push(format!("<file-tree>\n{tree}</file-tree>"));
        }

        // Instructions section
        for instruction in &self.instructions {
            parts.push(format!(
                "<instructions source=\"{}\">\n{}\n</instructions>",
                instruction.path.display(),
                instruction.content.trim()
            ));
        }

        parts.join("\n\n")
    }
}

/// Format the current date as YYYY-MM-DD.
fn format_date() -> String {
    // Use the date command for simplicity, fallback to epoch-based calculation
    if let Ok(output) = Command::new("date").arg("+%Y-%m-%d").output() {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }

    // Fallback: calculate from system time (rough approximation)
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let days = duration.as_secs() / 86400;
    let years = 1970 + days / 365;
    format!("{years}-01-01")
}

/// Detect the current platform.
fn detect_platform() -> String {
    if cfg!(target_os = "linux") {
        "linux".to_string()
    } else if cfg!(target_os = "macos") {
        "macos".to_string()
    } else if cfg!(target_os = "windows") {
        "windows".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Check if a directory is inside a git repository.
fn is_git_repository(dir: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(dir)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get the current git branch.
fn get_git_branch(dir: &Path) -> Option<String> {
    Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Get git repository status.
fn get_git_status(dir: &Path) -> GitStatus {
    let mut status = GitStatus::default();

    // Get modified files count
    if let Ok(output) = Command::new("git")
        .args(["diff", "--name-only"])
        .current_dir(dir)
        .output()
    {
        if output.status.success() {
            status.modified = String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .count();
        }
    }

    // Get untracked files count
    if let Ok(output) = Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(dir)
        .output()
    {
        if output.status.success() {
            status.untracked = String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .count();
        }
    }

    // Get staged files count
    if let Ok(output) = Command::new("git")
        .args(["diff", "--staged", "--name-only"])
        .current_dir(dir)
        .output()
    {
        if output.status.success() {
            status.staged = String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .count();
        }
    }

    // Get recent commits
    if let Ok(output) = Command::new("git")
        .args(["log", "--oneline", "-5"])
        .current_dir(dir)
        .output()
    {
        if output.status.success() {
            status.recent_commits = String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect();
        }
    }

    status
}

/// Detect the project type based on manifest files.
fn detect_project_type(dir: &Path) -> Option<ProjectType> {
    // Walk up the directory tree to find project root
    let mut current = dir.to_path_buf();
    loop {
        if current.join("Cargo.toml").exists() {
            return Some(ProjectType::Rust);
        }
        if current.join("package.json").exists() {
            return Some(ProjectType::Node);
        }
        if current.join("pyproject.toml").exists()
            || current.join("setup.py").exists()
            || current.join("requirements.txt").exists()
        {
            return Some(ProjectType::Python);
        }
        if current.join("go.mod").exists() {
            return Some(ProjectType::Go);
        }

        if !current.pop() {
            break;
        }
    }
    None
}

/// Gather instruction files (CLAUDE.md) from the directory tree.
fn gather_instructions(dir: &Path) -> Vec<InstructionSource> {
    let mut instructions = Vec::new();
    let instruction_files = ["CLAUDE.md", "AGENTS.md"];

    // First, check global ~/.claude/CLAUDE.md
    if let Ok(home) = std::env::var("HOME") {
        let home_path = PathBuf::from(home);
        let global_claude = home_path.join(".claude").join("CLAUDE.md");
        if global_claude.exists() {
            if let Ok(content) = std::fs::read_to_string(&global_claude) {
                instructions.push(InstructionSource {
                    path: global_claude,
                    content,
                });
            }
        }
    }

    // Walk up the directory tree collecting instruction files
    let mut current = dir.to_path_buf();
    let mut found_paths = Vec::new();

    loop {
        for filename in &instruction_files {
            let path = current.join(filename);
            if path.exists() && !found_paths.contains(&path) {
                found_paths.push(path);
            }
        }

        if !current.pop() {
            break;
        }
    }

    // Reverse so that root instructions come first, local overrides last
    found_paths.reverse();

    for path in found_paths {
        if let Ok(content) = std::fs::read_to_string(&path) {
            instructions.push(InstructionSource { path, content });
        }
    }

    instructions
}

/// Generate a file tree of the project using git ls-files or find.
fn generate_file_tree(dir: &Path, max_depth: usize) -> Option<String> {
    // Try git ls-files first (respects .gitignore)
    if let Ok(output) = Command::new("git")
        .args(["ls-files"])
        .current_dir(dir)
        .output()
    {
        if output.status.success() {
            let files = String::from_utf8_lossy(&output.stdout);
            let tree = build_tree_from_files(&files, max_depth);
            if !tree.is_empty() {
                return Some(tree);
            }
        }
    }

    // Fallback to find (limited depth)
    if let Ok(output) = Command::new("find")
        .args([
            ".",
            "-maxdepth",
            &max_depth.to_string(),
            "-type",
            "f",
            "-not",
            "-path",
            "./.git/*",
            "-not",
            "-path",
            "./target/*",
            "-not",
            "-path",
            "./node_modules/*",
        ])
        .current_dir(dir)
        .output()
    {
        if output.status.success() {
            let files = String::from_utf8_lossy(&output.stdout);
            let tree = build_tree_from_files(&files, max_depth);
            if !tree.is_empty() {
                return Some(tree);
            }
        }
    }

    None
}

/// Build a tree representation from a list of file paths.
fn build_tree_from_files(files: &str, max_depth: usize) -> String {
    use std::collections::BTreeSet;

    let mut dirs: BTreeSet<String> = BTreeSet::new();
    let mut file_list: BTreeSet<String> = BTreeSet::new();

    for line in files.lines() {
        let line = line.trim().trim_start_matches("./");
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split('/').collect();
        let depth = parts.len();

        if depth > max_depth {
            // Only show up to max_depth
            let truncated: Vec<&str> = parts.iter().take(max_depth).copied().collect();
            dirs.insert(truncated.join("/"));
        } else {
            file_list.insert(line.to_string());
            // Add parent directories
            for i in 1..parts.len() {
                dirs.insert(parts[..i].join("/"));
            }
        }
    }

    // Build output showing directories and files
    let mut output = Vec::new();

    for item in &file_list {
        output.push(item.clone());
    }

    // Limit output size
    if output.len() > 100 {
        output.truncate(100);
        output.push("... (truncated)".to_string());
    }

    output.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_platform_returns_valid() {
        let platform = detect_platform();
        assert!(["linux", "macos", "windows", "unknown"].contains(&platform.as_str()));
    }

    #[test]
    fn gather_context_works() {
        let ctx = ProjectContext::gather();
        assert!(!ctx.working_dir.as_os_str().is_empty());
        assert!(!ctx.platform.is_empty());
        assert!(!ctx.date.is_empty());
    }
}
