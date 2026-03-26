---
"@omnidotdev/cli": minor
---

Move shell mode from bare positional to explicit `omni shell` subcommand (alias: `omni sh`). This frees the positional space for plugin routing via `external_subcommand`, so `omni run up runa` works without quoting.

Migration: `omni "list files"` becomes `omni shell "list files"` or `omni sh "list files"`
