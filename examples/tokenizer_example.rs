use mojentic::llm::gateways::TokenizerGateway;

/// Example demonstrating the TokenizerGateway usage.
///
/// This shows how to:
/// - Create a tokenizer instance
/// - Encode text into tokens
/// - Decode tokens back to text
/// - Count tokens for context window management
fn main() {
    // Create a tokenizer with the default cl100k_base encoding
    // (used by GPT-4 and GPT-3.5-turbo)
    let tokenizer = TokenizerGateway::default();

    println!("=== TokenizerGateway Example ===\n");

    // Example 1: Basic encoding and decoding
    let text1 = "Hello, world! This is a test message.";
    println!("Original text: \"{}\"", text1);

    let tokens1 = tokenizer.encode(text1);
    println!("Tokens: {:?}", tokens1);
    println!("Token count: {}", tokens1.len());

    let decoded1 = tokenizer.decode(&tokens1);
    println!("Decoded text: \"{}\"", decoded1);
    println!("Round-trip successful: {}\n", text1 == decoded1);

    // Example 2: Counting tokens for context window management
    let long_message = r#"
This is a longer message that demonstrates token counting.
Token counting is important for:
- Managing context window limits
- Estimating API costs
- Optimizing prompt engineering
- Debugging tokenization issues
    "#
    .trim();

    let tokens2 = tokenizer.encode(long_message);
    println!("\nLong message token count: {}", tokens2.len());
    println!("First 10 tokens: {:?}", &tokens2[..10.min(tokens2.len())]);

    // Example 3: Comparing different text lengths
    let texts = vec![
        "Hi",
        "Hello, how are you?",
        "The quick brown fox jumps over the lazy dog.",
        "A much longer sentence with more words will naturally have more tokens.",
    ];

    println!("\n=== Token Counts for Different Text Lengths ===");
    for text in texts {
        let tokens = tokenizer.encode(text);
        println!("\"{}\"", text);
        println!("  → {} tokens\n", tokens.len());
    }

    // Example 4: Unicode and special characters
    let unicode_text = "Hello 世界! 🌍 Special chars: @#$%";
    let unicode_tokens = tokenizer.encode(unicode_text);
    println!("Unicode text: \"{}\"", unicode_text);
    println!("Token count: {}", unicode_tokens.len());
    println!("Decoded: \"{}\"\n", tokenizer.decode(&unicode_tokens));

    // Example 5: Using count_tokens convenience method
    let sample_text = "What is the capital of France?";
    let count = tokenizer.count_tokens(sample_text);
    println!("Token count for \"{}\": {}", sample_text, count);

    // Example 6: Different encodings
    println!("\n=== Different Encodings ===");
    let text = "This is a test of different encodings";

    let tokenizer_cl100k = TokenizerGateway::new("cl100k_base").unwrap();
    let count_cl100k = tokenizer_cl100k.count_tokens(text);

    let tokenizer_p50k = TokenizerGateway::new("p50k_base").unwrap();
    let count_p50k = tokenizer_p50k.count_tokens(text);

    println!("cl100k_base: {} tokens", count_cl100k);
    println!("p50k_base: {} tokens", count_p50k);

    println!("\nTokenizer example completed!");
}
