---
"@omnidotdev/cli": patch
---

Route knowledge retrieval (embeddings and query condensing) through the Synapse gateway when you sign in with a Synapse API key, instead of sending that key to OpenAI. A Synapse key is now only ever sent to your Synapse gateway; an OpenAI key still goes directly to OpenAI. Semantic knowledge search also works when no `openai` provider is configured
