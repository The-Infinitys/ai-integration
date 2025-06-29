// src/modules/tools/shell.rs
use super::{Tool, ToolError};
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::process::Command;

pub struct ShellTool;

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &'static str {
        "shell"
    }

    fn description(&self) -> &'static str {
        "Execute a shell command and return its stdout and stderr. Use this for general system operations."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute."
                },
                "args": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "Arguments to pass to the command."
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let command_str = args["command"].as_str()
            .ok_or_else(|| ToolError::ExecutionError("Missing 'command' argument for shell tool.".to_string()))?;

        let args_vec: Vec<String> = if let Some(args_val) = args["args"].as_array() {
            args_val.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        } else {
            vec![]
        };

        let output = Command::new(command_str)
            .args(&args_vec)
            .output()
            .await
            .map_err(|e| ToolError::ShellError(format!("Failed to execute command '{}': {}", command_str, e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        if output.status.success() {
            Ok(json!({
                "stdout": stdout,
                "stderr": stderr,
                "success": true
            }))
        } else {
            Ok(json!({
                "stdout": stdout,
                "stderr": stderr,
                "success": false,
                "exit_code": output.status.code().unwrap_or(-1)
            }))
        }
    }
}