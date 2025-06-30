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
            let mut current_messages_for_api = initial_messages;

                // Acquire lock for immutable access (api, tool_manager, default_prompt_template)
                let (api_clone, tool_manager_schemas, default_prompt_template_clone) = {
                    let agent_locked_immutable = self_arc_mutex.lock().await;

                    // Clone necessary data out of the lock
                    let api_clone = agent_locked_immutable.api.clone();
                    let tool_manager_schemas = agent_locked_immutable.tool_manager.get_tool_json_schemas();
                    let default_prompt_template_clone = agent_locked_immutable.default_prompt_template.clone();
                    (api_clone, tool_manager_schemas, default_prompt_template_clone)
                }; // `agent_locked_immutable` is dropped here, releasing the lock

                // System prompt (tool definitions) logic remains
                let formatted_prompt = default_prompt_template_clone
                    .replace("{{TOOLS_JSON_SCHEMA}}", &tool_manager_schemas.to_string());

                // Ensure system message is at the beginning
                if let Some(msg) = current_messages_for_api.get_mut(0) {
                    if msg.role == ChatRole::System {
                        msg.content = formatted_prompt;
                    } else {
                        current_messages_for_api.insert(
                            0,
                            ChatMessage {
                                role: ChatRole::System,
                                content: formatted_prompt,
                            },
                        );
                    }
                } else {
                    current_messages_for_api.insert(
                        0,
                        ChatMessage {
                            role: ChatRole::System,
                            content: formatted_prompt,
                        },
                    );
                }

                let mut ai_response_stream = api_clone
                    .get_chat_completion_stream(current_messages_for_api.clone())
                    .await?;

                let mut full_ai_response_content = String::new();
                let mut tool_block_detected = false;
                let mut tool_yaml_buffer = String::new();
                let mut pending_display_content = String::new();

                while let Some(chunk_result) = ai_response_stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            full_ai_response_content.push_str(&chunk);

                            // ★変更点: ここで `extract_tool_call_from_response` を使用してツールブロックを検出
                            // `tool_block_detected` が一度trueになったら、ツールブロックの終わりを探すモードに移行
                            if !tool_block_detected {
                                pending_display_content.push_str(&chunk);
                                // partial_response_content は現在のチャンクを含むこれまでの全内容
                                if let Some(_tool_call) = extract_tool_call_from_response(&full_ai_response_content) {
                                    // ツールブロックが検出されたが、まだストリームの途中の可能性があるので、
                                    // そのツールブロック部分だけをバッファに格納し、残りをpending_display_contentからクリア
                                    tool_block_detected = true;
                                    tool_yaml_buffer = full_ai_response_content.clone(); // フル応答からツールブロック全体をキャプチャ

                                    // ツールブロックの手前までのコンテンツを抽出して表示
                                    let tool_block_start_index = full_ai_response_content.find("---tool_call:").or_else(|| full_ai_response_content.find("```tool_call:")).unwrap_or_else(|| full_ai_response_content.find("```").unwrap_or(0));
                                    if tool_block_start_index > 0 {
                                        yield Ok(AgentEvent::AiResponseChunk(
                                            full_ai_response_content[..tool_block_start_index].to_string()
                                        ));
                                    }
                                    yield Ok(AgentEvent::AttemptingToolDetection);
                                    pending_display_content.clear(); // ここでクリア
                                    break; // ツールブロックが検出されたら、このAI応答ストリームを中断し、次のループでツール実行へ
                                } else {
                                    // ツールブロックがまだ検出されていない場合は、通常のAI応答チャンクとして表示
                                    yield Ok(AgentEvent::AiResponseChunk(chunk.to_string()));
                                }
                            } else {
                                // ツールブロックがすでに検出されている場合、バッファに追記し続ける
                                tool_yaml_buffer.push_str(&chunk);
                                // ここでは、ツールブロックの終了を検出するのではなく、`extract_tool_call_from_response` が
                                // 完全な `tool_yaml_buffer` を処理できるように待機します。
                                // `extract_tool_call_from_response` が None を返した場合でも、
                                // ストリームが終了するか、次のチャンクで完了することを期待します。
                            }
                        }
                        Err(e) => {
                            yield Err(e);
                            return;
                        }
                    }
                }

                // Acquire lock to add message to history
                {
                    let mut agent_locked_mut = self_arc_mutex.lock().await;
                    agent_locked_mut.add_message_to_history(ChatMessage {
                        role: ChatRole::Assistant,
                        content: full_ai_response_content.clone(),
                    });
                } // `agent_locked_mut` is dropped here, releasing the lock

                if tool_block_detected {
                    // ストリームが終了したか、ツールブロックが検出されたので、バッファからツール呼び出しを抽出
                    let tool_call_result = extract_tool_call_from_response(&tool_yaml_buffer);
                    if let Some(call_tool) = tool_call_result {
                        yield Ok(AgentEvent::ToolCallDetected(call_tool.clone()));
                        yield Ok(AgentEvent::ToolExecuting(call_tool.tool_name.clone()));

                        let self_arc_mutex_clone_for_tool_exec = self_arc_mutex.clone();
                        let tool_name_for_future = call_tool.tool_name.clone();
                        let parameters_for_future = call_tool.parameters.clone();

                        let actual_tool_result_outcome = async move {
                            let agent_locked_for_tool_exec = self_arc_mutex_clone_for_tool_exec.lock().await;

                            agent_locked_for_tool_exec.tool_manager.execute_tool(
                                &tool_name_for_future,
                                parameters_for_future
                            ).await
                        }.await;

                        match actual_tool_result_outcome {
                            Ok(tool_result) => {
                                yield Ok(AgentEvent::ToolResult(call_tool.tool_name.clone(), tool_result.clone()));

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

                                // Acquire lock to add message to history
                                {
                                    let mut agent_locked_mut = self_arc_mutex.lock().await;
                                    agent_locked_mut.add_message_to_history(ChatMessage {
                                        role: ChatRole::System, // ツール結果はSystemロール
                                        content: format!("---\n{}\n---", tool_output_message_content),
                                    });
                                } // Lock dropped

                                yield Ok(AgentEvent::Thinking("AIがツール結果を考慮中...".to_string()));

                                // Break here, let ChatSession re-evaluate and call again with updated history
                            }
                            Err(e) => {
                                yield Ok(AgentEvent::ToolError(call_tool.tool_name.clone(), format!("{:?}", e)));

                                let error_message_content = serde_yaml::to_string(&serde_json::json!({
                                    "tool_error": {
                                        "tool_name": call_tool.tool_name,
                                        "error": format!("{:?}", e)
                                    }
                                }))
                                .unwrap_or_else(|_| {
                                    "Failed to serialize tool error to YAML.".to_string()
                                });

                                // Acquire lock to add message to history
                                {
                                    let mut agent_locked_mut = self_arc_mutex.lock().await;
                                    agent_locked_mut.add_message_to_history(ChatMessage {
                                        role: ChatRole::System, // エラーはSystemとしてAIにフィードバック
                                        content: format!("---\n{}\n---", error_message_content),
                                    });
                                } // Lock dropped

                                yield Ok(AgentEvent::Thinking("AIがツールエラーを考慮中...".to_string()));

                            }
                        }
                    } else {
                        // extract_tool_call_from_response が None を返した場合（パースできなかった場合）
                        yield Ok(AgentEvent::YamlParseError(
                            "YAMLツール呼び出しのパースに失敗しました".to_string(),
                            tool_yaml_buffer.clone()
                        ));
                        yield Ok(AgentEvent::ToolBlockParseWarning(tool_yaml_buffer.clone()));
                        // ここで `full_ai_response_content` を表示し直す必要はありません。
                        // `full_ai_response_content` は既に履歴に保存されています。
                        // yield Ok(AgentEvent::AiResponseChunk(full_ai_response_content));
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