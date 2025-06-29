// src/modules/agent/tools/utils.rs
use super::super::{Tool, ToolError};
use async_trait::async_trait;
use serde_json::{Value, json};
use www_search::{EngineType, SearchData, www_search}; // www-search クレートをインポート

pub struct SearchEngineTool;

#[async_trait]
impl Tool for SearchEngineTool {
    fn name(&self) -> &'static str {
        "websearch"
    }

    fn description(&self) -> &'static str {
        "Use WWW Search Engine like Google, DuckDuckGo to get search results. Provide a 'query' and optionally an 'engine' (google or duckduckgo)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { // Web検索のクエリ。
                    "type": "string",
                    "description": "The query you are trying to find"
                },
                "engine": {
                    "type": "string",
                    "description": "The search engine to use. Can be 'google' or 'duckduckgo'. Defaults to 'google'."
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let search_query = args["query"]
            .as_str()
            .ok_or_else(|| {
                ToolError::ExecutionError("Missing 'query' argument for search tool.".to_string())
            })?
            .to_string();

        let engine = args["engine"].as_str().unwrap_or("google"); // デフォルトはGoogle
        let engine = match engine {
            "google" => EngineType::Google,
            "duckduckgo" => EngineType::DuckDuckGo,
            _ => {
                return Err(ToolError::ExecutionError(format!(
                    "Unsupported search engine: {}. Supported engines are 'google' and 'duckduckgo'.",
                    engine
                )));
            }
        };
        let results: Vec<SearchData> = www_search(engine, search_query)
            .await
            .map_err(|e| ToolError::ExecutionError(format!("Google search failed: {}", e)))?;

        // 検索結果をJSON形式に変換
        let formatted_results: Vec<Value> = results
            .into_iter()
            .map(|data| {
                json!({
                    "title": data.title,
                    "url": data.url,
                    "description": data.description,
                })
            })
            .collect();

        Ok(json!({
            "results": formatted_results,
            "success": true
        }))
    }
}
