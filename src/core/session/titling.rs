//! Auto-titling for sessions
//!
//! Generates concise titles from the first user message

use super::{Session, SessionManager};

/// Maximum title length
pub const MAX_TITLE_LENGTH: usize = 60;

impl SessionManager {
    /// Update session title
    ///
    /// # Errors
    ///
    /// Returns error if update fails
    pub fn set_session_title(&self, session_id: &str, title: &str) -> anyhow::Result<Session> {
        // Truncate if too long
        let title = if title.len() > MAX_TITLE_LENGTH {
            format!("{}...", &title[..MAX_TITLE_LENGTH - 3])
        } else {
            title.to_string()
        };

        Ok(self.storage().update(
            &["session", &self.project().id, session_id],
            |s: &mut Session| {
                s.title = title.clone();
            },
        )?)
    }
}

/// Generate a title prompt for the LLM
#[must_use]
pub fn titling_prompt(first_message: &str) -> String {
    format!(
        r"Generate a concise title (max 50 chars) for this conversation based on the first message.
Return ONLY the title, no quotes or explanation.

First message:
{first_message}

Title:"
    )
}

/// Extract a title from LLM response (cleans up any quotes or extra whitespace)
#[must_use]
pub fn extract_title(response: &str) -> String {
    response
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .lines()
        .next()
        .unwrap_or(response.trim())
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_title_removes_quotes() {
        assert_eq!(extract_title("\"My Title\""), "My Title");
        assert_eq!(extract_title("'My Title'"), "My Title");
    }

    #[test]
    fn extract_title_handles_multiline() {
        assert_eq!(extract_title("First Line\nSecond Line"), "First Line");
    }

    #[test]
    fn extract_title_trims_whitespace() {
        assert_eq!(extract_title("  My Title  \n"), "My Title");
    }
}
