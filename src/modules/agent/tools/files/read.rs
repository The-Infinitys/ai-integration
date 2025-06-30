// src/modules/agent/tools/files/read.rs
use crate::modules::agent::tools::{Tool, ToolError};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub struct ReadTool;

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &'static str {
        "file_read"
    }

    fn description(&self) -> &'static str {
        "Reads the content of a file at a given path, optionally within a specified line range. Returns the content as an array of strings, with each string representing a line."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The absolute or relative path to the file to be read."
                },
                "range": {
                    "type": "object",
                    "description": "An optional range of lines to read.",
                    "properties": {
                        "start": {
                            "type": "integer",
                            "description": "The starting line number (1-indexed)."
                        },
                        "end": {
                            "type": "integer",
                            "description": "The ending line number (inclusive)."
                        }
                    }
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let path_str = args["path"].as_str().ok_or_else(|| ToolError::ExecutionError("Missing 'path' argument.".to_string()))?;
        let path = Path::new(path_str);

        if !path.exists() {
            return Err(ToolError::ExecutionError(format!("File not found at path: {}", path_str)));
        }

        let file = File::open(path).map_err(|e| ToolError::ExecutionError(format!("Failed to open file: {}", e)))?;
        let reader = BufReader::new(file);

        let lines: Result<Vec<String>, _> = reader.lines().collect();
        let mut lines = lines.map_err(|e| ToolError::ExecutionError(format!("Failed to read lines: {}", e)))?;

        if let Some(range) = args.get("range") {
            let start = range["start"].as_u64().unwrap_or(1).saturating_sub(1) as usize;
            let end = range["end"].as_u64().map(|v| v as usize).unwrap_or(lines.len());

            if start < lines.len() && start <= end {
                lines = lines.drain(start..std::cmp::min(end, lines.len())).collect();
            } else {
                lines.clear();
            }
        }

        Ok(json!({
            "lines": lines
        }))
    }
}