/// Example demonstrating file management tools.
///
/// This example shows how to use the file_manager module with its various tools
/// for reading, writing, listing, and searching files within a sandboxed directory.
use mojentic::llm::tools::file_manager::{
    CreateDirectoryTool, FilesystemGateway, FindFilesByGlobTool, FindFilesContainingTool,
    FindLinesMatchingTool, ListAllFilesTool, ListFilesTool, ReadFileTool, WriteFileTool,
};
use mojentic::llm::tools::LlmTool;
use serde_json::json;
use std::collections::HashMap;
use tempfile::TempDir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Mojentic Rust - File Tool Example\n");

    // Create a temporary directory for demonstration
    let sandbox_dir = TempDir::new()?;
    let sandbox_path = sandbox_dir.path();

    println!("Sandbox directory: {:?}", sandbox_path);
    println!();

    // Initialize the filesystem gateway
    let gateway = FilesystemGateway::new(sandbox_path)?;

    // Create some example files using WriteFileTool
    let write_tool = WriteFileTool::new(gateway.clone());

    println!("📝 Creating example files...");

    let mut args = HashMap::new();
    args.insert("path".to_string(), json!("example.txt"));
    args.insert("content".to_string(), json!("Hello, world!\nThis is an example file.\n"));
    write_tool.run(&args, &mojentic::llm::tools::ToolRunCtx::default()).await?;
    println!("  ✓ Created example.txt");

    args.insert("path".to_string(), json!("test.rs"));
    args.insert("content".to_string(), json!("fn main() {\n    println!(\"Hello\");\n}\n"));
    write_tool.run(&args, &mojentic::llm::tools::ToolRunCtx::default()).await?;
    println!("  ✓ Created test.rs");

    // Create a directory
    let create_dir_tool = CreateDirectoryTool::new(gateway.clone());
    let mut args = HashMap::new();
    args.insert("path".to_string(), json!("src"));
    create_dir_tool.run(&args, &mojentic::llm::tools::ToolRunCtx::default()).await?;
    println!("  ✓ Created src/ directory");

    // Write a file in the subdirectory
    let mut args = HashMap::new();
    args.insert("path".to_string(), json!("src/lib.rs"));
    args.insert("content".to_string(), json!("pub struct MyStruct {\n    value: i32,\n}\n"));
    write_tool.run(&args, &mojentic::llm::tools::ToolRunCtx::default()).await?;
    println!("  ✓ Created src/lib.rs");
    println!();

    // List files in root directory
    println!("📁 Listing files in root directory:");
    let list_tool = ListFilesTool::new(gateway.clone());
    let mut args = HashMap::new();
    args.insert("path".to_string(), json!("."));
    let result = list_tool.run(&args, &mojentic::llm::tools::ToolRunCtx::default()).await?;
    println!("  {}", result);
    println!();

    // List all files recursively
    println!("📁 Listing all files recursively:");
    let list_all_tool = ListAllFilesTool::new(gateway.clone());
    let mut args = HashMap::new();
    args.insert("path".to_string(), json!("."));
    let result = list_all_tool.run(&args, &mojentic::llm::tools::ToolRunCtx::default()).await?;
    println!("  {}", result);
    println!();

    // Read a file
    println!("📖 Reading example.txt:");
    let read_tool = ReadFileTool::new(gateway.clone());
    let mut args = HashMap::new();
    args.insert("path".to_string(), json!("example.txt"));
    let result = read_tool.run(&args, &mojentic::llm::tools::ToolRunCtx::default()).await?;
    println!("  Content: {}", result);
    println!();

    // Find files by glob pattern
    println!("🔍 Finding Rust files (*.rs):");
    let glob_tool = FindFilesByGlobTool::new(gateway.clone());
    let mut args = HashMap::new();
    args.insert("path".to_string(), json!("."));
    args.insert("pattern".to_string(), json!("**/*.rs"));
    let result = glob_tool.run(&args, &mojentic::llm::tools::ToolRunCtx::default()).await?;
    println!("  {}", result);
    println!();

    // Find files containing a pattern
    println!("🔍 Finding files containing 'println':");
    let containing_tool = FindFilesContainingTool::new(gateway.clone());
    let mut args = HashMap::new();
    args.insert("path".to_string(), json!("."));
    args.insert("pattern".to_string(), json!("println"));
    let result = containing_tool.run(&args, &mojentic::llm::tools::ToolRunCtx::default()).await?;
    println!("  {}", result);
    println!();

    // Find lines matching a pattern
    println!("🔍 Finding lines in test.rs matching 'fn':");
    let lines_tool = FindLinesMatchingTool::new(gateway.clone());
    let mut args = HashMap::new();
    args.insert("path".to_string(), json!("test.rs"));
    args.insert("pattern".to_string(), json!("fn"));
    let result = lines_tool.run(&args, &mojentic::llm::tools::ToolRunCtx::default()).await?;
    println!("  {}", result);
    println!();

    println!("✅ Example completed successfully!");
    println!("\nFile management tools demonstrated:");
    println!("  ✓ CreateDirectoryTool: Create directories");
    println!("  ✓ WriteFileTool: Write file contents");
    println!("  ✓ ReadFileTool: Read file contents");
    println!("  ✓ ListFilesTool: List directory contents");
    println!("  ✓ ListAllFilesTool: Recursive directory listing");
    println!("  ✓ FindFilesByGlobTool: Pattern-based file search");
    println!("  ✓ FindFilesContainingTool: Content-based file search");
    println!("  ✓ FindLinesMatchingTool: Line-level pattern matching");

    Ok(())
}
