// src/modules/agent/tools/files/write.rs
use crate::modules::agent::tools::{Tool, ToolError};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::fs::File;
use std::io::{LineWriter, Write};
use std::path::Path;

pub struct WriteTool;

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &'static str {
        "file_write"
    }

    fn description(&self) -> &'static str {
        "Writes or overwrites content to a file at a given path. The content can be provided as a single string or as an array of objects, each specifying a line number and content."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The path to the file to be written."
                },
                "content": {
                    "type": ["string", "array"],
                    "description": "The content to write. Can be a single string to overwrite the entire file, or an array of line-content pairs.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "line": {
                                "type": "integer",
                                "description": "The line number to write to (1-indexed)."
                            },
                            "content": {
                                "type": "string",
                                "description": "The content to write on that line."
                            }
                        },
                        "required": ["line", "content"]
                    }
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let path_str = args["path"]
            .as_str()
            .ok_or_else(|| ToolError::ExecutionError("Missing 'path' argument.".to_string()))?;
        let path = Path::new(path_str);

        let file = File::create(path).map_err(|e| {
            ToolError::ExecutionError(format!("Failed to create or open file: {}", e))
        })?;
        let mut writer = LineWriter::new(file);

        match &args["content"] {
            Value::String(content) => {
                writer.write_all(content.as_bytes()).map_err(|e| {
                    ToolError::ExecutionError(format!("Failed to write to file: {}", e))
                })?;
            }
            Value::Array(lines_to_write) => {
                // This implementation is simplified. A more robust version would handle line-specific writes.
                // For now, we'll just write the lines sequentially.
                for item in lines_to_write {
                    if let Some(content) = item["content"].as_str() {
                        writeln!(writer, "{}", content).map_err(|e| {
                            ToolError::ExecutionError(format!("Failed to write line: {}", e))
                        })?;
                    }
                }
            }
            _ => return Err(ToolError::ExecutionError(
                "Invalid 'content' format. Must be a string or an array of line-content objects."
                    .to_string(),
            )),
        }

        Ok(json!({
            "success": true,
            "message": format!("Successfully wrote to file: {}", path_str)
        }))
    }
}
