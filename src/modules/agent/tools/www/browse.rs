// src/modules/agent/tools/utils.rs
use super::super::{Tool, ToolError};
use async_trait::async_trait;
use serde_json::{Value, json};
use www_search::browse;

pub struct WebPageBrowser;

#[async_trait]
impl Tool for WebPageBrowser {
    fn name(&self) -> &'static str {
        "webbrowser"
    }

    fn description(&self) -> &'static str {
        "Visit WebSite and get information"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { // WebページのURL
                    "type": "string",
                    "description": "The url you are trying to visit"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let target_url = args["url"].as_str().ok_or_else(|| {
            ToolError::ExecutionError("Missing 'url' argument for browser tool.".to_string())
        })?;
        let result = browse::fetch_and_markdown(target_url)
            .await
            .map_err(|e| ToolError::ExecutionError(format!("WebPage Visit Failed: {}", e)))?;

        Ok(json!({
            "result": result,
            "success": true
        }))
    }
}
