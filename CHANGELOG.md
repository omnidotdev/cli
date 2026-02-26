# Changelog

## 0.5.0

### Minor Changes

- [`7cf01b9`](https://github.com/omnidotdev/cli/commit/7cf01b9848be160c23d86f9b82d160c3d022c68d) Thanks [@coopbri](https://github.com/coopbri)! - Record AI token usage to Aether for billing after each LLM completion

- [`a94576a`](https://github.com/omnidotdev/cli/commit/a94576a72e195c80f6c5b23d174099fcd405a751) Thanks [@coopbri](https://github.com/coopbri)! - Add @file mention expansion to inline file contents as a fenced code block in the prompt

- [`7cf01b9`](https://github.com/omnidotdev/cli/commit/7cf01b9848be160c23d86f9b82d160c3d022c68d) Thanks [@coopbri](https://github.com/coopbri)! - Add `omni auth login` and `omni auth logout` commands for cloud Synapse authentication

- [`a94576a`](https://github.com/omnidotdev/cli/commit/a94576a72e195c80f6c5b23d174099fcd405a751) Thanks [@coopbri](https://github.com/coopbri)! - Add /compact slash command to summarize old messages and free context window space

- [`a94576a`](https://github.com/omnidotdev/cli/commit/a94576a72e195c80f6c5b23d174099fcd405a751) Thanks [@coopbri](https://github.com/coopbri)! - Add /cost slash command to display per-session token usage and estimated cost

- [`7cf01b9`](https://github.com/omnidotdev/cli/commit/7cf01b9848be160c23d86f9b82d160c3d022c68d) Thanks [@coopbri](https://github.com/coopbri)! - Add knowledge pack support with Manifold resolver, local caching, and tag-based chunk selection for persona system prompts

- [`11cd883`](https://github.com/omnidotdev/cli/commit/11cd883b1eff28980921697e70908650d6554daf) Thanks [@coopbri](https://github.com/coopbri)! - Add MCP STDIO reconnect-and-retry on transport failure, and animated braille spinner for tool calls in the TUI (inline block + status bar)

- [`9ece8de`](https://github.com/omnidotdev/cli/commit/9ece8de666d5775a608431f040565a8ca3869619) Thanks [@coopbri](https://github.com/coopbri)! - Add `multi_search` tool for parallel multi-source web search and TUI display for tool call activity

- [`65d45be`](https://github.com/omnidotdev/cli/commit/65d45be37e89c6b79f62d95b9ef9c539ff9accb8) Thanks [@coopbri](https://github.com/coopbri)! - Add Synapse client integration, agent-core provider registry, and Beacon browser tool

- [`a94576a`](https://github.com/omnidotdev/cli/commit/a94576a72e195c80f6c5b23d174099fcd405a751) Thanks [@coopbri](https://github.com/coopbri)! - Add /undo slash command to revert last agent file changes using shadow git snapshots

### Patch Changes

- [`1ddde64`](https://github.com/omnidotdev/cli/commit/1ddde64516fd8b698cbc41ada96b30407b721014) Thanks [@coopbri](https://github.com/coopbri)! - Fix hardcoded Sonnet pricing with model-aware cost lookup, add auth token extraction fallback chain, and add missing tests

## Unreleased

### Added

- `omni auth login` — authenticate with cloud Synapse using your Omni account email and password; stores the access token in `~/.config/omni/cli/config.toml`
- `omni auth logout` — clear the stored access token
- Cloud auth token is now used as the Bearer token for Synapse requests, enabling Omni Credits from the CLI without manual API key configuration
