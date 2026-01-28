//! Secret masking for logs and output
//!
//! Prevents sensitive data from leaking in logs, error messages, and responses

use std::borrow::Cow;

use regex::Regex;

/// Patterns that match common secret formats
const SECRET_PATTERNS: &[(&str, &str)] = &[
    // API keys
    (r"sk-[a-zA-Z0-9]{20,}", "[MASKED_API_KEY]"),
    (r"sk-ant-[a-zA-Z0-9-]{20,}", "[MASKED_ANTHROPIC_KEY]"),
    (r"sk-proj-[a-zA-Z0-9-]{20,}", "[MASKED_OPENAI_KEY]"),
    // AWS
    (r"AKIA[0-9A-Z]{16}", "[MASKED_AWS_KEY]"),
    // GitHub
    (r"ghp_[a-zA-Z0-9]{36}", "[MASKED_GITHUB_TOKEN]"),
    (r"gho_[a-zA-Z0-9]{36}", "[MASKED_GITHUB_TOKEN]"),
    (r"ghu_[a-zA-Z0-9]{36}", "[MASKED_GITHUB_TOKEN]"),
    (r"ghs_[a-zA-Z0-9]{36}", "[MASKED_GITHUB_TOKEN]"),
    (r"ghr_[a-zA-Z0-9]{36}", "[MASKED_GITHUB_TOKEN]"),
    // Bearer tokens
    (r"(?i)bearer\s+[a-zA-Z0-9._-]+", "[MASKED_BEARER_TOKEN]"),
    // Private keys (match entire block including content)
    (r"-----BEGIN[A-Z ]+PRIVATE KEY-----[\s\S]*?-----END[A-Z ]+PRIVATE KEY-----", "[MASKED_PRIVATE_KEY]"),
    // JWT tokens
    (r"eyJ[a-zA-Z0-9_-]+\.eyJ[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+", "[MASKED_JWT]"),
    // Generic API key patterns (simpler)
    (r"(?i)api_key\s*=\s*[a-zA-Z0-9_-]{16,}", "[MASKED_SECRET]"),
    (r"(?i)secret\s*=\s*[a-zA-Z0-9_-]{16,}", "[MASKED_SECRET]"),
];

/// Compiled secret patterns for efficient matching
pub struct SecretMasker {
    patterns: Vec<(Regex, &'static str)>,
}

impl Default for SecretMasker {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretMasker {
    /// Create a new secret masker with default patterns
    #[must_use]
    pub fn new() -> Self {
        let patterns = SECRET_PATTERNS
            .iter()
            .filter_map(|(pattern, replacement)| {
                Regex::new(pattern).ok().map(|re| (re, *replacement))
            })
            .collect();

        Self { patterns }
    }

    /// Mask secrets in a string
    #[must_use]
    pub fn mask<'a>(&self, text: &'a str) -> Cow<'a, str> {
        let mut result = Cow::Borrowed(text);

        for (pattern, replacement) in &self.patterns {
            if pattern.is_match(&result) {
                result = Cow::Owned(pattern.replace_all(&result, *replacement).into_owned());
            }
        }

        result
    }

    /// Check if text contains any secrets
    #[must_use]
    pub fn contains_secret(&self, text: &str) -> bool {
        self.patterns.iter().any(|(pattern, _)| pattern.is_match(text))
    }
}

/// Global secret masker instance
static MASKER: std::sync::LazyLock<SecretMasker> = std::sync::LazyLock::new(SecretMasker::new);

/// Mask secrets in a string using the global masker
#[must_use]
pub fn mask_secrets(text: &str) -> Cow<'_, str> {
    MASKER.mask(text)
}

/// Check if text contains any secrets
#[must_use]
pub fn contains_secrets(text: &str) -> bool {
    MASKER.contains_secret(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_anthropic_key() {
        let text = "Using key sk-ant-api03-abcdefghijklmnop1234";
        let masked = mask_secrets(text);
        assert!(masked.contains("[MASKED_ANTHROPIC_KEY]"));
        assert!(!masked.contains("sk-ant-api03"));
    }

    #[test]
    fn mask_openai_key() {
        let text = "key=sk-proj-abcdefghij12345678901234";
        let masked = mask_secrets(text);
        assert!(masked.contains("[MASKED_OPENAI_KEY]"));
    }

    #[test]
    fn mask_github_token() {
        let text = "GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz1234567890";
        let masked = mask_secrets(text);
        assert!(masked.contains("[MASKED_GITHUB_TOKEN]"));
    }

    #[test]
    fn mask_aws_key() {
        let text = "aws_access_key_id=AKIAIOSFODNN7EXAMPLE";
        let masked = mask_secrets(text);
        assert!(masked.contains("[MASKED_AWS_KEY]"));
    }

    #[test]
    fn mask_jwt() {
        let text = "token: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let masked = mask_secrets(text);
        assert!(masked.contains("[MASKED_JWT]"));
    }

    #[test]
    fn mask_bearer_token() {
        let text = "Authorization: Bearer my-secret-token-12345";
        let masked = mask_secrets(text);
        assert!(masked.contains("[MASKED_BEARER_TOKEN]"));
    }

    #[test]
    fn mask_generic_api_key() {
        let text = "api_key = super_secret_key_12345678";
        let masked = mask_secrets(text);
        assert!(masked.contains("[MASKED_SECRET]"));
    }

    #[test]
    fn mask_private_key() {
        let text = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA...\n-----END RSA PRIVATE KEY-----";
        let masked = mask_secrets(text);
        assert!(masked.contains("[MASKED_PRIVATE_KEY]"));
        assert!(!masked.contains("MIIE")); // Content should be masked
    }

    #[test]
    fn no_mask_for_normal_text() {
        let text = "This is normal text without any secrets";
        let masked = mask_secrets(text);
        assert_eq!(masked, text);
    }

    #[test]
    fn contains_secret_detection() {
        assert!(contains_secrets("sk-ant-api03-abcdefghijklmnop1234"));
        assert!(!contains_secrets("just normal text"));
    }

    #[test]
    fn mask_multiple_secrets() {
        // ghp_ + 36 chars = 40 total
        let text = "key1=sk-ant-api03-abc1234567890123456 and key2=ghp_abcdefghijklmnopqrstuvwxyz1234567890";
        let masked = mask_secrets(text);
        assert!(masked.contains("[MASKED_ANTHROPIC_KEY]"));
        assert!(masked.contains("[MASKED_GITHUB_TOKEN]"));
    }
}
