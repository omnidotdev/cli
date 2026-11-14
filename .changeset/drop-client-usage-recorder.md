---
"@omnidotdev/cli": minor
---

Remove the client-side Aether usage recorder. The CLI no longer reads `AETHER_URL`, `AETHER_APP_ID`, or `AETHER_SERVICE_API_KEY` or posts usage costs: a client never holds a service key, and the Synapse gateway meters and bills AI usage. The status bar and `/cost` now label the session cost as a list-price estimate, not the bill
