use serde::{Deserialize, Serialize};

// ponytail: fixed 4 chars/token approximation; upgrade to tiktoken / bpe tokenizer when exact token counting is required.
pub const DEFAULT_RESERVED_OUTPUT_TOKENS: usize = 4_096;
pub const MIN_RESERVED_OUTPUT_TOKENS: usize = 1_024;
pub const CHARS_PER_TOKEN: usize = 4;
pub const MIN_INPUT_CHAR_BUDGET: usize = 5_000;

/// Reserve room for the model's output, but never more than 1/4 of the window.
/// Clamp so tiny-window models keep a sane, non-negative input budget.
///
/// Ported from `convertToLLMMessageService.ts:606-613`:
/// ```typescript
/// const reserveCap = Math.max(1_024, Math.floor(contextWindow / 4));
/// const desiredReserve = Math.max(reservedOutputTokenSpace ?? 4_096, 1_024);
/// reservedOutputTokenSpace = Math.min(desiredReserve, reserveCap);
/// ```
pub fn calculate_reserved_output_tokens(
    context_window: usize,
    reserved_output_token_space: Option<usize>,
) -> usize {
    let reserve_cap = (context_window / 4).max(MIN_RESERVED_OUTPUT_TOKENS);
    let desired_reserve = reserved_output_token_space
        .unwrap_or(DEFAULT_RESERVED_OUTPUT_TOKENS)
        .max(MIN_RESERVED_OUTPUT_TOKENS);
    desired_reserve.min(reserve_cap)
}

/// Calculate the available input character budget.
/// Never let this collapse below a small floor so a huge reserve on a tiny window
/// cannot force total truncation.
///
/// Ported from `convertToLLMMessageService.ts:681-684`:
/// ```typescript
/// const inputCharBudget = Math.max(
///     (contextWindow - reservedOutputTokenSpace) * CHARS_PER_TOKEN,
///     5_000
/// );
/// ```
pub fn calculate_input_char_budget(
    context_window: usize,
    reserved_output_token_space: Option<usize>,
) -> usize {
    let reserved = calculate_reserved_output_tokens(context_window, reserved_output_token_space);
    let available_tokens = context_window.saturating_sub(reserved);
    available_tokens
        .saturating_mul(CHARS_PER_TOKEN)
        .max(MIN_INPUT_CHAR_BUDGET)
}

/// Token & character budget for an LLM context window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextBudget {
    pub context_window: usize,
    pub reserved_output_tokens: usize,
    pub available_input_tokens: usize,
    pub input_char_budget: usize,
}

impl ContextBudget {
    pub fn calculate(context_window: usize, reserved_output_token_space: Option<usize>) -> Self {
        let reserved_output_tokens =
            calculate_reserved_output_tokens(context_window, reserved_output_token_space);
        let available_input_tokens = context_window.saturating_sub(reserved_output_tokens);
        let input_char_budget = available_input_tokens
            .saturating_mul(CHARS_PER_TOKEN)
            .max(MIN_INPUT_CHAR_BUDGET);

        Self {
            context_window,
            reserved_output_tokens,
            available_input_tokens,
            input_char_budget,
        }
    }

    pub fn is_within_budget(&self, char_count: usize) -> bool {
        char_count <= self.input_char_budget
    }

    pub fn remaining_chars(&self, current_chars: usize) -> usize {
        self.input_char_budget.saturating_sub(current_chars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_context_window_default_reserve() {
        // 32,000 window: reserveCap = max(1024, 8000) = 8000. Desired = 4096. Reserved = 4096.
        let budget = ContextBudget::calculate(32_000, None);
        assert_eq!(budget.reserved_output_tokens, 4_096);
        assert_eq!(budget.available_input_tokens, 27_904);
        assert_eq!(budget.input_char_budget, 27_904 * 4);
        assert!(budget.is_within_budget(50_000));
        assert!(!budget.is_within_budget(200_000));
    }

    #[test]
    fn test_large_context_window_custom_reserve() {
        // 200,000 window, requested 64,000. Cap = max(1024, 50,000) = 50,000.
        // Reserved clamped to cap = 50,000.
        let budget = ContextBudget::calculate(200_000, Some(64_000));
        assert_eq!(budget.reserved_output_tokens, 50_000);
        assert_eq!(budget.available_input_tokens, 150_000);
        assert_eq!(budget.input_char_budget, 150_000 * 4);
    }

    #[test]
    fn test_tiny_context_window_clamped_to_floor() {
        // 2,000 window: reserveCap = max(1024, 500) = 1024. Desired = 4096. Reserved = 1024.
        let budget = ContextBudget::calculate(2_000, None);
        assert_eq!(budget.reserved_output_tokens, 1_024);
        assert_eq!(budget.available_input_tokens, 976);
        // (976 * 4) = 3904 < 5000 -> floor of 5,000 applies
        assert_eq!(budget.input_char_budget, 5_000);
    }

    #[test]
    fn test_zero_or_very_small_window() {
        let budget = ContextBudget::calculate(500, None);
        assert_eq!(budget.reserved_output_tokens, 1_024);
        assert_eq!(budget.available_input_tokens, 0);
        assert_eq!(budget.input_char_budget, 5_000);
    }

    #[test]
    fn test_desired_reserve_smaller_than_min() {
        // Requested 500 tokens, min reserve floor is 1024
        let budget = ContextBudget::calculate(32_000, Some(500));
        assert_eq!(budget.reserved_output_tokens, 1_024);
    }
}
