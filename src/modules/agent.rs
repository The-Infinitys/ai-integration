// src/modules/agent.rs
pub mod api;
pub mod tools;

use crate::modules::agent::api::{ChatMessage, ChatRole};
use anyhow::Result;
use api::{AIApi, OllamaApiError};
use futures_util::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::boxed::Box;
use std::pin::Pin;

use std::sync::Arc;
use tokio::sync::Mutex;

use tools::ToolManager;

// AIがツール呼び出しを記述するYAMLの構造体
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AiToolCall {
    pub tool_name: String,
    pub parameters: Value,
}

/// エージェントからチャットセッションに送られるイベントの種類
// #[derive(Debug)]
#[allow(dead_code)]
pub enum AgentEvent {
    /// AIの応答のチャンク（通常のテキスト）
    AiResponseChunk(String),
    /// AIが自身のメッセージを履歴に追加したいことを示す
    #[allow(dead_code)]
    AddMessageToHistory(ChatMessage),
    /// ツール呼び出しが検出された
    ToolCallDetected(AiToolCall),
    /// ツールが実行されている
    ToolExecuting(String),
    /// ツールの実行が成功した
    ToolResult(String, Value), // tool_name, result
    /// ツールの実行が失敗した
    ToolError(String, String), // tool_name, error_message
    /// AIが思考中であることを示すメッセージ
    Thinking(String),
    /// ユーザーメッセージが追加されたことを示す (UIでは特に表示しない)
    #[allow(dead_code)]
    UserMessageAdded,
    /// ツールブロックの検出を試みているメッセージ
    AttemptingToolDetection,
    /// 通常のAIのテキストとして表示する保留中のコンテンツ
    #[allow(dead_code)]
    PendingDisplayContent(String),
    /// ツールブロックをパースできなかった場合の警告
    ToolBlockParseWarning(String),
    /// YAMLツール呼び出しのパースに失敗したエラー
    YamlParseError(String, String), // error message, yaml content
}

// AIの応答からツール呼び出しを抽出・パースするヘルパー関数
// ---
// `extract_tool_call_from_response` 関数をより堅牢に修正。
// `---` または ` ``` ` の開始/終了マーカー、および ` ```tool_call:` や ` ```yaml` のような言語指定に対応。
fn extract_tool_call_from_response(response_content: &str) -> Option<AiToolCall> {
    let lines: Vec<&str> = response_content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed_line = lines[i].trim();

        let (is_start_marker, _marker_len) = if trimmed_line.starts_with("---") {
            (true, "---".len())
        } else if trimmed_line.starts_with("```") {
            (true, "```".len())
        } else {
            (false, 0)
        };

        if is_start_marker {
            let mut block_content = String::new();
            let mut current_line_idx = i + 1; // マーカーの次の行から開始

            // マーカーの直後に言語指定や `tool_call:` が続く場合の処理
            let content_after_marker = trimmed_line.strip_prefix("---").or_else(|| trimmed_line.strip_prefix("```"));
            if let Some(remainder) = content_after_marker {
                let stripped_remainder = remainder.trim();
                // 残りの部分が空でなく、かつ言語指定やtool_call:と判断できる場合、この行の残りをコンテンツに含める
                if !stripped_remainder.is_empty() && (stripped_remainder.starts_with("tool_call:") || stripped_remainder == "yaml" || stripped_remainder == "json") {
                     block_content.push_str(stripped_remainder);
                     block_content.push('\n');
                }
            }
            
            // ブロックの本体を抽出
            while current_line_idx < lines.len() {
                let inner_trimmed_line = lines[current_line_idx].trim();

                // 終了マーカーの検出
                if inner_trimmed_line == "---" || inner_trimmed_line == "```" {
                    // 終了マーカーの前に、`tool_call:` キーが含まれているか確認
                    if block_content.contains("tool_call:") {
                        #[derive(Debug, Deserialize)]
                        struct OuterToolCall {
                            tool_call: AiToolCall,
                        }
                        match serde_yaml::from_str::<OuterToolCall>(&block_content) {
                            Ok(outer_call) => return Some(outer_call.tool_call),
                            Err(_) => {
                                // パースエラーが発生しても、他のブロックを試すためにNoneを返す
                                return None; // ストリーム処理のコンテキストでは、このNoneでエラーイベントを発行するのが適切
                            }
                        }
                    } else {
                        // `tool_call:` が見つからない場合は、このブロックはツール呼び出しではない
                        break;
                    }
                }

                block_content.push_str(lines[current_line_idx]);
                block_content.push('\n');
                current_line_idx += 1;
            }
        }
        i += 1;
    }
    None
}


pub struct AIAgent {
    api: AIApi, // Keep this private
    pub messages: Vec<ChatMessage>,
    pub tool_manager: ToolManager,
    default_prompt_template: String,
}

impl AIAgent {
    pub fn new(base_url: String, default_model: String) -> Self {
        let api = AIApi::new(base_url, default_model);
        let mut tool_manager = ToolManager::new();

        tool_manager.register_tool(tools::shell::ShellTool);
        tool_manager.register_tool(tools::www::search::SearchEngineTool);
        tool_manager.register_tool(tools::www::browse::WebPageBrowser);
        tool_manager.register_tool(tools::files::info::InfoTool);
        tool_manager.register_tool(tools::files::read::ReadTool);
        tool_manager.register_tool(tools::files::write::WriteTool);
        let default_prompt_template = include_str!("default-prompt.md").to_string();

        AIAgent {
            api,
            messages: vec![],
            tool_manager,
            default_prompt_template,
        }
    }

    // New public method to set the model via the agent's internal API
    pub fn set_model(&mut self, model_name: String) {
        self.api.set_model(model_name); // Now this calls AIApi's set_model
    }

    pub async fn list_available_models(&self) -> Result<serde_json::Value, OllamaApiError> {
        self.api.list_models().await
    }

    pub async fn chat_with_tools_realtime(
        self_arc_mutex: Arc<Mutex<Self>>,
        initial_messages: Vec<ChatMessage>,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<AgentEvent, OllamaApiError>> + Send>>,
        OllamaApiError,
    > {
        let agent_stream = async_stream::stream! {
            let mut loop_messages = initial_messages;

            loop {
                // --- 1. Get latest state and prepare for API call ---
                let (api_clone, tool_manager_schemas, default_prompt_template_clone) = {
                    let agent_locked = self_arc_mutex.lock().await;
                    (
                        agent_locked.api.clone(),
                        agent_locked.tool_manager.get_tool_json_schemas(),
                        agent_locked.default_prompt_template.clone(),
                    )
                };

                // --- 2. Inject system prompt ---
                let formatted_prompt = default_prompt_template_clone
                    .replace("{{TOOLS_JSON_SCHEMA}}", &tool_manager_schemas.to_string());
                
                loop_messages.retain(|msg| msg.role != ChatRole::System);
                
                // Insert system prompt before the last user message for better context
                let insert_index = if let Some(pos) = loop_messages.iter().rposition(|m| m.role == ChatRole::User) {
                    pos
                } else {
                    loop_messages.len()
                };
                loop_messages.insert(
                    insert_index,
                    ChatMessage {
                        role: ChatRole::System,
                        content: formatted_prompt,
                    },
                );

                // --- 3. Get AI response stream ---
                let mut ai_response_stream = api_clone
                    .get_chat_completion_stream(loop_messages.clone())
                    .await?;

                // --- 4. Process the entire stream from AI ---
                let mut full_ai_response_content = String::new();
                while let Some(chunk_result) = ai_response_stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            full_ai_response_content.push_str(&chunk);
                            // Yield each chunk immediately for the "typing" effect in the UI
                            yield Ok(AgentEvent::AiResponseChunk(chunk));
                        }
                        Err(e) => {
                            yield Err(e);
                            return; // End the whole stream on a critical error
                        }
                    }
                }

                // --- 5. Add AI's full response to history ---
                // This is done only once after the entire response is received.
                {
                    let mut agent_locked = self_arc_mutex.lock().await;
                    agent_locked.add_message_to_history(ChatMessage {
                        role: ChatRole::Assistant,
                        content: full_ai_response_content.clone(),
                    });
                }

                // --- 6. Parse the full response for a tool call and decide the next step ---
                if let Some(call_tool) = extract_tool_call_from_response(&full_ai_response_content) {
                    // A tool was found in the response.
                    yield Ok(AgentEvent::ToolCallDetected(call_tool.clone()));
                    yield Ok(AgentEvent::ToolExecuting(call_tool.tool_name.clone()));

                    // Execute the tool
                    let tool_result_outcome = {
                        let agent_locked = self_arc_mutex.lock().await;
                        agent_locked.tool_manager.execute_tool(
                            &call_tool.tool_name,
                            call_tool.parameters.clone()
                        ).await
                    };

                    // Process the tool's result and add it to history
                    match tool_result_outcome {
                        Ok(tool_result) => {
                            yield Ok(AgentEvent::ToolResult(call_tool.tool_name.clone(), tool_result.clone()));
                            let tool_output_message_content = serde_yaml::to_string(&serde_json::json!({
                                "tool_result": { "tool_name": call_tool.tool_name, "result": tool_result }
                            })).unwrap_or_else(|_| "Failed to serialize tool result.".to_string());

                            let mut agent_locked = self_arc_mutex.lock().await;
                            agent_locked.add_message_to_history(ChatMessage {
                                role: ChatRole::System,
                                content: format!("---\n{}\n---", tool_output_message_content),
                            });
                        }
                        Err(e) => {
                            let error_message = format!("{:?}", e);
                            yield Ok(AgentEvent::ToolError(call_tool.tool_name.clone(), error_message.clone()));
                            let error_message_content = serde_yaml::to_string(&serde_json::json!({
                                "tool_error": { "tool_name": call_tool.tool_name, "error": error_message }
                            })).unwrap_or_else(|_| "Failed to serialize tool error.".to_string());
                            
                            let mut agent_locked = self_arc_mutex.lock().await;
                            agent_locked.add_message_to_history(ChatMessage {
                                role: ChatRole::System,
                                content: format!("---\n{}\n---", error_message_content),
                            });
                        }
                    }
                    
                    // Update the message history for the next iteration of the loop
                    {
                        let agent_locked = self_arc_mutex.lock().await;
                        loop_messages = agent_locked.messages.clone();
                    }

                    yield Ok(AgentEvent::Thinking("AI is considering the tool's result...".to_string()));
                    // Continue the loop to let the AI process the tool result and think again.
                } else {
                    // No tool call was detected in the AI's response.
                    // The conversation turn is complete. Break the loop.
                    break;
                }
            }
        };

        Ok(Box::pin(agent_stream))
    }

    pub fn add_message_to_history(&mut self, message: ChatMessage) {
        self.messages.push(message);
    }

    pub fn revert_last_user_message(&mut self) {
        let mut last_user_idx = None;
        for (i, msg) in self.messages.iter().enumerate().rev() {
            if msg.role == ChatRole::User {
                last_user_idx = Some(i);
                break;
            }
        }

        if let Some(idx) = last_user_idx {
            // ユーザーメッセージとその後に続くAIの応答、ツール結果などを全て削除
            self.messages.truncate(idx);
        }
        // ユーザーメッセージが見つからない場合は何もしない
    }
}