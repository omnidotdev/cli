# @omnidotdev/cli

## 0.3.0

### Minor Changes

- [`0783ebd`](https://github.com/omnidotdev/cli/commit/0783ebdfc88814b2c20aa517b2309685d5a7f776) Thanks [@coopbri](https://github.com/coopbri)! - Add unified LLM provider, session sharing, skill system, and LSP integration

  - Add unified LLM provider via `llm` crate supporting Anthropic, OpenAI, Google Gemini, Groq, and Mistral
  - Add session sharing with URL tokens, secrets, and optional TTL expiration
  - Add CLI commands: `omni session share` and `omni session unshare`
  - Add skill system for loading reusable instructions from SKILL.md files
  - Discover skills from `.omni/skill/`, `.opencode/skill/`, and `.claude/skills/` directories
  - Add `skill` tool for agents to load specialized instructions on demand
  - Add LSP integration with support for 13 language servers
  - Add `lsp` tool for code intelligence (hover, definition, references, symbols)
  - Add agent loop detection to prevent infinite tool call loops
  - Add secret masking for API keys, tokens, and credentials in tool output
  - Add MCP server integration to discover and execute external tools
  - Add plugin auto-discovery and integration into agent tool registry

- [`80a07eb`](https://github.com/omnidotdev/cli/commit/80a07ebd733a3c7a30041cb0c16c905698385291) Thanks [@coopbri](https://github.com/coopbri)! - Add GitHub integration tools

  - Add `github_pr` tool for creating pull requests
  - Add `github_issue` tool for creating, viewing, listing, and closing issues
  - Add `github_pr_review` tool for viewing PRs, diffs, checks, and adding comments
  - All tools use the `gh` CLI and respect plan mode restrictions

- [`46bedb3`](https://github.com/omnidotdev/cli/commit/46bedb3837e9f738a089d91a7fb9b335eecdf1ad) Thanks [@coopbri](https://github.com/coopbri)! - Add persistent memory system for cross-session context

  - Add `memory_add` tool for storing facts about user preferences, project patterns, and corrections
  - Add `memory_search` tool for finding relevant memories by query or category
  - Add `memory_delete` tool for removing outdated memories
  - Memories persist across sessions and can be injected into system prompts
  - Support for pinned memories that are always included in context

- [`08b5595`](https://github.com/omnidotdev/cli/commit/08b559543609df28f5341264209da8ccdada43a9) Thanks [@coopbri](https://github.com/coopbri)! - Add sandboxed execution tool

  - Add `sandbox_exec` tool for running untrusted code safely
  - Uses Docker containers with resource limits when available
  - Falls back to timeout + restricted env when Docker not present
  - Supports Python, Node, Ruby, and shell execution
  - Configurable timeout, network access, and workdir mounting

- [`761b54a`](https://github.com/omnidotdev/cli/commit/761b54a9c70079e23756dcea33d22b387c0a00ea) Thanks [@coopbri](https://github.com/coopbri)! - Add session continuity flags for resuming conversations

  - Add `--continue` / `-c` flag to resume most recent session
  - Add `--session` / `-s` flag to resume specific session by ID or slug
  - Works with both `tui` and `agent` commands
  - Session list now shows slugs for easier CLI use
  - Fix `session list` command using wrong storage path
  - Fail fast with clear error for invalid session IDs

- [`6e4f1ac`](https://github.com/omnidotdev/cli/commit/6e4f1ac1f4c38c26ebe5b4a856db60e4b4a7cb2e) Thanks [@coopbri](https://github.com/coopbri)! - Add new agent tools and LLM providers

  - Add `apply_patch` tool for applying unified diffs to files
  - Add `multi_edit` tool for editing multiple files in a single operation
  - Add support for Groq, Google Gemini, Mistral, LMStudio, OpenRouter, and Together AI providers

### Patch Changes

- [`761b54a`](https://github.com/omnidotdev/cli/commit/761b54a9c70079e23756dcea33d22b387c0a00ea) Thanks [@coopbri](https://github.com/coopbri)! - Wrap command palette selection at top/bottom for better UX

## 0.2.1

### Patch Changes

- [`ccaf64f`](https://github.com/omnidotdev/cli/commit/ccaf64f71c0410fefda4b7f1b8a7b084b58cfa69) Thanks [@coopbri](https://github.com/coopbri)! - Improve terminal compatibility and provider configuration UX

  - Fix key event handling for Termux and other terminals that don't report KeyEventKind correctly
  - Show provider-agnostic messages when no provider is configured
  - Update placeholder text to guide users toward configuration

- [`5820f20`](https://github.com/omnidotdev/cli/commit/5820f20c30130331be52342b49884aba782e50ae) Thanks [@coopbri](https://github.com/coopbri)! - Improve TUI experience

  - Fix message cutoff by removing bottom padding and correcting height calculations
  - Show activity status (e.g., "Using Bash...") instead of just "Thinking..."
  - Add vertical and horizontal padding to user messages for better visual separation
  - Lighten user message background color for a "previous message" look
  - Implement smooth line-by-line scrolling for messages
  - Allow typing while agent is responding (can prepare next message)
  - Add rich diff rendering for all tool output (muted green/red for diff-like content)
  - Fix paste behavior: pasted text now inserts without auto-submitting
  - Add Ctrl+Left/Right for word-by-word cursor movement

## 0.2.0

### Minor Changes

- [`04e91f8`](https://github.com/omnidotdev/cli/commit/04e91f8025a6bb48b9e65b2c085ead25b2cd29ec) Thanks [@coopbri](https://github.com/coopbri)! - Add model autocomplete dropdown with automatic provider switching

  - Type `/model ` to see available models with provider info
  - Arrow keys navigate, Tab/Enter to select
  - Automatically switches provider when model requires it (e.g., gpt-4o switches to OpenAI)

- [`7b14e8f`](https://github.com/omnidotdev/cli/commit/7b14e8f141e9c3a0b1596b331fa6ec2fb670c2e5) Thanks [@coopbri](https://github.com/coopbri)! - Add session management to TUI

  - Switch between sessions with Ctrl+S and Enter
  - Create new sessions with 'n' key
  - Delete sessions with 'd' key
  - Session list blocked while streaming

### Patch Changes

- [`770af7a`](https://github.com/omnidotdev/cli/commit/770af7a706a0185d908a6d9a343421bde29c5b03) Thanks [@coopbri](https://github.com/coopbri)! - Fix autoscroll, text wrapping, and model identity

  - Fix autoscroll during streaming responses
  - Fix text wrapping in tool output (was overflowing on long lines)
  - Fix model self-identification when switching models mid-conversation

- [`f87c14d`](https://github.com/omnidotdev/cli/commit/f87c14d355d53441fbc4a6949a786b5051b76642) Thanks [@coopbri](https://github.com/coopbri)! - Make clipboard support optional via `clipboard` feature flag for Termux/Android compatibility

## 0.1.0

### Minor Changes

- [`be9dba9`](https://github.com/omnidotdev/cli/commit/be9dba9b605f72e9e803a0b7cae75cf4ee1381cb) Thanks [@coopbri](https://github.com/coopbri)! - Opening the conduit
