---
"@omnidotdev/cli": patch
---

Fix autoscroll, text wrapping, and model identity

- Fix autoscroll during streaming responses
- Fix text wrapping in tool output (was overflowing on long lines)
- Fix model self-identification when switching models mid-conversation
