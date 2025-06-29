// src/modules/agent.rs
pub mod api;
pub mod tools;

use anyhow::Result;
use api::{ChatMessage, ChatRole, OllamaApiError};
use futures_util::stream::{Stream, StreamExt, once};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::boxed::Box;
use std::pin::Pin;

use tools::ToolManager;

// AIがツール呼び出しを記述するYAMLの構造体
#[derive(Debug, Deserialize, Serialize)]
struct AiToolCall {
    tool_name: String,
    parameters: Value,
}

// AIの応答からツール呼び出しを抽出・パースするヘルパー関数
fn extract_tool_call_from_response(response_content: &str) -> Option<AiToolCall> {
    // プロンプトに合わせて、YAMLコードブロックをパース
    // ---
    // tool_call:
    //   tool_name: ...
    //   parameters: ...
    // ---
    let start_block_marker = "---";
    let end_block_marker = "---";
    let tool_call_key = "tool_call:";

    let mut found_start = false;
    let mut block_content = String::new();

    for line in response_content.lines() {
        let trimmed_line = line.trim();

        if !found_start {
            // ブロック開始マーカーを探す
            if trimmed_line == start_block_marker {
                found_start = true;
                // `tool_call:` の行はAIが出力する実際のYAMLコンテンツの一部なので含める
                // ただし、ブロック開始マーカー自体は含めない
            }
        } else {
            // ブロック終了マーカーを見つけたら抽出終了
            if trimmed_line == end_block_marker {
                break;
            }
            // ブロック内のコンテンツを収集
            block_content.push_str(line);
            block_content.push('\n');
        }
    }

    if found_start && !block_content.is_empty() {
        // 収集したブロックコンテンツの中に "tool_call:" が含まれているか確認
        if block_content.contains(tool_call_key) {
            // `serde_yaml` を使ってデシリアライズ
            // AiToolCallのネストされた構造を考慮してパース
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
            // `tool_call:` キーワードが見つからない場合はツール呼び出しではない
            None
        }
    } else {
        // 開始マーカーが見つからない、またはブロックが空の場合はツール呼び出しではない
        None
    }
}

pub struct AIAgent {
    api: api::AIApi,
    messages: Vec<ChatMessage>,
    pub tool_manager: ToolManager,
    default_prompt_template: String,
}

impl AIAgent {
    pub fn new(base_url: String, default_model: String) -> Self {
        let api = api::AIApi::new(base_url, default_model);
        let mut tool_manager = ToolManager::new();

        tool_manager.register_tool(tools::shell::ShellTool);

        // default-prompt.md を相対パスで指定。
        // `src/modules/agent/default-prompt.md` は `src/modules/agent.rs` から見ると `default-prompt.md` になる
        // `include_str!` はコンパイル時にファイルを文字列として埋め込むため、実行時のファイルパスは不要
        let default_prompt_template = include_str!("default-prompt.md").to_string();

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

    pub async fn chat_with_tools(
        &mut self,
        user_content: String,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, OllamaApiError>> + Send>>, OllamaApiError>
    {
        // ユーザーメッセージを追加
        self.messages.push(ChatMessage {
            role: ChatRole::User,
            content: user_content,
        });

        // ツール利用のループ
        loop {
            let mut full_messages_for_api = self.messages.clone();

            // システムプロンプトを先頭に追加（ツール定義を埋め込む）
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

            // AIからの応答を取得
            let mut ai_response_stream = self
                .api
                .get_chat_completion_stream(full_messages_for_api)
                .await?;
            let mut full_ai_response_content = String::new();

            // ストリームから応答を収集
            while let Some(chunk_result) = ai_response_stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        full_ai_response_content.push_str(&chunk);
                    }
                    Err(e) => {
                        return Ok(once(async { Err(e) }).boxed());
                    }
                }
            }

            // AIの応答を履歴に追加
            self.messages.push(ChatMessage {
                role: ChatRole::Assistant,
                content: full_ai_response_content.clone(),
            });

            // ツール呼び出しを解析
            if let Some(call_tool) = extract_tool_call_from_response(&full_ai_response_content) {
                println!(
                    "\n--- ツール呼び出しを検出: {} (引数: {:?}) ---",
                    call_tool.tool_name, call_tool.parameters
                );

                // ツールを実行
                match self
                    .tool_manager
                    .execute_tool(&call_tool.tool_name, call_tool.parameters.clone())
                    .await
                {
                    Ok(tool_result) => {
                        println!("--- ツール実行結果 ---");
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&tool_result).unwrap_or_default()
                        );
                        println!("---------------------");

                        // ツール実行結果をAIへのメッセージ履歴に追加（YAML形式）
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
                            role: ChatRole::System, // ツール結果はシステムメッセージとしてAIにフィードバック
                            content: format!("---\n{}\n---", tool_output_message_content), // プロンプトに合わせて`---`で囲む
                        });

                        // ループを続行し、AIに次の応答をさせる
                        continue;
                    }
                    Err(e) => {
                        eprintln!("ツール実行エラー: {:?}", e);
                        // エラーをAIにフィードバック
                        let error_message_content = serde_yaml::to_string(&serde_json::json!({
                            "tool_error": {
                                "tool_name": call_tool.tool_name,
                                "error": format!("{:?}", e)
                            }
                        }))
                        .unwrap_or_else(|_| "Failed to serialize tool error to YAML.".to_string());

                        self.messages.push(ChatMessage {
                            role: ChatRole::System,
                            content: format!("---\n{}\n---", error_message_content), // プロンプトに合わせて`---`で囲む
                        });

                        // エラー発生時もループを続行し、AIに次の応答をさせる
                        continue;
                    }
                }
            } else {
                // ツール呼び出しでなかった場合、最終的なAIの応答としてストリームを返す
                return Ok(once(async move { Ok(full_ai_response_content) }).boxed());
            }
        }
    }

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
            .is_some_and(|m| m.role == ChatRole::User)
        {
            self.messages.pop();
        }
    }
}