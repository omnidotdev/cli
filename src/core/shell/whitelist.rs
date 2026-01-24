//! Safe command whitelist for auto-execution.

/// Commands that are safe to auto-execute without confirmation.
const SAFE_COMMANDS: &[&str] = &[
    // Filesystem (read-only)
    "ls",
    "find",
    "cat",
    "head",
    "tail",
    "less",
    "more",
    "file",
    "stat",
    "wc",
    "du",
    "df",
    "tree",
    "realpath",
    "dirname",
    "basename",
    // Search
    "grep",
    "rg",
    "ag",
    "fd",
    "locate",
    "which",
    "whereis",
    "type",
    // Process info
    "ps",
    "top",
    "htop",
    "btop",
    "pgrep",
    "lsof",
    "pstree",
    // System info
    "uname",
    "hostname",
    "whoami",
    "id",
    "date",
    "uptime",
    "free",
    "env",
    "printenv",
    "arch",
    "nproc",
    "getconf",
    // Network (read-only)
    "ping",
    "curl",
    "wget",
    "dig",
    "nslookup",
    "host",
    "netstat",
    "ss",
    "ip",
    "ifconfig",
    "traceroute",
    "mtr",
    // Git (read-only)
    "git status",
    "git log",
    "git diff",
    "git branch",
    "git show",
    "git ls-files",
    "git remote",
    "git tag",
    "git stash list",
    "git shortlog",
    "git blame",
    "git rev-parse",
    // Dev tools (read-only)
    "cargo check",
    "cargo test",
    "cargo clippy",
    "cargo fmt --check",
    "cargo doc",
    "cargo tree",
    "cargo metadata",
    "npm test",
    "npm run test",
    "bun test",
    "bun run test",
    "go test",
    "python -m pytest",
    "pytest",
    "rustc --version",
    "node --version",
    "npm --version",
    "bun --version",
    "go version",
    "python --version",
    // Misc read-only
    "echo",
    "printf",
    "true",
    "false",
    "test",
    "expr",
    "seq",
    "sort",
    "uniq",
    "cut",
    "tr",
    "awk",
    "sed",
    "jq",
    "yq",
    "xargs",
    "tee",
];

/// Check if a command is whitelisted for auto-execution.
///
/// Returns true if the command starts with any whitelisted prefix.
/// Handles pipes by checking the first command in the pipeline.
#[must_use]
pub fn is_whitelisted(command: &str) -> bool {
    let command = command.trim();

    // Handle pipes: check first command in pipeline
    let first_command = command.split('|').next().unwrap_or(command).trim();

    // Check if it starts with any whitelisted command
    SAFE_COMMANDS.iter().any(|safe| {
        // Exact match or starts with safe command followed by space/end
        first_command == *safe
            || first_command.starts_with(&format!("{safe} "))
            || first_command.starts_with(&format!("{safe}\t"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelists_simple_commands() {
        assert!(is_whitelisted("ls"));
        assert!(is_whitelisted("ls -la"));
        assert!(is_whitelisted("find . -name '*.rs'"));
        assert!(is_whitelisted("cat file.txt"));
        assert!(is_whitelisted("git status"));
        assert!(is_whitelisted("git log --oneline -10"));
    }

    #[test]
    fn whitelists_piped_commands() {
        assert!(is_whitelisted("ls -la | grep foo"));
        assert!(is_whitelisted("find . -name '*.rs' | wc -l"));
        assert!(is_whitelisted("cat file.txt | head -10"));
        assert!(is_whitelisted("ps aux | grep node"));
    }

    #[test]
    fn rejects_dangerous_commands() {
        assert!(!is_whitelisted("rm -rf /"));
        assert!(!is_whitelisted("rm file.txt"));
        assert!(!is_whitelisted("mv a b"));
        assert!(!is_whitelisted("chmod 777 file"));
        assert!(!is_whitelisted("sudo anything"));
        assert!(!is_whitelisted("dd if=/dev/zero"));
    }

    #[test]
    fn rejects_partial_matches() {
        // "lsof" should match, but "lsofx" should not
        assert!(is_whitelisted("lsof -i :3000"));
        assert!(!is_whitelisted("lsofx"));

        // "cat" should match, but "catalog" should not
        assert!(is_whitelisted("cat file.txt"));
        assert!(!is_whitelisted("catalog"));
    }

    #[test]
    fn handles_whitespace() {
        assert!(is_whitelisted("  ls -la  "));
        assert!(is_whitelisted("ls\t-la"));
    }
}
