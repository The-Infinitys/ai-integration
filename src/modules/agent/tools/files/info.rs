// src/modules/agent/tools/files/info.rs
use crate::modules::agent::tools::{Tool, ToolError};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

pub struct InfoTool;

#[async_trait]
impl Tool for InfoTool {
    fn name(&self) -> &'static str {
        "file_info"
    }

    fn description(&self) -> &'static str {
        "Retrieves detailed information about a file or directory at a given path."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The path to the file or directory."
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let path_str = args["path"].as_str().ok_or_else(|| ToolError::ExecutionError("Missing 'path' argument.".to_string()))?;
        let path = Path::new(path_str);

        if !path.exists() {
            return Err(ToolError::ExecutionError(format!("No such file or directory: {}", path_str)));
        }

        let metadata = fs::metadata(path).map_err(|e| ToolError::ExecutionError(format!("Failed to get metadata: {}", e)))?;

        let file_type = if metadata.is_dir() {
            "directory"
        } else if metadata.is_file() {
            "file"
        } else {
            "other"
        };

        let created = metadata.created().map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()).ok();
        let modified = metadata.modified().map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()).ok();

        Ok(json!({
            "path": path_str,
            "type": file_type,
            "size_bytes": metadata.len(),
            "permissions_readonly": metadata.permissions().readonly(),
            "created_timestamp": created,
            "modified_timestamp": modified
        }))
    }
}
