//! Explicit model capabilities, separate from provider wire conventions.
use crate::error::{MojenticError, Result};
use crate::llm::gateway::ReasoningEffort;
use serde::{Deserialize, Serialize};

/// A caller-selected model contract; never inferred from a model-name substring.
/// Provider defaults remain unknown when no effort is requested.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningProfile {
    /// Stable label for provenance and diagnostics.
    pub name: String,
    /// Exact accepted controls; high and xhigh are not aliases.
    pub supported_efforts: Vec<ReasoningEffort>,
    /// Whether the model consumes native assistant reasoning in history.
    pub supports_reasoning_history: bool,
}

impl ReasoningProfile {
    /// Qwen3.8 thinking controls and native reasoning-history support.
    /// No default effort is imposed: set one explicitly for comparable runs.
    pub fn qwen3_8() -> Self {
        Self {
            name: "qwen3.8".into(),
            supported_efforts: vec![
                ReasoningEffort::Disabled,
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::XHigh,
            ],
            supports_reasoning_history: true,
        }
    }

    /// Reject unsupported controls before HTTP instead of silently translating them.
    pub fn validate(&self, effort: Option<ReasoningEffort>, preserve_history: bool) -> Result<()> {
        if let Some(effort) = effort {
            if !self.supported_efforts.contains(&effort) {
                return Err(MojenticError::ConfigError(format!(
                    "reasoning effort {effort:?} is unsupported by profile {}; supported: {:?}",
                    self.name, self.supported_efforts
                )));
            }
        }
        if preserve_history && !self.supports_reasoning_history {
            return Err(MojenticError::ConfigError(format!(
                "profile {} does not support native reasoning history",
                self.name
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qwen_accepts_exact_controls_without_inventing_a_default() {
        let profile = ReasoningProfile::qwen3_8();
        for effort in [
            None,
            Some(ReasoningEffort::Disabled),
            Some(ReasoningEffort::Low),
            Some(ReasoningEffort::Medium),
            Some(ReasoningEffort::XHigh),
        ] {
            assert!(profile.validate(effort, true).is_ok());
        }
        assert!(profile.validate(Some(ReasoningEffort::High), false).is_err());
        assert!(profile.validate(Some(ReasoningEffort::Enabled), false).is_err());
    }
    #[test]
    fn boolean_model_profile_rejects_named_effort_and_native_history() {
        let profile = ReasoningProfile {
            name: "boolean-only".into(),
            supported_efforts: vec![ReasoningEffort::Disabled, ReasoningEffort::Enabled],
            supports_reasoning_history: false,
        };
        assert!(profile.validate(Some(ReasoningEffort::Enabled), false).is_ok());
        assert!(profile.validate(Some(ReasoningEffort::Low), false).is_err());
        assert!(profile.validate(None, true).is_err());
    }
}
