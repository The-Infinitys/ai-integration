// src/modules/tools/shell.rs
use super::{Tool, ToolError};
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::process::Command; // tokio::process::Command を使用

pub struct ShellTool;

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &'static str {
        "shell"
    }

    fn description(&self) -> &'static str {
        "Execute a shell command line and return its stdout and stderr. Use this for general system operations."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command_line": { // 引数を単一の command_line 文字列に変更
                    "type": "string",
                    "description": "The complete shell command line to execute, including command and arguments."
                }
            },
            "required": ["command_line"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let command_line = args["command_line"].as_str()
            .ok_or_else(|| ToolError::ExecutionError("Missing 'command_line' argument for shell tool.".to_string()))?;

        // コマンドライン文字列を空白で分割し、コマンド名と引数を抽出
        let parts: Vec<&str> = command_line.split_whitespace().collect();
        if parts.is_empty() {
            return Err(ToolError::ExecutionError("Empty command line provided.".to_string()));
        }

        let command_name = parts[0];
        let command_args = &parts[1..];

        let output = Command::new(command_name)
            .args(command_args)
            .output()
            .await
            .map_err(|e| ToolError::ShellError(format!("Failed to execute command '{}': {}", command_line, e)))?;

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