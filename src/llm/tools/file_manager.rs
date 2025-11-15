use std::fs;
use std::path::{Path, PathBuf};
use regex::Regex;
use serde_json::{json, Value};

use crate::llm::tools::Tool;
use crate::error::{MojenticError, Result};

/// A gateway for interacting with the filesystem within a sandboxed base path.
///
/// This struct provides safe filesystem operations that are restricted to a
/// specific base directory, preventing path traversal attacks.
#[derive(Debug, Clone)]
pub struct FilesystemGateway {
    base_path: PathBuf,
}

impl FilesystemGateway {
    /// Creates a new FilesystemGateway with the specified base path.
    pub fn new<P: AsRef<Path>>(base_path: P) -> Result<Self> {
        let base_path = base_path.as_ref();

        if !base_path.is_dir() {
            return Err(MojenticError::Tool {
                message: format!("Base path {:?} is not a directory", base_path),
                source: None,
            });
        }

        Ok(Self {
            base_path: base_path.canonicalize()?,
        })
    }

    /// Resolves a path relative to the base path and ensures it stays within the sandbox.
    fn resolve_path<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf> {
        let path = path.as_ref();
        let resolved = self.base_path.join(path).canonicalize()
            .or_else(|_| {
                // If canonicalize fails (e.g., file doesn't exist yet), use join + normalize
                Ok::<PathBuf, std::io::Error>(self.base_path.join(path))
            })?;

        if !resolved.starts_with(&self.base_path) {
            return Err(MojenticError::Tool {
                message: format!("Path {:?} attempts to escape the sandbox", path),
                source: None,
            });
        }

        Ok(resolved)
    }

    /// Lists files in a directory (non-recursive).
    pub fn ls<P: AsRef<Path>>(&self, path: P) -> Result<Vec<String>> {
        let resolved_path = self.resolve_path(path)?;
        let entries = fs::read_dir(&resolved_path)?;

        let mut files = Vec::new();
        for entry in entries {
            let entry = entry?;
            let relative = entry.path()
                .strip_prefix(&self.base_path)
                .unwrap()
                .to_string_lossy()
                .to_string();
            files.push(relative);
        }

        Ok(files)
    }

    /// Lists all files recursively in a directory.
    pub fn list_all_files<P: AsRef<Path>>(&self, path: P) -> Result<Vec<String>> {
        let resolved_path = self.resolve_path(path)?;
        let mut files = Vec::new();

        self.collect_files_recursively(&resolved_path, &mut files)?;

        Ok(files)
    }

    fn collect_files_recursively(&self, dir: &Path, files: &mut Vec<String>) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        let entries = fs::read_dir(dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                self.collect_files_recursively(&path, files)?;
            } else {
                let relative = path
                    .strip_prefix(&self.base_path)
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                files.push(relative);
            }
        }

        Ok(())
    }

    /// Finds files matching a glob pattern.
    pub fn find_files_by_glob<P: AsRef<Path>>(&self, path: P, pattern: &str) -> Result<Vec<String>> {
        let resolved_path = self.resolve_path(path)?;
        let glob_pattern = resolved_path.join(pattern);
        let glob_str = glob_pattern.to_string_lossy();

        let mut files = Vec::new();
        for entry in glob::glob(&glob_str)
            .map_err(|e| MojenticError::Tool {
                message: format!("Invalid glob pattern: {}", e),
                source: Some(Box::new(e)),
            })? {
            match entry {
                Ok(path) => {
                    if let Ok(relative) = path.strip_prefix(&self.base_path) {
                        files.push(relative.to_string_lossy().to_string());
                    }
                }
                Err(_) => continue,
            }
        }

        Ok(files)
    }

    /// Finds files containing text matching a regex pattern.
    pub fn find_files_containing<P: AsRef<Path>>(&self, path: P, pattern: &str) -> Result<Vec<String>> {
        let resolved_path = self.resolve_path(path)?;
        let regex = Regex::new(pattern)
            .map_err(|e| MojenticError::Tool {
                message: format!("Invalid regex pattern: {}", e),
                source: Some(Box::new(e)),
            })?;

        let mut files = Vec::new();
        self.find_matching_files(&resolved_path, &regex, &mut files)?;

        Ok(files)
    }

    fn find_matching_files(&self, dir: &Path, regex: &Regex, files: &mut Vec<String>) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        let entries = fs::read_dir(dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                self.find_matching_files(&path, regex, files)?;
            } else if path.is_file() {
                if let Ok(content) = fs::read_to_string(&path) {
                    if regex.is_match(&content) {
                        let relative = path
                            .strip_prefix(&self.base_path)
                            .unwrap()
                            .to_string_lossy()
                            .to_string();
                        files.push(relative);
                    }
                }
            }
        }

        Ok(())
    }

    /// Finds all lines in a file matching a regex pattern.
    pub fn find_lines_matching<P: AsRef<Path>>(&self, path: P, file_name: &str, pattern: &str) -> Result<Vec<Value>> {
        let resolved_path = self.resolve_path(path)?;
        let file_path = resolved_path.join(file_name);
        let regex = Regex::new(pattern)
            .map_err(|e| MojenticError::Tool {
                message: format!("Invalid regex pattern: {}", e),
                source: Some(Box::new(e)),
            })?;

        let content = fs::read_to_string(&file_path)?;
        let mut matching_lines = Vec::new();

        for (i, line) in content.lines().enumerate() {
            if regex.is_match(line) {
                matching_lines.push(json!({
                    "line_number": i + 1,
                    "content": line
                }));
            }
        }

        Ok(matching_lines)
    }

    /// Reads the content of a file.
    pub fn read<P: AsRef<Path>>(&self, path: P, file_name: &str) -> Result<String> {
        let resolved_path = self.resolve_path(path)?;
        let file_path = resolved_path.join(file_name);
        Ok(fs::read_to_string(file_path)?)
    }

    /// Writes content to a file.
    pub fn write<P: AsRef<Path>>(&self, path: P, file_name: &str, content: &str) -> Result<()> {
        let resolved_path = self.resolve_path(path)?;
        let file_path = resolved_path.join(file_name);
        fs::write(file_path, content)?;
        Ok(())
    }
}

/// Tool for listing files in a directory (non-recursive).
pub struct ListFilesTool {
    fs: FilesystemGateway,
}

impl ListFilesTool {
    pub fn new(fs: FilesystemGateway) -> Self {
        Self { fs }
    }
}

impl Tool for ListFilesTool {
    fn descriptor(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "list_files",
                "description": "List files in the specified directory (non-recursive), optionally filtered by extension. Use this when you need to see what files are available in a specific directory without including subdirectories.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The path relative to the sandbox root to list files from. For example, '.' for the root directory, 'src' for the src directory, or 'docs/images' for a nested directory."
                        },
                        "extension": {
                            "type": "string",
                            "description": "The file extension to filter by (e.g., '.py', '.txt', '.md'). If not provided, all files will be listed. For example, using '.py' will only list Python files in the directory."
                        }
                    },
                    "additionalProperties": false,
                    "required": ["path"]
                }
            }
        })
    }

    fn run(&self, args: Value) -> Result<String> {
        let path = args["path"].as_str()
            .ok_or_else(|| MojenticError::Tool {
                message: "Missing 'path' parameter".to_string(),
                source: None,
            })?;

        let extension = args.get("extension").and_then(|v| v.as_str());

        let files = self.fs.ls(path)?;

        let filtered: Vec<String> = if let Some(ext) = extension {
            files.into_iter().filter(|f| f.ends_with(ext)).collect()
        } else {
            files
        };

        serde_json::to_string(&filtered)
            .map_err(|e| MojenticError::Tool {
                message: format!("Failed to serialize result: {}", e),
                source: Some(Box::new(e)),
            })
    }
}

/// Tool for reading the entire content of a file.
pub struct ReadFileTool {
    fs: FilesystemGateway,
}

impl ReadFileTool {
    pub fn new(fs: FilesystemGateway) -> Self {
        Self { fs }
    }
}

impl Tool for ReadFileTool {
    fn descriptor(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read the entire content of a file as a string. Use this when you need to access or analyze the complete contents of a file.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The full relative path including the filename of the file to read. For example, 'README.md' for a file in the root directory, 'src/main.py' for a file in the src directory, or 'docs/images/diagram.png' for a file in a nested directory."
                        }
                    },
                    "additionalProperties": false,
                    "required": ["path"]
                }
            }
        })
    }

    fn run(&self, args: Value) -> Result<String> {
        let path = args["path"].as_str()
            .ok_or_else(|| MojenticError::Tool {
                message: "Missing 'path' parameter".to_string(),
                source: None,
            })?;

        let (directory, file_name) = split_path(path);
        self.fs.read(directory, file_name)
    }
}

/// Tool for writing content to a file, completely overwriting any existing content.
pub struct WriteFileTool {
    fs: FilesystemGateway,
}

impl WriteFileTool {
    pub fn new(fs: FilesystemGateway) -> Self {
        Self { fs }
    }
}

impl Tool for WriteFileTool {
    fn descriptor(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Write content to a file, completely overwriting any existing content. Use this when you want to replace the entire contents of a file with new content.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The full relative path including the filename where the file should be written. For example, 'output.txt' for a file in the root directory, 'src/main.py' for a file in the src directory, or 'docs/images/diagram.png' for a file in a nested directory."
                        },
                        "content": {
                            "type": "string",
                            "description": "The content to write to the file. This will completely replace any existing content in the file. For example, 'Hello, world!' for a simple text file, or a JSON string for a configuration file."
                        }
                    },
                    "additionalProperties": false,
                    "required": ["path", "content"]
                }
            }
        })
    }

    fn run(&self, args: Value) -> Result<String> {
        let path = args["path"].as_str()
            .ok_or_else(|| MojenticError::Tool {
                message: "Missing 'path' parameter".to_string(),
                source: None,
            })?;

        let content = args["content"].as_str()
            .ok_or_else(|| MojenticError::Tool {
                message: "Missing 'content' parameter".to_string(),
                source: None,
            })?;

        let (directory, file_name) = split_path(path);
        self.fs.write(directory, file_name, content)?;
        Ok(format!("Successfully wrote to {}", path))
    }
}

/// Tool for listing all files recursively in a directory.
pub struct ListAllFilesTool {
    fs: FilesystemGateway,
}

impl ListAllFilesTool {
    pub fn new(fs: FilesystemGateway) -> Self {
        Self { fs }
    }
}

impl Tool for ListAllFilesTool {
    fn descriptor(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "list_all_files",
                "description": "List all files recursively in the specified directory, including files in subdirectories. Use this when you need a complete inventory of all files in a directory and its subdirectories.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The path relative to the sandbox root to list files from recursively. For example, '.' for the root directory and all subdirectories, 'src' for the src directory and all its subdirectories, or 'docs/images' for a nested directory and its subdirectories."
                        }
                    },
                    "additionalProperties": false,
                    "required": ["path"]
                }
            }
        })
    }

    fn run(&self, args: Value) -> Result<String> {
        let path = args["path"].as_str()
            .ok_or_else(|| MojenticError::Tool {
                message: "Missing 'path' parameter".to_string(),
                source: None,
            })?;

        let files = self.fs.list_all_files(path)?;

        serde_json::to_string(&files)
            .map_err(|e| MojenticError::Tool {
                message: format!("Failed to serialize result: {}", e),
                source: Some(Box::new(e)),
            })
    }
}

/// Tool for finding files matching a glob pattern.
pub struct FindFilesByGlobTool {
    fs: FilesystemGateway,
}

impl FindFilesByGlobTool {
    pub fn new(fs: FilesystemGateway) -> Self {
        Self { fs }
    }
}

impl Tool for FindFilesByGlobTool {
    fn descriptor(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "find_files_by_glob",
                "description": "Find files matching a glob pattern in the specified directory. Use this when you need to locate files with specific patterns in their names or paths (e.g., all Python files with '*.py' or all text files in any subdirectory with '**/*.txt').",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The path relative to the sandbox root to search in. For example, '.' for the root directory, 'src' for the src directory, or 'docs/images' for a nested directory."
                        },
                        "pattern": {
                            "type": "string",
                            "description": "The glob pattern to match files against. Examples: '*.py' for all Python files in the specified directory, '**/*.txt' for all text files in the specified directory and any subdirectory, or '**/*test*.py' for all Python files with 'test' in their name in the specified directory and any subdirectory."
                        }
                    },
                    "additionalProperties": false,
                    "required": ["path", "pattern"]
                }
            }
        })
    }

    fn run(&self, args: Value) -> Result<String> {
        let path = args["path"].as_str()
            .ok_or_else(|| MojenticError::Tool {
                message: "Missing 'path' parameter".to_string(),
                source: None,
            })?;

        let pattern = args["pattern"].as_str()
            .ok_or_else(|| MojenticError::Tool {
                message: "Missing 'pattern' parameter".to_string(),
                source: None,
            })?;

        let files = self.fs.find_files_by_glob(path, pattern)?;

        serde_json::to_string(&files)
            .map_err(|e| MojenticError::Tool {
                message: format!("Failed to serialize result: {}", e),
                source: Some(Box::new(e)),
            })
    }
}

/// Tool for finding files containing text matching a regex pattern.
pub struct FindFilesContainingTool {
    fs: FilesystemGateway,
}

impl FindFilesContainingTool {
    pub fn new(fs: FilesystemGateway) -> Self {
        Self { fs }
    }
}

impl Tool for FindFilesContainingTool {
    fn descriptor(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "find_files_containing",
                "description": "Find files containing text matching a regex pattern in the specified directory. Use this when you need to search for specific content across multiple files, such as finding all files that contain a particular function name or text string.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The path relative to the sandbox root to search in. For example, '.' for the root directory, 'src' for the src directory, or 'docs/images' for a nested directory."
                        },
                        "pattern": {
                            "type": "string",
                            "description": "The regex pattern to search for in files. Examples: 'function\\s+main' to find files containing a main function, 'import\\s+os' to find files importing the os module, or 'TODO|FIXME' to find files containing TODO or FIXME comments. The pattern uses Rust's regex crate syntax."
                        }
                    },
                    "additionalProperties": false,
                    "required": ["path", "pattern"]
                }
            }
        })
    }

    fn run(&self, args: Value) -> Result<String> {
        let path = args["path"].as_str()
            .ok_or_else(|| MojenticError::Tool {
                message: "Missing 'path' parameter".to_string(),
                source: None,
            })?;

        let pattern = args["pattern"].as_str()
            .ok_or_else(|| MojenticError::Tool {
                message: "Missing 'pattern' parameter".to_string(),
                source: None,
            })?;

        let files = self.fs.find_files_containing(path, pattern)?;

        serde_json::to_string(&files)
            .map_err(|e| MojenticError::Tool {
                message: format!("Failed to serialize result: {}", e),
                source: Some(Box::new(e)),
            })
    }
}

/// Tool for finding all lines in a file matching a regex pattern.
pub struct FindLinesMatchingTool {
    fs: FilesystemGateway,
}

impl FindLinesMatchingTool {
    pub fn new(fs: FilesystemGateway) -> Self {
        Self { fs }
    }
}

impl Tool for FindLinesMatchingTool {
    fn descriptor(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "find_lines_matching",
                "description": "Find all lines in a file matching a regex pattern, returning both line numbers and content. Use this when you need to locate specific patterns within a single file and need to know exactly where they appear.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The full relative path including the filename of the file to search in. For example, 'README.md' for a file in the root directory, 'src/main.py' for a file in the src directory, or 'docs/images/diagram.png' for a file in a nested directory."
                        },
                        "pattern": {
                            "type": "string",
                            "description": "The regex pattern to match lines against. Examples: 'def\\s+\\w+' to find all function definitions, 'class\\s+\\w+' to find all class definitions, or 'TODO|FIXME' to find all TODO or FIXME comments. The pattern uses Rust's regex crate syntax."
                        }
                    },
                    "additionalProperties": false,
                    "required": ["path", "pattern"]
                }
            }
        })
    }

    fn run(&self, args: Value) -> Result<String> {
        let path = args["path"].as_str()
            .ok_or_else(|| MojenticError::Tool {
                message: "Missing 'path' parameter".to_string(),
                source: None,
            })?;

        let pattern = args["pattern"].as_str()
            .ok_or_else(|| MojenticError::Tool {
                message: "Missing 'pattern' parameter".to_string(),
                source: None,
            })?;

        let (directory, file_name) = split_path(path);
        let lines = self.fs.find_lines_matching(directory, file_name, pattern)?;

        serde_json::to_string(&lines)
            .map_err(|e| MojenticError::Tool {
                message: format!("Failed to serialize result: {}", e),
                source: Some(Box::new(e)),
            })
    }
}

/// Tool for creating a new directory.
pub struct CreateDirectoryTool {
    fs: FilesystemGateway,
}

impl CreateDirectoryTool {
    pub fn new(fs: FilesystemGateway) -> Self {
        Self { fs }
    }
}

impl Tool for CreateDirectoryTool {
    fn descriptor(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "create_directory",
                "description": "Create a new directory at the specified path. If the directory already exists, this operation will succeed without error. Use this when you need to create a directory structure before writing files to it.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The relative path where the directory should be created. For example, 'new_folder' for a directory in the root, 'src/new_folder' for a directory in the src directory, or 'docs/images/new_folder' for a nested directory. Parent directories will be created automatically if they don't exist."
                        }
                    },
                    "additionalProperties": false,
                    "required": ["path"]
                }
            }
        })
    }

    fn run(&self, args: Value) -> Result<String> {
        let path = args["path"].as_str()
            .ok_or_else(|| MojenticError::Tool {
                message: "Missing 'path' parameter".to_string(),
                source: None,
            })?;

        let resolved_path = self.fs.resolve_path(path)?;
        fs::create_dir_all(&resolved_path)?;
        Ok(format!("Successfully created directory '{}'", path))
    }
}

fn split_path(path: &str) -> (&str, &str) {
    let path_obj = Path::new(path);
    let directory = path_obj.parent()
        .and_then(|p| p.to_str())
        .unwrap_or(".");
    let file_name = path_obj.file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("");
    (directory, file_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_filesystem_gateway_new() {
        let temp_dir = TempDir::new().unwrap();
        let gateway = FilesystemGateway::new(temp_dir.path());
        assert!(gateway.is_ok());
    }

    #[test]
    fn test_resolve_path_security() {
        let temp_dir = TempDir::new().unwrap();
        let gateway = FilesystemGateway::new(temp_dir.path()).unwrap();

        // Should fail - trying to escape sandbox
        let result = gateway.resolve_path("../../../etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_ls() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        let gateway = FilesystemGateway::new(temp_dir.path()).unwrap();
        let files = gateway.ls(".").unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].contains("test.txt"));
    }

    #[test]
    fn test_read_write() {
        let temp_dir = TempDir::new().unwrap();
        let gateway = FilesystemGateway::new(temp_dir.path()).unwrap();

        gateway.write(".", "test.txt", "Hello, world!").unwrap();
        let content = gateway.read(".", "test.txt").unwrap();

        assert_eq!(content, "Hello, world!");
    }
}
