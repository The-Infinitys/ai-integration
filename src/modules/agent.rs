// src/modules/agent.rs
pub mod api;
pub mod tools;

use anyhow::Result;
use api::{ChatMessage, ChatRole, OllamaApiError, AIApi};
use futures_util::stream::{Stream, StreamExt, once};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::boxed::Box;
use std::io::{self, Write};
use std::pin::Pin;
use colored::*;

use tools::ToolManager;

// AIがツール呼び出しを記述するYAMLの構造体
#[derive(Debug, Deserialize, Serialize)]
struct AiToolCall {
    tool_name: String,
    parameters: Value,
}

// AIの応答からツール呼び出しを抽出・パースするヘルパー関数
fn extract_tool_call_from_response(response_content: &str) -> Option<AiToolCall> {
    let block_marker = "---";
    let tool_call_key = "tool_call:";

    let mut found_start = false;
    let mut block_content = String::new();

    // 行ごとに処理し、`tool_call:` キーを持つYAMLブロックを探す
    for line in response_content.lines() {
        let trimmed_line = line.trim();

        if trimmed_line == block_marker {
            if found_start {
                // 既に開始マーカーが見つかっている状態で終了マーカーを見つけた
                break;
            } else {
                found_start = true;
                continue; // 開始マーカー自体はブロック内容に含めない
            }
        }

        if found_start {
            block_content.push_str(line);
            block_content.push('\n');
        }
    }

    if found_start && !block_content.is_empty() && block_content.contains(tool_call_key) {
        #[derive(Debug, Deserialize)]
        struct OuterToolCall {
            tool_call: AiToolCall,
        }

        // YAMLパースはブロックの内容全体で行う
        match serde_yaml::from_str::<OuterToolCall>(&block_content) {
            Ok(outer_call) => Some(outer_call.tool_call),
            Err(e) => {
                eprintln!("{} YAMLツール呼び出しのパースに失敗しました: {}", "エラー:".red().bold(), e);
                eprintln!("{} YAML内容:\n{}", "内容:".yellow(), block_content);
                None
            }
        }
    } else {
        None
    }
}


pub struct AIAgent {
    api: AIApi,
    messages: Vec<ChatMessage>,
    pub tool_manager: ToolManager,
    default_prompt_template: String,
}

impl AIAgent {
    pub fn new(base_url: String, default_model: String) -> Self {
        let api = AIApi::new(base_url, default_model);
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

    pub async fn list_available_models(&self) -> Result<serde_json::Value, OllamaApiError> {
        self.api.list_models().await
    }

    pub async fn chat_with_tools_realtime(
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

            let mut ai_response_stream = self
                .api
                .get_chat_completion_stream(full_messages_for_api)
                .await?;
            
            let mut full_ai_response_content = String::new();
            let mut tool_block_detected = false;
            let mut tool_yaml_buffer = String::new();
            let mut pending_display_content = String::new(); // AIがツール呼び出しで中断する前のテキスト部分

            // ストリームから応答をリアルタイムで収集し、表示し、ツール呼び出しを検出
            while let Some(chunk_result) = ai_response_stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        full_ai_response_content.push_str(&chunk);

                        // ツールブロック検出ロジックをより堅牢に
                        if !tool_block_detected {
                            pending_display_content.push_str(&chunk);
                            // ツールブロックの開始を検知
                            if pending_display_content.contains("---") && pending_display_content.contains("tool_call:") {
                                tool_block_detected = true;
                                
                                // ツールブロック開始までのテキストを表示
                                let parts: Vec<&str> = pending_display_content.splitn(2, "---").collect();
                                if parts.len() > 1 {
                                    print!("{}", parts[0].bold());
                                    io::stdout().flush().expect("stdout flush failed");
                                    tool_yaml_buffer.push_str("---"); // 最初のマーカーをバッファに追加
                                    tool_yaml_buffer.push_str(parts[1]); // 残りの部分をバッファに追加
                                } else {
                                    // "---" が見つからない場合は、チャンク全体を表示し、バッファリング開始
                                    print!("{}", pending_display_content.bold());
                                    io::stdout().flush().expect("stdout flush failed");
                                    tool_yaml_buffer.push_str(&pending_display_content);
                                }
                                println!("\n{}", "  --- ツール呼び出しを検出しようとしています... ---".truecolor(128, 128, 128));
                                io::stdout().flush().expect("stdout flush failed");
                                pending_display_content.clear(); // クリアしてツールYAMLバッファリングに専念
                            } else {
                                // まだツールブロックではない場合、通常のAIのテキストとしてリアルタイム表示
                                print!("{}", chunk.bold());
                                io::stdout().flush().expect("stdout flush failed");
                                // pending_display_content は次のチャンクでツール呼び出しの可能性をチェックするために保持
                            }
                        } else {
                            // ツールブロックの途中
                            tool_yaml_buffer.push_str(&chunk);
                            // ツールブロックの終了マーカーを検知
                            if chunk.contains("---") && tool_yaml_buffer.contains("tool_call:") {
                                // 最後の "---" が見つかったので、ストリームの読み込みを停止
                                break; 
                            }
                        }
                    }
                    Err(e) => {
                        return Ok(once(async { Err(e) }).boxed());
                    }
                }
            }
            
            // AIの応答を履歴に追加 (ツール呼び出しの有無に関わらず、AIが出力した内容は履歴に残す)
            self.messages.push(ChatMessage {
                role: ChatRole::Assistant,
                content: full_ai_response_content.clone(),
            });

            // ツール呼び出しが検出された場合のみ、追加の処理を行う
            if tool_block_detected {
                // ツール呼び出しを解析
                if let Some(call_tool) = extract_tool_call_from_response(&tool_yaml_buffer) {
                    // ここを修正します
                    let formatted_tool_call = if call_tool.tool_name == "shell" {
                        if let Some(command_line) = call_tool.parameters["command_line"].as_str() {
                            format!("{} ({})", call_tool.tool_name, command_line)
                        } else {
                            format!("{} (無効な引数)", call_tool.tool_name)
                        }
                    } else {
                        // 他のツールについては、デフォルトでツール名と整形されたJSON引数を表示
                        format!("{} (引数: {})",
                            call_tool.tool_name,
                            serde_json::to_string_pretty(&call_tool.parameters).unwrap_or_default()
                        )
                    };
                    println!("\n{}", format!("--- ツール呼び出しを検出: {} ---", formatted_tool_call).truecolor(128, 128, 128));
                    // 修正ここまで
                    
                    io::stdout().flush().expect("stdout flush failed");
                    
                    println!("{}", "  ツールを実行中...".truecolor(128, 128, 128));
                    io::stdout().flush().expect("stdout flush failed");

                    // ツールを実行
                    match self
                        .tool_manager
                        .execute_tool(&call_tool.tool_name, call_tool.parameters.clone())
                        .await
                    {
                        Ok(tool_result) => {
                            println!("{}", "--- ツール実行結果 ---".truecolor(128, 128, 128));
                            
                            if call_tool.tool_name == "shell" {
                                let stdout = tool_result["stdout"].as_str().unwrap_or("");
                                let stderr = tool_result["stderr"].as_str().unwrap_or("");
                                let success = tool_result["success"].as_bool().unwrap_or(false);

                                if !stdout.is_empty() {
                                    println!("{} {}", "stdout:".green().bold(), stdout.truecolor(128, 128, 128));
                                }
                                if !stderr.is_empty() {
                                    println!("{} {}", "stderr:".red().bold(), stderr.truecolor(128, 128, 128));
                                }
                                if !stdout.is_empty() || !stderr.is_empty() {
                                    println!("{}", "---------------------".truecolor(128, 128, 128));
                                }
                                if !success {
                                    println!("{}", "コマンドがエラーコードを返しました。".red().bold());
                                }
                            } else {
                                // 他のツールの場合、デフォルトのJSON pretty print
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&tool_result).unwrap_or_default().truecolor(128, 128, 128)
                                );
                            }

                            println!("{}", "---------------------".truecolor(128, 128, 128)); // 共通の終了マーカー
                            io::stdout().flush().expect("stdout flush failed");

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
                                content: format!("---\n{}\n---", tool_output_message_content),
                            });

                            println!("{}", "  AIがツール結果を考慮中...".normal());
                            io::stdout().flush().expect("stdout flush failed");

                            // ループを続行し、AIに次の応答をさせる
                            continue;
                        }
                        Err(e) => {
                            eprintln!("{} {:?}", "ツール実行エラー:".red().bold(), e);
                            io::stdout().flush().expect("stdout flush failed");

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
                                content: format!("---\n{}\n---", error_message_content),
                            });

                            println!("{}", "  AIがツールエラーを考慮中...".normal());
                            io::stdout().flush().expect("stdout flush failed");

                            // エラー発生時もループを続行し、AIに次の応答をさせる
                            continue;
                        }
                    }
                } else {
                    // ツールブロックらしきものはあったがパースできなかった場合
                    eprintln!("{} {}", "警告: ツールブロックが検出されましたが、パースできませんでした。AIの出力形式を確認してください。".yellow().bold(), tool_yaml_buffer);
                    io::stdout().flush().expect("stdout flush failed");

                    // パースできなかったツールブロックの内容は通常のAI応答として返す
                    return Ok(once(async move { Ok(full_ai_response_content) }).boxed());
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
            .map_or(false, |m| m.role == ChatRole::User)
        {
            self.messages.pop();
        }
    }
}