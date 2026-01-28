---
"@omnidotdev/cli": minor
---

Add sandboxed execution tool

- Add `sandbox_exec` tool for running untrusted code safely
- Uses Docker containers with resource limits when available
- Falls back to timeout + restricted env when Docker not present
- Supports Python, Node, Ruby, and shell execution
- Configurable timeout, network access, and workdir mounting
