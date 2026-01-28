---
"@omnidotdev/cli": minor
---

Add session continuity flags for resuming conversations

- Add `--continue` / `-c` flag to resume most recent session
- Add `--session` / `-s` flag to resume specific session by ID or slug
- Works with both `tui` and `agent` commands
- Session list now shows slugs for easier CLI use
- Fix `session list` command using wrong storage path
- Fail fast with clear error for invalid session IDs
