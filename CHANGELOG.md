# @omnidotdev/cli

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
