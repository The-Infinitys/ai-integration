// src/modules/agent.rs
pub mod api;
pub mod tools;

use api::{ChatMessage, ChatRole, OllamaApiError};
use futures_util::stream::{Stream, StreamExt, once};
use std::pin::Pin;
use std::boxed::Box;
use std::fs;
use serde_json::Value;

use tools::{ToolManager}; // ToolError は現在直接使われていないが、将来的に必要になる可能性があるので残しておく


// ツール呼び出しの構造体 (AIが生成するJSONをパースするため)
#[derive(Debug, serde::Deserialize)]
struct ToolCall {
    tool_name: String,
    parameters: Value,
}

// AIの応答を解析するための全体構造
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)] // いずれかのフィールドが一致すればパースする
enum AgentResponseContent {
    ToolCall {
        call_tool: ToolCall,
    },
    #[allow(dead_code)]
    Text(String), // テキスト応答
}


pub struct AIAgent {
    api: api::AIApi,
    messages: Vec<ChatMessage>,
    tool_manager: ToolManager,
    default_prompt_template: String,
}

impl AIAgent {
    pub fn new(base_url: String, default_model: String) -> Self {
        let api = api::AIApi::new(base_url, default_model);
        let mut tool_manager = ToolManager::new();

        tool_manager.register_tool(tools::shell::ShellTool);

        let default_prompt_template = fs::read_to_string("src/modules/agent/default-prompt.md")
            .expect("default-prompt.md not found. Please create it.");

        AIAgent {
            api,
            messages: vec![],
            tool_manager,
            default_prompt_template,
        }
    }

    pub async fn list_models(&self) -> Result<serde_json::Value, OllamaApiError> {
        self.api.list_models().await
    }

    pub async fn chat_with_tools(&mut self, user_content: String) -> Result<Pin<Box<dyn Stream<Item = Result<String, OllamaApiError>> + Send>>, OllamaApiError> {
        // ユーザーメッセージを追加
        self.messages.push(ChatMessage {
            role: ChatRole::User,
            content: user_content,
        });

        // 現在のメッセージとツール定義を結合してプロンプトを構築
        let mut full_messages_for_api = self.messages.clone();

        // システムプロンプトを先頭に追加（ツール定義を埋め込む）
        let tool_schemas = self.tool_manager.get_tool_json_schemas();
        let formatted_prompt = self.default_prompt_template.replace("{{TOOLS_JSON_SCHEMA}}", &tool_schemas.to_string());

        // 既存のシステムメッセージがあれば上書き、なければ追加
        if let Some(msg) = full_messages_for_api.get_mut(0) {
            if msg.role == ChatRole::System {
                msg.content = formatted_prompt;
            } else {
                full_messages_for_api.insert(0, ChatMessage {
                    role: ChatRole::System,
                    content: formatted_prompt,
                });
            }
        } else {
            full_messages_for_api.insert(0, ChatMessage {
                role: ChatRole::System,
                content: formatted_prompt,
            });
        }

        // AIからの応答を取得（ツール呼び出しの可能性あり）
        let mut ai_response_stream = self.api.get_chat_completion_stream(full_messages_for_api).await?;
        let mut full_ai_response_content = String::new();

        // ストリームから応答を収集
        while let Some(chunk_result) = ai_response_stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    full_ai_response_content.push_str(&chunk);
                },
                Err(e) => {
                    return Ok(once(async { Err(e) }).boxed());
                }
            }
        }

        // AIの応答をログに追加
        self.messages.push(ChatMessage {
            role: ChatRole::Assistant,
            content: full_ai_response_content.clone(),
        });

        // ツール呼び出しの解析
        let tool_output_stream: Pin<Box<dyn Stream<Item = Result<String, OllamaApiError>> + Send>> = if let Ok(parsed_content) = serde_json::from_str::<AgentResponseContent>(&full_ai_response_content) {
            if let AgentResponseContent::ToolCall { call_tool } = parsed_content {
                println!("\n--- ツール呼び出しを検出: {} (引数: {:?}) ---", call_tool.tool_name, call_tool.parameters);

                match self.tool_manager.execute_tool(&call_tool.tool_name, call_tool.parameters.clone()).await {
                    Ok(tool_result) => {
                        println!("--- ツール実行結果 ---");
                        println!("{}", serde_json::to_string_pretty(&tool_result).unwrap_or_default());
                        println!("---------------------");

                        let tool_output_message_content = serde_json::json!({
                            "tool_output": {
                                "tool_name": call_tool.tool_name,
                                "result": tool_result
                            }
                        }).to_string();

                        self.messages.push(ChatMessage {
                            role: ChatRole::System,
                            content: tool_output_message_content,
                        });

                        // Make final_ai_response_stream mutable here
                        let mut final_ai_response_stream = self.api.get_chat_completion_stream(self.messages.clone()).await?;
                        let mut final_full_ai_response_content = String::new();

                        while let Some(chunk_result) = final_ai_response_stream.next().await {
                            match chunk_result {
                                Ok(chunk) => {
                                    final_full_ai_response_content.push_str(&chunk);
                                },
                                Err(e) => {
                                    return Ok(once(async { Err(e) }).boxed());
                                }
                            }
                        }
                        self.messages.push(ChatMessage {
                            role: ChatRole::Assistant,
                            content: final_full_ai_response_content.clone(),
                        });
                        once(async move { Ok(final_full_ai_response_content) }).boxed()
                    },
                    Err(e) => {
                        eprintln!("ツール実行エラー: {:?}", e);
                        let error_message_content = serde_json::json!({
                            "tool_error": {
                                "tool_name": call_tool.tool_name,
                                "error": format!("{:?}", e)
                            }
                        }).to_string();

                        self.messages.push(ChatMessage {
                            role: ChatRole::System,
                            content: error_message_content,
                        });

                        once(async move { Ok(format!("ツール実行中にエラーが発生しました: {:?}", e)) }).boxed()
                    },
                }
            } else {
                once(async move { Ok(full_ai_response_content) }).boxed()
            }
        } else {
            once(async move { Ok(full_ai_response_content) }).boxed()
        };

        Ok(tool_output_stream)
    }

    pub fn add_ai_response(&mut self, ai_content: String) {
        self.messages.push(ChatMessage {
            role: ChatRole::Assistant,
            content: ai_content,
        });
    }

    pub fn revert_last_user_message(&mut self) {
        if self.messages.last().map_or(false, |m| m.role == ChatRole::User) {
            self.messages.pop();
        }
    }
}