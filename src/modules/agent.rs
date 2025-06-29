// src/modules/agent.rs
pub mod api;
pub mod tools;

use anyhow::Result;
use api::{ChatMessage, ChatRole, OllamaApiError, AIApi}; // AIApi をインポート
use futures_util::stream::{Stream, StreamExt, once};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::boxed::Box;
use std::pin::Pin;
use colored::*;

use tools::ToolManager;

// AIがツール呼び出しを記述するYAMLの構造体 (変更なし)
#[derive(Debug, Deserialize, Serialize)]
struct AiToolCall {
    tool_name: String,
    parameters: Value,
}

// AIの応答からツール呼び出しを抽出・パースするヘルパー関数 (変更なし)
fn extract_tool_call_from_response(response_content: &str) -> Option<AiToolCall> {
    let start_block_marker = "---";
    let end_block_marker = "---";
    let tool_call_key = "tool_call:";

    let mut found_start = false;
    let mut block_content = String::new();

    for line in response_content.lines() {
        let trimmed_line = line.trim();

        if !found_start {
            if trimmed_line == start_block_marker {
                found_start = true;
            }
        } else {
            if trimmed_line == end_block_marker {
                break;
            }
            block_content.push_str(line);
            block_content.push('\n');
        }
    }

    if found_start && !block_content.is_empty() {
        if block_content.contains(tool_call_key) {
            #[derive(Debug, Deserialize)]
            struct OuterToolCall {
                tool_call: AiToolCall,
            }

            match serde_yaml::from_str::<OuterToolCall>(&block_content) {
                Ok(outer_call) => Some(outer_call.tool_call),
                Err(e) => {
                    eprintln!("Failed to parse YAML tool call: {}", e);
                    eprintln!("YAML content was:\n{}", block_content);
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    }
}

pub struct AIAgent {
    api: AIApi, // 修正: OllamaApi から AIApi に変更
    messages: Vec<ChatMessage>,
    pub tool_manager: ToolManager,
    default_prompt_template: String,
}

impl AIAgent {
    pub fn new(base_url: String, default_model: String) -> Self {
        let api = AIApi::new(base_url, default_model); // 修正: AIApi::new を呼び出し
        let mut tool_manager = ToolManager::new();

        tool_manager.register_tool(tools::shell::ShellTool);

        let default_prompt_template = include_str!("default-prompt.md").to_string();

        AIAgent {
            api,
            messages: vec![],
            tool_manager,
            default_prompt_template,
        }
    }

    // `list_available_models` はそのまま、内部で `self.api.list_models()` を呼び出す
    pub async fn list_available_models(&self) -> Result<serde_json::Value, OllamaApiError> {
        self.api.list_models().await
    }

    pub async fn chat_with_tools_realtime(
        &mut self,
        user_content: String,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, OllamaApiError>> + Send>>, OllamaApiError>
    {
        // ユーザーメッセージを追加 (変更なし)
        self.messages.push(ChatMessage {
            role: ChatRole::User,
            content: user_content,
        });

        // ツール利用のループ (変更なし)
        loop {
            let mut full_messages_for_api = self.messages.clone();

            // システムプロンプトを先頭に追加（ツール定義を埋め込む） (変更なし)
            let tool_schemas = self.tool_manager.get_tool_json_schemas();
            let formatted_prompt = self
                .default_prompt_template
                .replace("{{TOOLS_JSON_SCHEMA}}", &tool_schemas.to_string());

            if let Some(msg) = full_messages_for_api.get_mut(0) {
                if msg.role == ChatRole::System {
                    msg.content = formatted_prompt;
                } else {
                    full_messages_for_api.insert(
                        0,
                        ChatMessage {
                            role: ChatRole::System,
                            content: formatted_prompt,
                        },
                    );
                }
            } else {
                full_messages_for_api.insert(
                    0,
                    ChatMessage {
                        role: ChatRole::System,
                        content: formatted_prompt,
                    },
                );
            }

            // AIからの応答ストリームを取得
            // 修正: self.api.get_chat_completion_stream を呼び出す
            let mut ai_response_stream = self
                .api
                .get_chat_completion_stream(full_messages_for_api)
                .await?;
            
            let mut full_ai_response_content = String::new();
            let mut tool_block_detected = false;
            let mut tool_yaml_buffer = String::new();

            // ストリームから応答をリアルタイムで収集し、表示し、ツール呼び出しを検出 (変更なし)
            while let Some(chunk_result) = ai_response_stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        full_ai_response_content.push_str(&chunk);

                        if !tool_block_detected {
                            if chunk.contains("---") && chunk.contains("tool_call:") {
                                tool_block_detected = true;
                                println!("{}", "\n  --- ツール呼び出しを検出しようとしています... ---".truecolor(128, 128, 128)); // グレー
                                tool_yaml_buffer.push_str(&chunk);
                            }
                        } else {
                            tool_yaml_buffer.push_str(&chunk);
                            if chunk.contains("---") && tool_yaml_buffer.contains("tool_call:") {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        return Ok(once(async { Err(e) }).boxed());
                    }
                }
            }

            // AIの応答を履歴に追加 (変更なし)
            self.messages.push(ChatMessage {
                role: ChatRole::Assistant,
                content: full_ai_response_content.clone(),
            });

            // ツール呼び出しを解析 (変更なし)
            if tool_block_detected {
                if let Some(call_tool) = extract_tool_call_from_response(&tool_yaml_buffer) {
                    println!(
                        "{}",
                        format!("\n--- ツール呼び出しを検出: {} (引数: {:?}) ---",
                            call_tool.tool_name, call_tool.parameters).truecolor(128, 128, 128) // グレー
                    );
                    
                    println!("{}", "  ツールを実行中...".truecolor(128, 128, 128)); // グレー

                    match self
                        .tool_manager
                        .execute_tool(&call_tool.tool_name, call_tool.parameters.clone())
                        .await
                    {
                        Ok(tool_result) => {
                            println!("{}", "--- ツール実行結果 ---".truecolor(128, 128, 128)); // グレー
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&tool_result).unwrap_or_default().truecolor(128, 128, 128) // グレー
                            );
                            println!("{}", "---------------------".truecolor(128, 128, 128)); // グレー

                            let tool_output_message_content =
                                serde_yaml::to_string(&serde_json::json!({
                                    "tool_result": {
                                        "tool_name": call_tool.tool_name,
                                        "result": tool_result
                                    }
                                }))
                                .unwrap_or_else(|_| {
                                    "Failed to serialize tool result to YAML.".to_string()
                                });

                            self.messages.push(ChatMessage {
                                role: ChatRole::System,
                                content: format!("---\n{}\n---", tool_output_message_content),
                            });

                            println!("{}", "  AIがツール結果を考慮中...".normal()); // 思考（通常の文字）
                            continue;
                        }
                        Err(e) => {
                            eprintln!("{} {:?}", "ツール実行エラー:".red().bold(), e);
                            let error_message_content = serde_yaml::to_string(&serde_json::json!({
                                "tool_error": {
                                    "tool_name": call_tool.tool_name,
                                    "error": format!("{:?}", e)
                                }
                            }))
                            .unwrap_or_else(|_| "Failed to serialize tool error to YAML.".to_string());

                            self.messages.push(ChatMessage {
                                role: ChatRole::System,
                                content: format!("---\n{}\n---", error_message_content),
                            });

                            println!("{}", "  AIがツールエラーを考慮中...".normal()); // 思考（通常の文字）
                            continue;
                        }
                    }
                } else {
                    eprintln!("{} {}", "警告: ツールブロックが検出されましたが、パースできませんでした。AIの出力形式を確認してください。".yellow().bold(), tool_yaml_buffer);
                    return Ok(once(async move { Ok(full_ai_response_content) }).boxed());
                }
            } else {
                return Ok(once(async move { Ok(full_ai_response_content) }).boxed());
            }
        }
    }

    // add_ai_response, revert_last_user_message は変更なし
    pub fn add_ai_response(&mut self, ai_content: String) {
        self.messages.push(ChatMessage {
            role: ChatRole::Assistant,
            content: ai_content,
        });
    }

    pub fn revert_last_user_message(&mut self) {
        if self
            .messages
            .last()
            .map_or(false, |m| m.role == ChatRole::User)
        {
            self.messages.pop();
        }
    }
}