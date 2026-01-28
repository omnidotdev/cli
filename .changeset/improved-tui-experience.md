---
"@omnidotdev/cli": patch
---

Improve TUI experience

- Fix message cutoff by removing bottom padding and correcting height calculations
- Show activity status (e.g., "Using Bash...") instead of just "Thinking..."
- Add vertical and horizontal padding to user messages for better visual separation
- Lighten user message background color for a "previous message" look
- Implement smooth line-by-line scrolling for messages
- Allow typing while agent is responding (can prepare next message)
- Add rich diff rendering for all tool output (muted green/red for diff-like content)
- Fix paste behavior: pasted text now inserts without auto-submitting
