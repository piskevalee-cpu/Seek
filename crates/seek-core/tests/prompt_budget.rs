//! The prompt budget test (SPEC 9.2).
//!
//! The system prompt plus the default tool schemas must not exceed 1,500 tokens. A test that
//! silently drifts above the budget is the whole point of this file, so the count is asserted
//! against a real tokenizer rather than an estimate.

use seek_core::prompt::{DEFAULT_TOOLS, SYSTEM_PROMPT, default_prompt_prefix};
use tiktoken_rs::cl100k_base;

/// The limit from SPEC 9.2.
const BUDGET: usize = 1_500;

#[test]
fn system_prompt_and_default_tools_fit_the_budget() {
    let bpe = cl100k_base().expect("the tokenizer is bundled with the crate");
    let prefix = default_prompt_prefix();
    let tokens = bpe.encode_with_special_tokens(&prefix).len();

    println!(
        "system prompt + {} default tool schemas: {tokens} / {BUDGET} tokens",
        DEFAULT_TOOLS.len()
    );

    assert!(
        tokens <= BUDGET,
        "system prompt + default tool schemas use {tokens} tokens, budget is {BUDGET}"
    );
}

#[test]
fn the_prompt_is_not_smaller_than_it_needs_to_be() {
    // Guard against a budget test that passes because the prompt was emptied out.
    let bpe = cl100k_base().expect("the tokenizer is bundled with the crate");
    let tokens = bpe.encode_with_special_tokens(SYSTEM_PROMPT).len();
    assert!(
        tokens > 40,
        "the system prompt looks truncated ({tokens} tokens)"
    );
}

#[test]
fn every_default_tool_is_declared() {
    assert_eq!(DEFAULT_TOOLS.len(), 4);
}
