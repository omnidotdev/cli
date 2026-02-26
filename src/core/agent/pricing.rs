//! Model-aware token pricing
//!
//! Provides per-model cost estimates instead of assuming a single hardcoded
//! price for every model. Unknown models and local providers resolve to zero

/// Per-million-token pricing for a model
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TokenPricing {
    /// Cost per million input tokens (USD)
    pub input_per_mtok: f64,
    /// Cost per million output tokens (USD)
    pub output_per_mtok: f64,
}

impl TokenPricing {
    /// Zero pricing (local or unknown models)
    pub const ZERO: Self = Self {
        input_per_mtok: 0.0,
        output_per_mtok: 0.0,
    };

    /// Calculate the total cost in USD for a given number of tokens
    #[must_use]
    pub fn cost(&self, input_tokens: u32, output_tokens: u32) -> f64 {
        f64::from(input_tokens).mul_add(
            self.input_per_mtok,
            f64::from(output_tokens) * self.output_per_mtok,
        ) / 1_000_000.0
    }
}

/// Local providers that never incur API costs
const LOCAL_PROVIDERS: &[&str] = &["ollama", "lmstudio"];

/// Look up pricing for a model/provider combination
///
/// Short-circuits to zero for local providers. For remote providers, uses
/// prefix matching on the model name. Unknown models resolve to
/// `TokenPricing::ZERO` rather than returning a potentially wrong estimate.
#[must_use]
pub fn lookup_pricing(model: &str, provider: &str) -> TokenPricing {
    // Local providers are always free
    if LOCAL_PROVIDERS
        .iter()
        .any(|p| provider.eq_ignore_ascii_case(p))
    {
        return TokenPricing::ZERO;
    }

    let model_lower = model.to_lowercase();

    // Anthropic
    if model_lower.contains("opus-4") || model_lower.contains("opus4") {
        return TokenPricing {
            input_per_mtok: 15.0,
            output_per_mtok: 75.0,
        };
    }
    if model_lower.contains("sonnet-4") || model_lower.contains("sonnet4") {
        return TokenPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
        };
    }
    if model_lower.contains("3-5-haiku")
        || model_lower.contains("3.5-haiku")
        || model_lower.contains("haiku-3")
    {
        return TokenPricing {
            input_per_mtok: 0.80,
            output_per_mtok: 4.0,
        };
    }
    if model_lower.contains("3-5-sonnet")
        || model_lower.contains("3.5-sonnet")
        || model_lower.contains("sonnet-3")
    {
        return TokenPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
        };
    }

    // OpenAI
    if model_lower.starts_with("o4-mini") {
        return TokenPricing {
            input_per_mtok: 1.10,
            output_per_mtok: 4.40,
        };
    }
    if model_lower.starts_with("o3-mini") {
        return TokenPricing {
            input_per_mtok: 1.10,
            output_per_mtok: 4.40,
        };
    }
    if model_lower.starts_with("o3") {
        return TokenPricing {
            input_per_mtok: 10.0,
            output_per_mtok: 40.0,
        };
    }
    if model_lower.starts_with("o1-mini") {
        return TokenPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 12.0,
        };
    }
    if model_lower.starts_with("o1") {
        return TokenPricing {
            input_per_mtok: 15.0,
            output_per_mtok: 60.0,
        };
    }
    if model_lower.starts_with("gpt-4o-mini") {
        return TokenPricing {
            input_per_mtok: 0.15,
            output_per_mtok: 0.60,
        };
    }
    if model_lower.starts_with("gpt-4o") {
        return TokenPricing {
            input_per_mtok: 2.50,
            output_per_mtok: 10.0,
        };
    }
    if model_lower.starts_with("gpt-4-turbo") {
        return TokenPricing {
            input_per_mtok: 10.0,
            output_per_mtok: 30.0,
        };
    }

    // Google
    if model_lower.contains("gemini") && model_lower.contains("flash") {
        return TokenPricing {
            input_per_mtok: 0.075,
            output_per_mtok: 0.30,
        };
    }
    if model_lower.contains("gemini") && model_lower.contains("pro") {
        return TokenPricing {
            input_per_mtok: 1.25,
            output_per_mtok: 5.0,
        };
    }

    // Mistral
    if model_lower.starts_with("mistral-large") {
        return TokenPricing {
            input_per_mtok: 2.0,
            output_per_mtok: 6.0,
        };
    }
    if model_lower.starts_with("codestral") {
        return TokenPricing {
            input_per_mtok: 0.30,
            output_per_mtok: 0.90,
        };
    }

    // Open-weight models (typically served locally or at zero marginal cost)
    if model_lower.starts_with("llama") || model_lower.starts_with("mixtral") {
        return TokenPricing::ZERO;
    }

    // Unknown model — zero is safer than a wrong estimate
    TokenPricing::ZERO
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_anthropic_models() {
        let p = lookup_pricing("claude-opus-4-20250514", "anthropic");
        assert_eq!(p.input_per_mtok, 15.0);
        assert_eq!(p.output_per_mtok, 75.0);

        let p = lookup_pricing("claude-sonnet-4-20250514", "anthropic");
        assert_eq!(p.input_per_mtok, 3.0);
        assert_eq!(p.output_per_mtok, 15.0);

        let p = lookup_pricing("claude-3-5-haiku-20241022", "anthropic");
        assert_eq!(p.input_per_mtok, 0.80);
        assert_eq!(p.output_per_mtok, 4.0);

        let p = lookup_pricing("claude-3-5-sonnet-20241022", "anthropic");
        assert_eq!(p.input_per_mtok, 3.0);
        assert_eq!(p.output_per_mtok, 15.0);
    }

    #[test]
    fn known_openai_models() {
        let p = lookup_pricing("gpt-4o", "openai");
        assert_eq!(p.input_per_mtok, 2.50);

        let p = lookup_pricing("gpt-4o-mini", "openai");
        assert_eq!(p.input_per_mtok, 0.15);

        let p = lookup_pricing("gpt-4-turbo", "openai");
        assert_eq!(p.input_per_mtok, 10.0);

        let p = lookup_pricing("o1", "openai");
        assert_eq!(p.input_per_mtok, 15.0);

        let p = lookup_pricing("o1-mini", "openai");
        assert_eq!(p.input_per_mtok, 3.0);

        let p = lookup_pricing("o3", "openai");
        assert_eq!(p.input_per_mtok, 10.0);

        let p = lookup_pricing("o3-mini", "openai");
        assert_eq!(p.input_per_mtok, 1.10);

        let p = lookup_pricing("o4-mini", "openai");
        assert_eq!(p.input_per_mtok, 1.10);
    }

    #[test]
    fn known_google_models() {
        let p = lookup_pricing("gemini-2.0-flash", "google");
        assert_eq!(p.input_per_mtok, 0.075);

        let p = lookup_pricing("gemini-1.5-pro", "google");
        assert_eq!(p.input_per_mtok, 1.25);
    }

    #[test]
    fn known_mistral_models() {
        let p = lookup_pricing("mistral-large-latest", "mistral");
        assert_eq!(p.input_per_mtok, 2.0);

        let p = lookup_pricing("codestral-latest", "mistral");
        assert_eq!(p.input_per_mtok, 0.30);
    }

    #[test]
    fn unknown_model_returns_zero() {
        let p = lookup_pricing("some-unknown-model", "some-provider");
        assert_eq!(p, TokenPricing::ZERO);
    }

    #[test]
    fn local_providers_return_zero() {
        let p = lookup_pricing("claude-sonnet-4-20250514", "ollama");
        assert_eq!(p, TokenPricing::ZERO);

        let p = lookup_pricing("gpt-4o", "lmstudio");
        assert_eq!(p, TokenPricing::ZERO);

        // Case-insensitive
        let p = lookup_pricing("anything", "Ollama");
        assert_eq!(p, TokenPricing::ZERO);
    }

    #[test]
    fn open_weight_models_are_zero() {
        let p = lookup_pricing("llama-3.1-70b", "together");
        assert_eq!(p, TokenPricing::ZERO);

        let p = lookup_pricing("mixtral-8x7b", "together");
        assert_eq!(p, TokenPricing::ZERO);
    }

    #[test]
    fn prefix_matching_works() {
        // Model IDs with date suffixes should still match
        let p = lookup_pricing("gpt-4o-2024-08-06", "openai");
        assert_eq!(p.input_per_mtok, 2.50);

        let p = lookup_pricing("o3-mini-2025-01-31", "openai");
        assert_eq!(p.input_per_mtok, 1.10);
    }

    #[test]
    fn cost_calculation() {
        let p = TokenPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
        };
        // 1000 input + 500 output
        let cost = p.cost(1000, 500);
        // (1000 * 3.0 + 500 * 15.0) / 1_000_000 = (3000 + 7500) / 1_000_000 = 0.0105
        assert!((cost - 0.0105).abs() < 1e-10);
    }

    #[test]
    fn zero_pricing_costs_nothing() {
        let cost = TokenPricing::ZERO.cost(100_000, 50_000);
        assert_eq!(cost, 0.0);
    }
}
