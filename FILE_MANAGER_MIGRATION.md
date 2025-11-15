# File Manager Migration Guide

## Status

The `file_manager.rs` module is **complete but disabled** due to needing trait migration from the old `Tool` trait to the new `LlmTool` trait. This document provides step-by-step instructions for completing the migration.

## Why is it disabled?

The file_manager module was written for an older `Tool` trait that no longer exists in the codebase. The trait was refactored to `LlmTool` with a different API signature. Rather than leaving broken code in the build, the module is temporarily disabled in `src/llm/tools/mod.rs`.

## What needs to be done?

The migration consists of 5 systematic changes across all tool implementations in `file_manager.rs`:

### 1. Update imports (line ~6)

**Before:**
```rust
use crate::llm::tools::Tool;
```

**After:**
```rust
use crate::llm::tools::{LlmTool, ToolDescriptor, FunctionDescriptor};
use std::collections::HashMap;
```

### 2. Update trait implementations

Find all occurrences of:
```rust
impl Tool for SomeTool {
```

Replace with:
```rust
impl LlmTool for SomeTool {
```

There are 8 tool structs to update:
- `ListFilesTool`
- `ReadFileTool`
- `WriteFileTool`
- `ListAllFilesTool`
- `FindFilesByGlobTool`
- `FindFilesContainingTool`
- `FindLinesMatchingTool`
- `CreateDirectoryTool`

### 3. Update descriptor() method

The `descriptor()` method needs both signature and implementation changes.

**Before:**
```rust
fn descriptor(&self) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "tool_name",
            "description": "Tool description",
            "parameters": {
                "type": "object",
                "properties": { ... },
                "required": [...]
            }
        }
    })
}
```

**After:**
```rust
fn descriptor(&self) -> ToolDescriptor {
    ToolDescriptor {
        r#type: "function".to_string(),
        function: FunctionDescriptor {
            name: "tool_name".to_string(),
            description: "Tool description".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { ... },
                "required": [...]
            }),
        },
    }
}
```

**Key changes:**
- Return type: `Value` → `ToolDescriptor`
- Structure: JSON value → struct with fields
- Fields: `name`, `description` become `.to_string()`
- `parameters` remains as `json!()` macro

### 4. Update run() method

The `run()` method has both signature and implementation changes.

**Before:**
```rust
fn run(&self, args: Value) -> Result<String> {
    let param = args["param"].as_str()
        .ok_or_else(|| MojenticError::Tool {
            message: "Missing 'param' parameter".to_string(),
            source: None,
        })?;

    // Do work...
    let result = some_work()?;

    Ok(result)  // Returns String directly
}
```

**After:**
```rust
fn run(&self, args: &HashMap<String, Value>) -> Result<Value> {
    let param = args.get("param")
        .and_then(|v| v.as_str())
        .ok_or_else(|| MojenticError::ToolError("Missing 'param' parameter".to_string()))?;

    // Do work...
    let result = some_work()?;

    Ok(json!(result))  // Wrap result in json!()
}
```

**Key changes:**
- Parameter type: `Value` → `&HashMap<String, Value>`
- Return type: `Result<String>` → `Result<Value>`
- Argument access: `args["key"]` → `args.get("key").and_then(|v| v.as_str())`
- Return value: `Ok(result)` → `Ok(json!(result))`
- Error type: `MojenticError::Tool { message, source }` → `MojenticError::ToolError(message)`

### 5. Fix error construction

Find all occurrences of:
```rust
MojenticError::Tool {
    message: format!("error message"),
    source: None,
}
```

Replace with:
```rust
MojenticError::ToolError(format!("error message"))
```

This also affects error handling in `FilesystemGateway` methods (not just the tools).

## Reference Implementation

See `src/llm/tools/simple_date_tool.rs` for a complete working example of the `LlmTool` trait pattern. This shows:
- Proper imports
- `impl LlmTool for SimpleDateTool`
- Correct `descriptor()` returning `ToolDescriptor`
- Correct `run()` taking `&HashMap` and returning `Result<Value>`
- Proper error handling with `MojenticError::ToolError`

## Tools to assist migration

### Search and replace patterns

1. **Trait name:**
   ```bash
   sed -i 's/impl Tool for /impl LlmTool for /g' src/llm/tools/file_manager.rs
   ```

2. **Error type (simple cases):**
   ```bash
   # This catches simple cases, but complex multi-line errors need manual fixing
   sed -i 's/MojenticError::Tool {/MojenticError::ToolError(/g' src/llm/tools/file_manager.rs
   ```

3. **Method signatures:**
   ```bash
   sed -i 's/fn descriptor(&self) -> Value {/fn descriptor(&self) -> ToolDescriptor {/g' src/llm/tools/file_manager.rs
   sed -i 's/fn run(&self, args: Value) -> Result<String> {/fn run(&self, args: \&HashMap<String, Value>) -> Result<Value> {/g' src/llm/tools/file_manager.rs
   ```

**Note:** These sed commands are helpers but won't complete the migration. Manual editing is required for:
- Converting descriptor body from `json!()` to `ToolDescriptor { ... }`
- Changing argument access from `args["key"]` to `args.get("key")`
- Wrapping return values in `Ok(json!(...))`
- Fixing multi-line error constructions

### Verification steps

After making changes:

1. **Enable the module:**
   ```rust
   // In src/llm/tools/mod.rs
   pub mod file_manager;
   ```

2. **Build:**
   ```bash
   cargo build --all-features
   ```

3. **Run tests:**
   ```bash
   cargo test
   ```

4. **Build examples:**
   ```bash
   cargo build --examples
   ```

5. **Try the file_tool example:**
   ```bash
   cargo run --example file_tool
   ```

## Estimated effort

- **Reading documentation:** 15 minutes
- **Making changes:** 2-3 hours (careful, systematic work)
- **Testing and debugging:** 1-2 hours

**Total:** 4-6 hours for a careful, complete migration.

## Why not automated?

While parts of this migration could be automated with sophisticated AST transformations, the manual approach is recommended because:

1. **Learning opportunity:** Understanding the trait differences helps with future Rust work
2. **Verification:** Manual review catches subtle bugs that automated tools miss
3. **Complexity:** Descriptor body transformation (JSON → struct) is non-trivial
4. **Testing:** Each tool should be tested after conversion

## Next steps after migration

Once `file_manager.rs` is working:

1. ✅ Re-enable in `src/llm/tools/mod.rs`
2. ✅ Run full test suite
3. ✅ Update `examples/file_tool.rs` to use real tools instead of placeholder
4. ✅ Update `examples/coding_file_tool.rs` to use real file + task tools
5. ✅ Update PARITY.md to mark Level 3 file tools as complete
6. ✅ Commit with message: "feat: Migrate file_manager to LlmTool trait"

## Questions?

See the Python reference implementation:
- `mojentic-py/src/mojentic/llm/tools/file_manager.py`

Or the Elixir implementation:
- `mojentic-ex/lib/mojentic/llm/tools/file_manager.ex`

Both show the expected behavior and can help clarify intent.
