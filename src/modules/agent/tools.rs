// src/modules/agent/tools.rs (previously src/modules/tools.rs, assuming it was moved/renamed)
pub mod files;
pub mod shell;
pub mod utils;
pub mod www;
use async_trait::async_trait;
use std::collections::HashMap;
use std::io::Error as IoError;

// Import ApiError so we can use it in ToolError
use crate::modules::agent::api::ApiError;

// Tool のエラー型
#[derive(Debug)]
pub enum ToolError {
    ShellError(String),
    NotFound(String),
    ExecutionError(String),
    SerializationError(String),
    #[allow(dead_code)]
    DeserializationError(String),
    Io(IoError),
    Api(ApiError),
}

// Implement Display for better error messages when printed
impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::ShellError(msg) => write!(f, "Shell error: {}", msg),
            ToolError::NotFound(msg) => write!(f, "Tool not found: {}", msg),
            ToolError::ExecutionError(msg) => write!(f, "Tool execution error: {}", msg),
            ToolError::SerializationError(msg) => write!(f, "Tool serialization error: {}", msg),
            ToolError::DeserializationError(msg) => {
                write!(f, "Tool deserialization error: {}", msg)
            }
            ToolError::Io(e) => write!(f, "Tool IO error: {}", e),
            ToolError::Api(e) => write!(f, "API error in tool context: {}", e),
        }
    }
}

// Implement std::error::Error for ToolError
impl std::error::Error for ToolError {}

impl From<serde_json::Error> for ToolError {
    fn from(err: serde_json::Error) -> Self {
        ToolError::SerializationError(err.to_string())
    }
}

impl From<IoError> for ToolError {
    fn from(err: IoError) -> Self {
        ToolError::Io(err)
    }
}

impl From<ApiError> for ToolError {
    fn from(err: ApiError) -> Self {
        ToolError::Api(err)
    }
}

/// AIが呼び出せる個々のツールを表すトレイト
#[async_trait]
pub trait Tool: Send + Sync {
    /// ツールの名前（AIが参照するID）
    fn name(&self) -> &'static str;
    /// ツールの説明（AIがいつ使うべきかを判断するための情報）
    fn description(&self) -> &'static str;
    /// ツールの引数のJSONスキーマ（AIが正しい形式で引数を渡せるように）
    fn parameters(&self) -> serde_json::Value;

    /// ツールを実行する非同期メソッド
    async fn execute(&self, args: serde_json::Value) -> Result<serde_json::Value, ToolError>;
}

/// ツールを管理する構造体
pub struct ToolManager {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl Default for ToolManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolManager {
    pub fn new() -> Self {
        ToolManager {
            tools: HashMap::new(),
        }
    }

    /// ツールを登録する
    pub fn register_tool<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name().to_string(), Box::new(tool));
    }

    /// 名前でツールを取得する
    pub fn get_tool(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|b| b.as_ref())
    }

    /// ツールをYAMLスキーマ形式で取得する（プロンプトに埋め込むため）
    pub fn get_tool_yaml_schemas(&self) -> serde_yaml::Value {
        let tool_definitions: Vec<serde_yaml::Value> = self
            .tools
            .values()
            .map(|tool| {
                serde_yaml::to_value(serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name(),
                        "description": tool.description(),
                        "parameters": tool.parameters(),
                    }
                }))
                .unwrap_or_else(|_| serde_yaml::Value::Null)
            })
            .collect();
        serde_yaml::to_value(tool_definitions)
            .unwrap_or_else(|_| serde_yaml::Value::Sequence(vec![]))
    }

    /// ツールを実行する
    pub async fn execute_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, ToolError> {
        if let Some(tool) = self.get_tool(name) {
            tool.execute(args).await
        } else {
            let available_tools: Vec<String> = self.tools.keys().cloned().collect();
            let error_message = format!(
                "Tool '{}' not found. Available tools are: [{}]",
                name,
                available_tools.join(", ")
            );
            Err(ToolError::NotFound(error_message))
        }
    }
}
