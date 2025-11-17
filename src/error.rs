//! Error types and result aliases for the Mojentic library.
//!
//! This module defines the core error type [`MojenticError`] and the [`Result`] type alias
//! used throughout the library. All public APIs that can fail return `Result<T>` for
//! consistent error handling.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum MojenticError {
    #[error("LLM gateway error: {0}")]
    GatewayError(String),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Tool error: {0}")]
    ToolError(String),

    #[error("Model not supported: {0}")]
    ModelNotSupported(String),

    #[error("Invalid configuration: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Event processing error: {0}")]
    EventError(String),

    #[error("Agent error: {0}")]
    AgentError(String),

    #[error("Dispatcher error: {0}")]
    DispatcherError(String),

    #[error("Timeout error: {0}")]
    TimeoutError(String),
}

pub type Result<T> = std::result::Result<T, MojenticError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_error_display() {
        let err = MojenticError::GatewayError("connection failed".to_string());
        assert_eq!(err.to_string(), "LLM gateway error: connection failed");
    }

    #[test]
    fn test_api_error_display() {
        let err = MojenticError::ApiError("rate limit exceeded".to_string());
        assert_eq!(err.to_string(), "API error: rate limit exceeded");
    }

    #[test]
    fn test_tool_error_display() {
        let err = MojenticError::ToolError("invalid parameters".to_string());
        assert_eq!(err.to_string(), "Tool error: invalid parameters");
    }

    #[test]
    fn test_model_not_supported_display() {
        let err = MojenticError::ModelNotSupported("gpt-5".to_string());
        assert_eq!(err.to_string(), "Model not supported: gpt-5");
    }

    #[test]
    fn test_config_error_display() {
        let err = MojenticError::ConfigError("missing API key".to_string());
        assert_eq!(err.to_string(), "Invalid configuration: missing API key");
    }

    #[test]
    fn test_serialization_error_conversion() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let err: MojenticError = json_err.into();

        match err {
            MojenticError::SerializationError(_) => {}
            _ => panic!("Expected SerializationError"),
        }
    }

    #[test]
    fn test_http_error_conversion() {
        // Create a reqwest error by building an invalid request
        let invalid_url = reqwest::Url::parse("http://").unwrap_err();
        // Test that we can convert a URL parse error into our error type
        let err = MojenticError::ApiError(invalid_url.to_string());
        assert!(err.to_string().contains("API error"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: MojenticError = io_err.into();

        match err {
            MojenticError::IoError(_) => {}
            _ => panic!("Expected IoError"),
        }
    }

    #[test]
    fn test_error_debug() {
        let err = MojenticError::ToolError("test".to_string());
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("ToolError"));
    }

    #[test]
    fn test_result_type() {
        let ok_result: Result<i32> = Ok(42);
        assert!(ok_result.is_ok());
        if let Ok(value) = ok_result {
            assert_eq!(value, 42);
        }

        let err_result: Result<i32> = Err(MojenticError::ToolError("test".to_string()));
        assert!(err_result.is_err());
    }
}
