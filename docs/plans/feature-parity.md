# Feature Parity Roadmap

## Completed

1. **Session Persistence** - Multiple named sessions with message/part hierarchy
2. **Storage Module** - JSON file-based key-value storage
3. **Project Detection** - Git-based project identification
4. **Compaction Infrastructure** - Context overflow handling
5. **Auto-titling Infrastructure** - LLM-generated session titles
6. **Session Export** - JSON and Markdown export
7. **TUI Session List** - Browse and switch sessions (Ctrl+S)

## Priority Queue

### High Priority

8. **MCP Support** - Model Context Protocol for extensible tools ✓ (basic)
   - Tool server connections ✓
   - Resource providers (TODO)
   - Prompt templates (TODO)

9. **Shell Mode** - Natural language shell command execution ✓
   - Command generation from natural language
   - Whitelist for safe commands
   - CLI integration

10. **Snapshot/Checkpoint** - Save and restore conversation state ✓
    - Shadow git repository for tracking
    - Rollback and revert capability

### Medium Priority

11. **File Watching** - React to file changes ✓
    - Notify-based watcher with gitignore support
    - Configurable ignore patterns

12. **Git Worktree** - Isolated workspaces ✓
    - Create, remove, reset worktrees
    - Random name generation

13. **Plugin System** - Extensible capabilities ✓
    - Trait-based plugin hooks
    - Custom tools via plugins

### Lower Priority

14. **TTS/STT** - Voice interface
    - Speech-to-text input
    - Text-to-speech output

15. **IDE Integration** - Editor plugins
    - VS Code extension
    - Neovim plugin

16. **Share** - Session sharing
    - Export to gist
    - Collaborative sessions
