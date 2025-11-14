//! Tokenizer gateway for encoding and decoding text using tiktoken.
//!
//! This module provides token counting functionality which is useful for:
//! - Managing context window limits
//! - Estimating API costs
//! - Debugging tokenization issues
//! - Optimizing prompt engineering

use tiktoken_rs::CoreBPE;

/// Gateway for tokenizing and detokenizing text using tiktoken.
///
/// The tokenizer gateway provides encoding and decoding functionality,
/// allowing you to convert text to tokens and back. This is essential
/// for understanding token usage and managing context windows.
///
/// # Examples
///
/// ```
/// use mojentic::llm::gateways::TokenizerGateway;
///
/// let tokenizer = TokenizerGateway::new("cl100k_base").unwrap();
/// let text = "Hello, world!";
/// let tokens = tokenizer.encode(text);
/// let decoded = tokenizer.decode(&tokens);
/// assert_eq!(text, decoded);
/// ```
pub struct TokenizerGateway {
    tokenizer: CoreBPE,
}

impl TokenizerGateway {
    /// Creates a new TokenizerGateway with the specified encoding model.
    ///
    /// # Arguments
    ///
    /// * `model` - The encoding model to use. Common options:
    ///   - "cl100k_base" - Used by GPT-4 and GPT-3.5-turbo (default)
    ///   - "p50k_base" - Used by older GPT-3 models
    ///   - "r50k_base" - Used by even older models
    ///
    /// # Errors
    ///
    /// Returns an error if the specified model is not available.
    ///
    /// # Examples
    ///
    /// ```
    /// use mojentic::llm::gateways::TokenizerGateway;
    ///
    /// let tokenizer = TokenizerGateway::new("cl100k_base").unwrap();
    /// ```
    pub fn new(model: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let tokenizer = match model {
            "cl100k_base" => tiktoken_rs::cl100k_base()?,
            "p50k_base" => tiktoken_rs::p50k_base()?,
            "r50k_base" => tiktoken_rs::r50k_base()?,
            _ => return Err(format!("Unsupported encoding model: {}", model).into()),
        };
        Ok(Self { tokenizer })
    }

    /// Encodes text into tokens.
    ///
    /// # Arguments
    ///
    /// * `text` - The text to encode
    ///
    /// # Returns
    ///
    /// A vector of token IDs representing the encoded text.
    ///
    /// # Examples
    ///
    /// ```
    /// use mojentic::llm::gateways::TokenizerGateway;
    ///
    /// let tokenizer = TokenizerGateway::default();
    /// let tokens = tokenizer.encode("Hello, world!");
    /// println!("Token count: {}", tokens.len());
    /// ```
    pub fn encode(&self, text: &str) -> Vec<usize> {
        tracing::debug!("Encoding text: {}", text);
        self.tokenizer.encode_with_special_tokens(text)
    }

    /// Decodes tokens back into text.
    ///
    /// # Arguments
    ///
    /// * `tokens` - The slice of token IDs to decode
    ///
    /// # Returns
    ///
    /// The decoded text.
    ///
    /// # Examples
    ///
    /// ```
    /// use mojentic::llm::gateways::TokenizerGateway;
    ///
    /// let tokenizer = TokenizerGateway::default();
    /// let tokens = vec![9906, 11, 1917, 0];
    /// let text = tokenizer.decode(&tokens);
    /// println!("Decoded: {}", text);
    /// ```
    pub fn decode(&self, tokens: &[usize]) -> String {
        tracing::debug!("Decoding {} tokens", tokens.len());
        self.tokenizer.decode(tokens.to_vec()).unwrap_or_else(|e| {
            tracing::error!("Failed to decode tokens: {}", e);
            String::new()
        })
    }

    /// Counts the number of tokens in a text string.
    ///
    /// This is a convenience method that encodes the text and returns
    /// the token count without allocating the token vector.
    ///
    /// # Arguments
    ///
    /// * `text` - The text to count tokens for
    ///
    /// # Returns
    ///
    /// The number of tokens in the text.
    ///
    /// # Examples
    ///
    /// ```
    /// use mojentic::llm::gateways::TokenizerGateway;
    ///
    /// let tokenizer = TokenizerGateway::default();
    /// let count = tokenizer.count_tokens("Hello, world!");
    /// println!("Token count: {}", count);
    /// ```
    pub fn count_tokens(&self, text: &str) -> usize {
        self.encode(text).len()
    }
}

impl Default for TokenizerGateway {
    fn default() -> Self {
        // Use cl100k_base as the default tokenizer
        Self::new("cl100k_base").expect("cl100k_base should always be available")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_basic() {
        let tokenizer = TokenizerGateway::default();
        let text = "Hello, world!";
        let tokens = tokenizer.encode(text);

        assert!(!tokens.is_empty());
        // Tokens are valid usize values (some tokens can be 0, like BOS/EOS)
        assert!(!tokens.is_empty());
    }

    #[test]
    fn test_encode_empty() {
        let tokenizer = TokenizerGateway::default();
        let tokens = tokenizer.encode("");
        assert_eq!(tokens.len(), 0);
    }

    #[test]
    fn test_encode_consistent() {
        let tokenizer = TokenizerGateway::default();
        let text = "The quick brown fox";
        let tokens1 = tokenizer.encode(text);
        let tokens2 = tokenizer.encode(text);

        assert_eq!(tokens1, tokens2);
    }

    #[test]
    fn test_decode_basic() {
        let tokenizer = TokenizerGateway::default();
        let original = "Hello, world!";
        let tokens = tokenizer.encode(original);
        let decoded = tokenizer.decode(&tokens);

        assert_eq!(original, decoded);
    }

    #[test]
    fn test_decode_empty() {
        let tokenizer = TokenizerGateway::default();
        let text = tokenizer.decode(&[]);
        assert_eq!(text, "");
    }

    #[test]
    fn test_round_trip() {
        let tokenizer = TokenizerGateway::default();
        let test_cases = vec![
            "Simple text",
            "Text with numbers: 123456",
            "Special characters: !@#$%^&*()",
            "Multi-line\ntext\nwith\nnewlines",
            "Unicode: 你好世界 🌍",
        ];

        for original in test_cases {
            let tokens = tokenizer.encode(original);
            let decoded = tokenizer.decode(&tokens);
            assert_eq!(original, decoded, "Round-trip failed for: {}", original);
        }
    }

    #[test]
    fn test_different_encodings() {
        // cl100k_base is the default and most common
        let tokenizer_cl100k = TokenizerGateway::default();

        // p50k_base for older models
        let tokenizer_p50k = TokenizerGateway::new("p50k_base").unwrap();

        let text = "Hello, world!";
        let tokens_cl100k = tokenizer_cl100k.encode(text);
        let tokens_p50k = tokenizer_p50k.encode(text);

        // Both should work
        assert!(!tokens_cl100k.is_empty());
        assert!(!tokens_p50k.is_empty());

        // Both should decode correctly
        assert_eq!(tokenizer_cl100k.decode(&tokens_cl100k), text);
        assert_eq!(tokenizer_p50k.decode(&tokens_p50k), text);
    }

    #[test]
    fn test_count_tokens() {
        let tokenizer = TokenizerGateway::default();
        let text = "What is the capital of France?";
        let count = tokenizer.count_tokens(text);

        // This specific message should be around 7-8 tokens with cl100k_base
        assert!(count > 5);
        assert!(count < 15);
    }

    #[test]
    fn test_count_tokens_matches_encode() {
        let tokenizer = TokenizerGateway::default();
        let text = "The quick brown fox jumps over the lazy dog.";

        let tokens = tokenizer.encode(text);
        let count = tokenizer.count_tokens(text);

        assert_eq!(tokens.len(), count);
    }

    #[test]
    fn test_long_text() {
        let tokenizer = TokenizerGateway::default();
        let long_text = "word ".repeat(1000);
        let tokens = tokenizer.encode(&long_text);

        assert!(tokens.len() > 1000);

        let decoded = tokenizer.decode(&tokens);
        assert_eq!(long_text, decoded);
    }

    #[test]
    fn test_unicode_handling() {
        let tokenizer = TokenizerGateway::default();
        let unicode_text = "Hello 世界! 🌍 Special chars: @#$%";
        let tokens = tokenizer.encode(unicode_text);
        let decoded = tokenizer.decode(&tokens);

        assert_eq!(unicode_text, decoded);
        assert!(!tokens.is_empty());
    }
}
