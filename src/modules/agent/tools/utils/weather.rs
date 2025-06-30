// src/modules/agent/tools/utils.rs
use super::super::{Tool, ToolError};
use async_trait::async_trait;
use serde_json::{Value, json};
use www_search::browse::fetch_and_markdown;
pub struct WeatherTool;

#[async_trait]
impl Tool for WeatherTool {
    fn name(&self) -> &'static str {
        "weathertool"
    }

    fn description(&self) -> &'static str {
        "get weather information"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "location": { // WebページのURL
                    "type": "string",
                    "description": "The location you are trying to check. For example, London, Tokyo, etc... if none, automatically detected"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let target_url = format!(
            "https://wttr.in/{}",
            args["location"].as_str().unwrap_or("")
        );
        let result = fetch_and_markdown(&target_url)
            .await
            .map_err(|e| ToolError::ExecutionError(format!("Failed to fetch Failed: {}", e)))?;
        Ok(json!({
            "result": result,
        }))
    }
}
