---
"@omnidotdev/cli": minor
---

`omni auth login` now connects the CLI to cloud Synapse with a Synapse API key (hidden prompt or stdin, checked against the gateway, stored on the `synapse` provider with owner-only file permissions) and makes Synapse the active provider. The old email and password sign-in stored a session token the gateway never accepted, so cloud usage could not work or bill; that flow and the unused bearer-token path are removed. `omni auth logout` removes the stored key
