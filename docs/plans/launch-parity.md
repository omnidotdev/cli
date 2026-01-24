# Launch Parity with OpenCode

Tracking feature parity needed before launch.

## High Priority (Core Functionality)

- [x] **Edit Tool** - Inline file modifications with diffs (not full file overwrites)
- [x] **Glob Tool** - File pattern matching for codebase exploration
- [x] **Grep Tool** - Content search with regex support
- [x] **List Tool** - Directory listing tool
- [x] **Session Compaction** - Auto-summarize older messages when context is full
- [x] **Session Export** - Export sessions to JSON/markdown (CLI: `omni session export`)

## Medium Priority (UX Polish)

- [ ] **Customizable Keybinds** - Keyboard shortcut configuration
- [ ] **More Providers** - Azure, Bedrock, Groq, Together, etc.
- [x] **Web Fetch Tool** - Fetch and process web pages
- [x] **Per-Project Config** - `.omni/config.toml` support
- [x] **Todo/Task Tools** - Agent self-tracking tools
- [x] **Title Auto-generation** - Generate session titles from first message
- [x] **Usage/Cost Tracking** - Display token usage and cost in status bar

## Lower Priority (Post-Launch)

- [ ] **MCP OAuth** - OAuth flow for MCP servers
- [ ] **Custom Agents via Markdown** - Agent definitions in config files
- [ ] **Hooks System** - file_edited, session_completed triggers
- [ ] **Themes** - Visual customization
- [ ] **LSP Integration** - Language server support
- [ ] **Batch Tool** - Parallel tool execution
