---
"@omnidotdev/cli": minor
---

Remove the standalone `omni shell` subcommand in favor of a unified agent interface. Natural language shell tasks are now handled by `omni agent`, which routes simple shell commands through its existing tool system. This aligns with industry best practice (single agentic entry point) and reduces user-facing complexity.

Breaking change: `omni shell "list files"` is now `omni agent "list files"` (or `omni a "list files"`)
