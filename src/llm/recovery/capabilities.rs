//! Inspected endpoint declarations. These do not control request or retry behavior.
use super::{Capabilities, Provider, Support};
use serde::Serialize;

/// Documentation supporting one endpoint capability declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CapabilityEvidence {
    /// Supported, unsupported, or undocumented/ambiguous for this endpoint.
    pub value: Support,
    /// Canonical provider endpoint; configured hosts may differ.
    pub endpoint: &'static str,
    /// Provider documentation inspected for this facility.
    pub documentation: &'static str,
    /// Original inspection date in ISO 8601 form (YYYY-MM-DD).
    pub inspected_on: &'static str,
    /// Endpoint-specific interpretation and limits of the cited documentation.
    pub basis: &'static str,
}

/// Evidence for all five recovery capabilities of one completion adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CapabilityEvidenceSet {
    /// Evidence for `local_request_cancellation`.
    pub local_request_cancellation: CapabilityEvidence,
    /// Evidence for `remote_request_cancellation`.
    pub remote_request_cancellation: CapabilityEvidence,
    /// Evidence for `request_status`.
    pub request_status: CapabilityEvidence,
    /// Evidence for `idempotency`.
    pub idempotency: CapabilityEvidence,
    /// Evidence for `exact_attempt_termination`.
    pub exact_attempt_termination: CapabilityEvidence,
}
impl CapabilityEvidenceSet {
    pub(crate) fn values(self) -> Capabilities {
        Capabilities {
            request_cancel: self.remote_request_cancellation.value,
            local_request_cancellation: self.local_request_cancellation.value,
            remote_request_cancellation: self.remote_request_cancellation.value,
            request_status: self.request_status.value,
            idempotency: self.idempotency.value,
            exact_attempt_termination: self.exact_attempt_termination.value,
        }
    }
    pub(crate) fn for_provider(provider: Provider) -> Self {
        match provider {
            Provider::Ollama => Self {
                local_request_cancellation: CapabilityEvidence {
                    value: Support::Supported,
                    endpoint: "POST http://localhost:11434/api/chat",
                    documentation: "https://github.com/ollama/ollama-js#abort",
                    inspected_on: "2026-10-16",
                    basis: "The provider SDK documents chat streaming and abort of streamed generations, raising AbortError locally; it does not attest to exact remote termination.",
                },
                remote_request_cancellation: CapabilityEvidence {
                    value: Support::Unsupported,
                    endpoint: "POST http://localhost:11434/api/chat",
                    documentation: "https://docs.ollama.com/api/chat",
                    inspected_on: "2026-10-16",
                    basis: "The chat endpoint documents generation, with no remote request cancellation facility. keep_alive controls model residency, not one request.",
                },
                request_status: CapabilityEvidence {
                    value: Support::Unsupported,
                    endpoint: "POST http://localhost:11434/api/chat",
                    documentation: "https://docs.ollama.com/api/chat",
                    inspected_on: "2026-10-16",
                    basis: "The chat request and response document no in-flight request status facility; done reports completion in the current response only.",
                },
                idempotency: CapabilityEvidence {
                    value: Support::Unsupported,
                    endpoint: "POST http://localhost:11434/api/chat",
                    documentation: "https://docs.ollama.com/api/chat",
                    inspected_on: "2026-10-16",
                    basis: "The chat request documents no idempotency key or deduplicated resend facility.",
                },
                exact_attempt_termination: CapabilityEvidence {
                    value: Support::Unknown,
                    endpoint: "POST http://localhost:11434/api/chat",
                    documentation: "https://docs.ollama.com/api/chat",
                    inspected_on: "2026-10-16",
                    basis: "The chat documentation does not prove termination of one exact wire attempt; SDK abort is not that evidence.",
                },
            },
            Provider::Omlx => Self {
                local_request_cancellation: CapabilityEvidence {
                    value: Support::Unknown,
                    endpoint: "POST http://localhost:8000/v1/chat/completions",
                    documentation: "https://github.com/jundot/omlx/blob/53ab0e66d75fe46e3cfa7ed7a7762d3bbcf21e9c/README.md#api-compatibility",
                    inspected_on: "2026-10-16",
                    basis: "The API endpoint inventory documents streaming chat completion but does not describe local request cancellation; OpenAI protocol compatibility alone is ambiguous.",
                },
                remote_request_cancellation: CapabilityEvidence {
                    value: Support::Unsupported,
                    endpoint: "POST http://localhost:8000/v1/chat/completions",
                    documentation: "https://github.com/jundot/omlx/blob/53ab0e66d75fe46e3cfa7ed7a7762d3bbcf21e9c/README.md#api-compatibility",
                    inspected_on: "2026-10-16",
                    basis: "The documented API endpoint inventory has no remote cancellation operation for chat completions; admin model management is not per-request cancellation.",
                },
                request_status: CapabilityEvidence {
                    value: Support::Unsupported,
                    endpoint: "POST http://localhost:8000/v1/chat/completions",
                    documentation: "https://github.com/jundot/omlx/blob/53ab0e66d75fe46e3cfa7ed7a7762d3bbcf21e9c/README.md#api-compatibility",
                    inspected_on: "2026-10-16",
                    basis: "The documented API endpoint inventory has no chat request status operation; model listing and usage analytics are not in-flight request status.",
                },
                idempotency: CapabilityEvidence {
                    value: Support::Unknown,
                    endpoint: "POST http://localhost:8000/v1/chat/completions",
                    documentation: "https://github.com/jundot/omlx/blob/53ab0e66d75fe46e3cfa7ed7a7762d3bbcf21e9c/README.md#api-compatibility",
                    inspected_on: "2026-10-16",
                    basis: "The endpoint inventory does not specify chat request headers or deduplication semantics, so idempotency is ambiguous.",
                },
                exact_attempt_termination: CapabilityEvidence {
                    value: Support::Unknown,
                    endpoint: "POST http://localhost:8000/v1/chat/completions",
                    documentation: "https://github.com/jundot/omlx/blob/53ab0e66d75fe46e3cfa7ed7a7762d3bbcf21e9c/README.md#api-compatibility",
                    inspected_on: "2026-10-16",
                    basis: "The streaming endpoint documentation does not prove termination of one exact wire attempt.",
                },
            },
            Provider::OpenAi => Self {
                local_request_cancellation: CapabilityEvidence {
                    value: Support::Supported,
                    endpoint: "POST https://api.openai.com/v1/chat/completions",
                    documentation: "https://developers.openai.com/api/reference/resources/chat/subresources/completions/streaming-events",
                    inspected_on: "2026-10-16",
                    basis: "Chat Completions streaming documents interrupted or cancelled streams and potentially missing final usage; this is local stream cancellation only.",
                },
                remote_request_cancellation: CapabilityEvidence {
                    value: Support::Unsupported,
                    endpoint: "POST https://api.openai.com/v1/chat/completions",
                    documentation: "https://developers.openai.com/api/reference/resources/chat/subresources/completions",
                    inspected_on: "2026-10-16",
                    basis: "The Chat Completions operation inventory has no remote cancel operation. Deleting a stored completion is not cancellation of inference.",
                },
                request_status: CapabilityEvidence {
                    value: Support::Unsupported,
                    endpoint: "POST https://api.openai.com/v1/chat/completions",
                    documentation: "https://developers.openai.com/api/reference/resources/chat/subresources/completions",
                    inspected_on: "2026-10-16",
                    basis: "Retrieval requires store=true and a completion ID; it retrieves a stored result, not the status of an in-flight or ambiguously accepted POST.",
                },
                idempotency: CapabilityEvidence {
                    value: Support::Unsupported,
                    endpoint: "POST https://api.openai.com/v1/chat/completions",
                    documentation: "https://developers.openai.com/api/reference/resources/chat/subresources/completions",
                    inspected_on: "2026-10-16",
                    basis: "The documented Chat Completions request parameters describe no idempotency facility for replaying this POST.",
                },
                exact_attempt_termination: CapabilityEvidence {
                    value: Support::Unknown,
                    endpoint: "POST https://api.openai.com/v1/chat/completions",
                    documentation: "https://developers.openai.com/api/reference/resources/chat/subresources/completions",
                    inspected_on: "2026-10-16",
                    basis: "The documentation does not prove termination of one exact wire attempt; local stream closure is insufficient.",
                },
            },
        }
    }
}
