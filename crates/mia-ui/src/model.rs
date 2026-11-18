//! A model that is honest about not being one.
//!
//! `NoModel` implements the harness `Model` trait and answers every
//! completion with a plain statement: no model is configured. It exists so
//! the GUI runs and stays useful without a provider key — preferences,
//! facts and documents are routed by `MiaRuntime` without touching the
//! model at all, so only free-form questions hit this stub.
//!
//! Wiring a real model is an environment choice, not a code change: the
//! `mia` binary builds an `OpenAiCompat` model when `MIA_MODEL_KEY` is
//! set, and falls back to `NoModel` otherwise.

use async_trait::async_trait;
use harness_core::{
    Context, Model, ModelError, ModelInfo, ModelOutput, StopReason, Usage,
};

/// The message returned for every completion without a model.
pub const NO_MODEL_TEXT: &str = "Nessun modello configurato: posso salvare \
preferenze, fatti e pagine, ma per le domande mi serve un modello. \
Imposta MIA_MODEL_KEY e riavvia Mia.";

#[derive(Debug, Default)]
pub struct NoModel;

#[async_trait]
impl Model for NoModel {
    async fn complete(&self, _ctx: &Context) -> Result<ModelOutput, ModelError> {
        Ok(ModelOutput {
            text: Some(NO_MODEL_TEXT.to_string()),
            tool_calls: vec![],
            usage: Usage::default(),
            stop_reason: StopReason::EndTurn,
            reasoning: None,
            images: vec![],
        })
    }

    fn info(&self) -> ModelInfo {
        ModelInfo {
            handle: "none".into(),
            provider: "none".into(),
            model: "none".into(),
            context_window: 0,
            input_cost_usd_per_million_tokens: None,
            output_cost_usd_per_million_tokens: None,
            supports_tool_use: false,
            supports_streaming: false,
            supports_web_grounding: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_core::Context;

    #[tokio::test]
    async fn no_model_answers_plainly() {
        let model = NoModel;
        let task = harness_core::Task {
            description: "che tempo fa?".into(),
            source: None,
            deadline: None,
        };
        let output = model
            .complete(&Context::new(task))
            .await
            .expect("NoModel never fails");
        assert_eq!(output.text.as_deref(), Some(NO_MODEL_TEXT));
        assert!(output.tool_calls.is_empty());
        assert_eq!(model.info().handle, "none");
        assert!(!model.info().supports_tool_use);
    }
}
