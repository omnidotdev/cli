---
"@omnidotdev/cli": minor
---

Add persistent memory system for cross-session context

- Add `memory_add` tool for storing facts about user preferences, project patterns, and corrections
- Add `memory_search` tool for finding relevant memories by query or category
- Add `memory_delete` tool for removing outdated memories
- Memories persist across sessions and can be injected into system prompts
- Support for pinned memories that are always included in context
