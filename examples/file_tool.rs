/// Example demonstrating the file management tools.
///
/// This example shows how to use the FilesystemGateway and various file tools
/// to interact with the filesystem safely within a sandboxed directory.

use mojentic::llm::{LlmBroker, LlmMessage};
use mojentic::llm::gateways::OllamaGateway;
use mojentic::llm::tools::file_manager::*;
use mojentic::llm::tools::LlmTool;
use std::fs;
use tempfile::TempDir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a temporary directory for the example
    let sandbox_dir = TempDir::new()?;
    let sandbox_path = sandbox_dir.path();

    println!("Sandbox directory: {:?}", sandbox_path);
    println!();

    // Create a FilesystemGateway
    let fs = FilesystemGateway::new(sandbox_path)?;

    // Create some example files
    fs::write(sandbox_path.join("example.txt"), "Hello, world!\nThis is an example file.\n")?;
    fs::write(sandbox_path.join("test.rs"), "fn main() {\n    println!(\"Hello\");\n}\n")?;

    let src_dir = sandbox_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(src_dir.join("lib.rs"), "pub struct MyStruct {\n    value: i32,\n}\n")?;

    println!("Created example files");
    println!();

    // Example 1: List files in root directory
    println!("Example 1: List files in root directory");
    let list_tool = ListFilesTool::new(fs.clone());
    let files = list_tool.run(serde_json::json!({"path": "."}))?;
    println!("Files in root: {}", files);
    println!();

    // Example 2: Read a file
    println!("Example 2: Read a file");
    let read_tool = ReadFileTool::new(fs.clone());
    let content = read_tool.run(serde_json::json!({"path": "example.txt"}))?;
    println!("Content of example.txt:");
    println!("{}", content);
    println!();

    // Example 3: Write a file
    println!("Example 3: Write a file");
    let write_tool = WriteFileTool::new(fs.clone());
    let message = write_tool.run(serde_json::json!({
        "path": "output.txt",
        "content": "New file content\n"
    }))?;
    println!("{}", message);
    println!();

    // Example 4: List all files recursively
    println!("Example 4: List all files recursively");
    let list_all_tool = ListAllFilesTool::new(fs.clone());
    let all_files = list_all_tool.run(serde_json::json!({"path": "."}))?;
    println!("All files (recursive): {}", all_files);
    println!();

    // Example 5: Find files by glob pattern
    println!("Example 5: Find files by glob pattern");
    let glob_tool = FindFilesByGlobTool::new(fs.clone());
    let rs_files = glob_tool.run(serde_json::json!({
        "path": ".",
        "pattern": "**/*.rs"
    }))?;
    println!("Rust files: {}", rs_files);
    println!();

    // Example 6: Find files containing pattern
    println!("Example 6: Find files containing pattern");
    let containing_tool = FindFilesContainingTool::new(fs.clone());
    let files_with_struct = containing_tool.run(serde_json::json!({
        "path": ".",
        "pattern": "struct"
    }))?;
    println!("Files containing 'struct': {}", files_with_struct);
    println!();

    // Example 7: Find lines matching pattern
    println!("Example 7: Find lines matching pattern");
    let lines_tool = FindLinesMatchingTool::new(fs.clone());
    let matching_lines = lines_tool.run(serde_json::json!({
        "path": "src/lib.rs",
        "pattern": "struct"
    }))?;
    println!("Lines containing 'struct' in lib.rs: {}", matching_lines);
    println!();

    // Example 8: Create a directory
    println!("Example 8: Create a directory");
    let mkdir_tool = CreateDirectoryTool::new(fs.clone());
    let mkdir_message = mkdir_tool.run(serde_json::json!({"path": "new_directory"}))?;
    println!("{}", mkdir_message);
    println!();

    // Example 9: Use file tools with LLM
    println!("Example 9: Use file tools with LLM");
    println!("Setting up LLM broker with file tools...");

    let gateway = OllamaGateway::default();
    let broker = LlmBroker::new("qwen2.5:7b", gateway);

    let tools: Vec<Box<dyn LlmTool>> = vec![
        Box::new(list_tool),
        Box::new(read_tool),
        Box::new(write_tool),
        Box::new(list_all_tool),
        Box::new(glob_tool),
        Box::new(containing_tool),
        Box::new(lines_tool),
        Box::new(mkdir_tool),
    ];

    let system_msg = LlmMessage::system(&format!(
        "You are a helpful assistant with access to file system tools. The sandbox root is {:?}.",
        sandbox_path
    ));
    let user_msg = LlmMessage::user(
        "What Rust files are in the sandbox? Read one of them and tell me what it does."
    );

    println!("Asking LLM: '{}'", user_msg.content);
    println!();

    match broker.generate(
        vec![system_msg, user_msg],
        None, None, None, Some(tools), None
    ).await {
        Ok(response) => {
            println!("LLM Response:");
            println!("{}", response.content);
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }

    println!();
    println!("Cleaning up sandbox directory...");
    println!("Done!");

    Ok(())
}
