//! The model catalog (SPEC 6, 9.7).
//!
//! The catalog says which models exist, how large their context is and **which reasoning variants
//! they declare**.
//!
//! Never invent a variant: a model without declared variants shows no reasoning chip, and
//! `/think` says that reasoning is not configurable for it. opencode once generated generic
//! budget-token variants for a model with its own native effort setting and had to fix it.

use serde_json::{Value, json};

use crate::provider::Protocol;

/// The Anthropic reasoning budgets (SPEC 9.7).
pub const ANTHROPIC_BUDGETS: [(&str, u32); 4] = [
    ("low", 4_000),
    ("medium", 10_000),
    ("high", 16_000),
    ("max", 32_000),
];

/// A reasoning variant: a named fragment of request-body options.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    /// The name the user picks, e.g. `off` or `high`.
    pub id: String,
    /// The fragment merged into the request body.
    pub body: Value,
}

impl Variant {
    pub fn new(id: impl Into<String>, body: Value) -> Self {
        Variant {
            id: id.into(),
            body,
        }
    }
}

/// A model in the catalog.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelInfo {
    /// The model id, e.g. `claude-sonnet-4-5`.
    pub id: String,
    /// The display name.
    pub name: String,
    /// The context window in tokens.
    pub context: u32,
    /// The protocol the model is talked to with.
    pub protocol: Protocol,
    /// The reasoning variants the model declares, in cycle order.
    pub variants: Vec<Variant>,
    /// The variant used when the user has not chosen one.
    pub default_variant: Option<String>,
    /// Whether the model runs on this machine.
    pub local: bool,
}

impl ModelInfo {
    /// Whether the model has any reasoning variants.
    pub fn has_variants(&self) -> bool {
        !self.variants.is_empty()
    }

    /// The variant with that name.
    pub fn variant(&self, id: &str) -> Option<&Variant> {
        self.variants.iter().find(|variant| variant.id == id)
    }

    /// The variant to use: the stored one, else the default, else the first.
    pub fn resolve(&self, stored: Option<&str>) -> Option<&Variant> {
        if let Some(stored) = stored
            && let Some(variant) = self.variant(stored)
        {
            return Some(variant);
        }
        if let Some(default) = &self.default_variant
            && let Some(variant) = self.variant(default)
        {
            return Some(variant);
        }
        self.variants.first()
    }

    /// The variants of an Anthropic model: off plus one per budget.
    pub fn anthropic_variants() -> Vec<Variant> {
        let mut variants = vec![Variant::new("off", json!({}))];
        for (name, budget) in ANTHROPIC_BUDGETS {
            variants.push(Variant::new(
                name,
                json!({ "thinking": { "type": "enabled", "budget_tokens": budget } }),
            ));
        }
        variants
    }

    /// The variants of an OpenAI model: none plus the effort levels.
    pub fn openai_variants(levels: &[&str]) -> Vec<Variant> {
        let mut variants = vec![Variant::new("off", json!({ "reasoning_effort": "none" }))];
        for level in levels {
            variants.push(Variant::new(*level, json!({ "reasoning_effort": level })));
        }
        variants
    }

    /// The variants of a local llama.cpp model.
    ///
    /// The server is asked what it supports; these are the two shapes the spec defines.
    pub fn local_variants(levels: &[&str]) -> Vec<Variant> {
        let mut variants = vec![Variant::new(
            "off",
            json!({ "chat_template_kwargs": { "enable_thinking": false } }),
        )];
        for level in levels {
            variants.push(Variant::new(
                *level,
                json!({
                    "chat_template_kwargs": {
                        "enable_thinking": true,
                        "reasoning_effort": level
                    }
                }),
            ));
        }
        variants
    }
}

/// The models that ship with the binary.
///
/// The context sizes come from the models.dev catalog (schema 2); the variants are the ones the
/// spec defines for the protocol.
pub fn bundled_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "claude-sonnet-4-5".into(),
            name: "Claude Sonnet 4.5".into(),
            context: 200_000,
            protocol: Protocol::AnthropicMessages,
            variants: ModelInfo::anthropic_variants(),
            default_variant: Some("medium".into()),
            local: false,
        },
        ModelInfo {
            id: "claude-opus-4-1".into(),
            name: "Claude Opus 4.1".into(),
            context: 200_000,
            protocol: Protocol::AnthropicMessages,
            variants: ModelInfo::anthropic_variants(),
            default_variant: Some("medium".into()),
            local: false,
        },
        ModelInfo {
            id: "claude-haiku-4-5".into(),
            name: "Claude Haiku 4.5".into(),
            context: 200_000,
            protocol: Protocol::AnthropicMessages,
            variants: ModelInfo::anthropic_variants(),
            default_variant: Some("medium".into()),
            local: false,
        },
        ModelInfo {
            id: "gpt-5.4".into(),
            name: "GPT-5.4".into(),
            context: 1_050_000,
            protocol: Protocol::OpenAiChat,
            variants: ModelInfo::openai_variants(&["minimal", "low", "medium", "high", "xhigh"]),
            default_variant: Some("medium".into()),
            local: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_catalog_is_not_empty() {
        let models = bundled_models();
        assert!(models.len() >= 4);
        assert!(models.iter().all(|model| model.context > 0));
    }

    #[test]
    fn anthropic_models_declare_off_and_the_budgets() {
        let model = &bundled_models()[0];
        let ids: Vec<_> = model.variants.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, ["off", "low", "medium", "high", "max"]);
        assert_eq!(
            model.variant("high").unwrap().body["thinking"]["budget_tokens"],
            16_000
        );
        assert!(
            model
                .variant("off")
                .unwrap()
                .body
                .as_object()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn the_off_variant_sends_no_thinking_options() {
        let model = &bundled_models()[0];
        let off = model.variant("off").unwrap();
        assert!(off.body.get("thinking").is_none());
    }

    #[test]
    fn a_model_without_variants_has_no_reasoning_chip() {
        let model = ModelInfo {
            id: "plain".into(),
            name: "Plain".into(),
            context: 8_000,
            protocol: Protocol::OpenAiChat,
            variants: Vec::new(),
            default_variant: None,
            local: false,
        };
        assert!(!model.has_variants());
        assert!(model.resolve(None).is_none());
    }

    #[test]
    fn a_stored_variant_wins_over_the_default() {
        let model = &bundled_models()[0];
        assert_eq!(model.resolve(Some("high")).unwrap().id, "high");
        assert_eq!(model.resolve(None).unwrap().id, "medium");
        // An unknown level falls back to the default.
        assert_eq!(model.resolve(Some("nonsense")).unwrap().id, "medium");
    }

    #[test]
    fn local_variants_use_chat_template_kwargs() {
        let variants = ModelInfo::local_variants(&["low", "high"]);
        assert_eq!(variants.len(), 3);
        assert_eq!(
            variants[0].body["chat_template_kwargs"]["enable_thinking"],
            false
        );
        assert_eq!(
            variants[2].body["chat_template_kwargs"]["reasoning_effort"],
            "high"
        );
    }
}
