// src/modules/agent/tools/utils.rs
use super::super::{Tool, ToolError};
use async_trait::async_trait;
use reqwest;
use serde_json::{Value, json};

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
        let result = reqwest::get(target_url)
            .await
            .map_err(|e| ToolError::ExecutionError(format!("Failed to fetch Failed: {}", e)))?;
        let result = result
            .text()
            .await
            .map_err(|e| ToolError::ExecutionError(format!("Failed to get text: {}", e)))?;
        Ok(json!({
            "result": result,
        }))
    }
}
