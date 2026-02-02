---
"@omnidotdev/cli": minor
---

Add Kimi 2.5 (Moonshot AI) provider support

- Add `kimi` provider with OpenAI-compatible API at `api.moonshot.cn`
- Add models: `kimi-k2.5`, `moonshot-v1-128k`, `moonshot-v1-32k`
- Add prefix detection for `kimi-*` and `moonshot-*` model names
- Set `MOONSHOT_API_KEY` environment variable to use
