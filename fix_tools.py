#!/usr/bin/env python3
"""Fix file_manager.rs tools to match LlmTool trait"""

import re

def fix_file_manager():
    with open('src/llm/tools/file_manager.rs', 'r') as f:
        content = f.read()
    
    # Fix WriteFileTool
    old_write = r'''impl LlmTool for WriteFileTool \{
    fn descriptor\(&self\) -> ToolDescriptor \{
        json!\(\{
            "type": "function",
            "function": \{
                "name": "write_file",
                "description": "Write content to a file, completely overwriting any existing content\. Use this when you want to replace the entire contents of a file with new content\.",
                "parameters": \{
                    "type": "object",
                    "properties": \{
                        "path": \{
                            "type": "string",
                            "description": "The full relative path including the filename where the file should be written\. For example, 'output\.txt' for a file in the root directory, 'src/main\.py' for a file in the src directory, or 'docs/images/diagram\.png' for a file in a nested directory\."
                        \},
                        "content": \{
                            "type": "string",
                            "description": "The content to write to the file\. This will completely replace any existing content in the file\. For example, 'Hello, world!' for a simple text file, or a JSON string for a configuration file\."
                        \}
                    \},
                    "additionalProperties": false,
                    "required": \["path", "content"\]
                \}
            \}
        \}\)
    \}

    fn run\(&self, args: &HashMap<String, Value>\) -> Result<Value> \{'''
    
    new_write = '''impl LlmTool for WriteFileTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "write_file".to_string(),
                description: "Write content to a file, completely overwriting any existing content. Use this when you want to replace the entire contents of a file with new content.".to_string(),
                parameters: json!({
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
                }),
            },
        }
    }

    fn run(&self, args: &HashMap<String, Value>) -> Result<Value> {'''
    
    content = re.sub(old_write, new_write, content, flags=re.DOTALL)
    
    # Fix WriteFileTool run return
    content = re.sub(
        r'Ok\(format!\("Successfully wrote to \{\}", path\)\)',
        r'Ok(json!(format!("Successfully wrote to {}", path)))',
        content
    )
    
    # Fix CreateDirectoryTool run return
    content = re.sub(
        r"Ok\(format!\(\"Successfully created directory '{}'\", path\)\)",
        r'Ok(json!(format!("Successfully created directory \'{}\'", path)))',
        content
    )
    
    # Fix serde_json::to_string(&...) to Ok(json!(...))
    content = re.sub(
        r'serde_json::to_string\(&([^)]+)\)\s*\.map_err\([^)]+\)',
        r'Ok(json!(\1))',
        content
    )
    
    # Fix error patterns - remove extra source field
    content = re.sub(
        r'MojenticError::ToolError\(([^)]+)\)\s*\n\s*source: None,',
        r'MojenticError::ToolError(\1)',
        content
    )
    
    with open('src/llm/tools/file_manager.rs', 'w') as f:
        f.write(content)
    
    print("Fixed file_manager.rs")

if __name__ == '__main__':
    fix_file_manager()
