/// Example demonstrating file management concepts.
///
/// NOTE: The file_manager module is currently disabled in the library.
/// This example is a placeholder demonstrating the intended API design.
/// Once file_manager is re-enabled, this example will work as written.
use std::fs;
use tempfile::TempDir;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Mojentic Rust - File Tool Example\n");
    println!("⚠️  NOTE: The file_manager module is currently disabled.");
    println!("This example demonstrates the directory structure that would be");
    println!("manipulated by file tools once they're re-enabled.\n");

    // Create a temporary directory for demonstration
    let sandbox_dir = TempDir::new()?;
    let sandbox_path = sandbox_dir.path();

    println!("Sandbox directory: {:?}", sandbox_path);
    println!();

    // Create some example files
    fs::write(sandbox_path.join("example.txt"), "Hello, world!\nThis is an example file.\n")?;
    fs::write(sandbox_path.join("test.rs"), "fn main() {\n    println!(\"Hello\");\n}\n")?;

    let src_dir = sandbox_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(src_dir.join("lib.rs"), "pub struct MyStruct {\n    value: i32,\n}\n")?;

    println!("Created example files");
    println!();

    // List the created files
    println!("Created structure:");
    for entry in fs::read_dir(sandbox_path)? {
        let entry = entry?;
        println!("  - {:?}", entry.file_name());
    }
    println!("  - src/");
    for entry in fs::read_dir(&src_dir)? {
        let entry = entry?;
        println!("    - {:?}", entry.file_name());
    }

    println!();
    println!("✅ Example completed!");
    println!("Once file_manager is re-enabled, this example will demonstrate:");
    println!("  - ListFilesTool: List directory contents");
    println!("  - ReadFileTool: Read file contents");
    println!("  - WriteFileTool: Write to files");
    println!("  - ListAllFilesTool: Recursive directory listing");
    println!("  - FindFilesByGlobTool: Pattern-based file search");
    println!("  - FindFilesContainingTool: Content-based file search");
    println!("  - FindLinesMatchingTool: Line-level pattern matching");
    println!("  - CreateDirectoryTool: Directory creation");
    println!("  - LLM-driven file operations within sandboxed environments");

    Ok(())
}
