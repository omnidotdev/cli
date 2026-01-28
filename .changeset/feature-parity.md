---
"@omnidotdev/cli": minor
---

Add unified LLM provider, session sharing, skill system, and LSP integration

- Add unified LLM provider via `llm` crate supporting Anthropic, OpenAI, Google Gemini, Groq, and Mistral
- Add session sharing with URL tokens, secrets, and optional TTL expiration
- Add CLI commands: `omni session share` and `omni session unshare`
- Add skill system for loading reusable instructions from SKILL.md files
- Discover skills from `.omni/skill/`, `.opencode/skill/`, and `.claude/skills/` directories
- Add `skill` tool for agents to load specialized instructions on demand
- Add LSP integration with support for 13 language servers
- Add `lsp` tool for code intelligence (hover, definition, references, symbols)
