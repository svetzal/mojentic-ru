# Example: File Tools

This chapter demonstrates how to build tools for interacting with the file system using the Mojentic framework. The included `FileTool` and `CodingFileTool` serve as reference implementations that you can use directly or adapt for your specific needs.

## Available Tools

### FileTool

The `FileTool` provides basic file operations:

- `read_file`: Read content of a file
- `write_file`: Write content to a file
- `list_dir`: List directory contents
- `file_exists`: Check if a file exists

### CodingFileTool

The `CodingFileTool` extends `FileTool` with features specifically for coding tasks:

- `apply_patch`: Apply a unified diff patch to a file
- `replace_text`: Replace specific text in a file
- `search_files`: Search for patterns in files

## Usage

```rust
use mojentic::prelude::*;
use mojentic::tools::FileTool;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize broker
    let gateway = Arc::new(OllamaGateway::new());
    let broker = LlmBroker::new("qwen3:32b", gateway, None);

    // Register tools
    let tools: Vec<Arc<dyn LlmTool>> = vec![
        Arc::new(FileTool::default()),
    ];

    // Ask the agent to perform file operations
    let messages = vec![
        LlmMessage::user("Create a file named 'hello.txt' with the content 'Hello, World!'"),
    ];

    let response = broker.generate(&messages, Some(tools), None).await?;
    println!("{}", response);
    
    Ok(())
}
```

## Security

By default, file tools are restricted to the current working directory. You can configure allowed paths to restrict access further.
