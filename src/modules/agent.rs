// src/modules/agent.rs
pub mod api;
pub mod tools;

use anyhow::Result;
use api::{AIApi, OllamaApiError};
use crate::modules::agent::api::{ChatMessage, ChatRole};
use futures_util::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::boxed::Box;
use std::pin::Pin;
// Removed: use std::io; // Unused import from previous error

use tokio::sync::Mutex;
use std::sync::Arc;

use tools::ToolManager;

// AIがツール呼び出しを記述するYAMLの構造体
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AiToolCall {
    pub tool_name: String,
    pub parameters: Value,
}

/// エージェントからチャットセッションに送られるイベントの種類
#[derive(Debug)]
pub enum AgentEvent {
    /// AIの応答のチャンク（通常のテキスト）
    AiResponseChunk(String),
    /// AIが自身のメッセージを履歴に追加したいことを示す
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
    UserMessageAdded,
    /// ツールブロックの検出を試みているメッセージ
    AttemptingToolDetection,
    /// 通常のAIのテキストとして表示する保留中のコンテンツ
    PendingDisplayContent(String),
    /// ツールブロックをパースできなかった場合の警告
    ToolBlockParseWarning(String),
    /// YAMLツール呼び出しのパースに失敗したエラー
    YamlParseError(String, String), // error message, yaml content
}

// AIの応答からツール呼び出しを抽出・パースするヘルパー関数
fn extract_tool_call_from_response(response_content: &str) -> Option<AiToolCall> {
    let block_marker = "---";
    let tool_call_key = "tool_call:";

    let mut found_start = false;
    let mut block_content = String::new();

    for line in response_content.lines() {
        let trimmed_line = line.trim();

        if trimmed_line == block_marker {
            if found_start {
                break;
            } else {
                found_start = true;
                continue;
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

        match serde_yaml::from_str::<OuterToolCall>(&block_content) {
            Ok(outer_call) => Some(outer_call.tool_call),
            Err(_e) => {
                None
            }
        }
    } else {
        None
    }
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
    ) -> Result<Pin<Box<dyn Stream<Item = Result<AgentEvent, OllamaApiError>> + Send>>, OllamaApiError>
    {
        let agent_stream = async_stream::stream! {
            let mut current_messages_for_api = initial_messages;

            loop {
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

                            if !tool_block_detected {
                                pending_display_content.push_str(&chunk);
                                if pending_display_content.contains("---")
                                    && pending_display_content.contains("tool_call:")
                                {
                                    tool_block_detected = true;

                                    let parts: Vec<&str> =
                                        pending_display_content.splitn(2, "---").collect();
                                    if parts.len() > 1 {
                                        yield Ok(AgentEvent::PendingDisplayContent(parts[0].to_string()));
                                        tool_yaml_buffer.push_str("---");
                                        tool_yaml_buffer.push_str(parts[1]);
                                    } else {
                                        yield Ok(AgentEvent::PendingDisplayContent(pending_display_content.clone()));
                                        tool_yaml_buffer.push_str(&pending_display_content);
                                    }
                                    yield Ok(AgentEvent::AttemptingToolDetection);
                                    pending_display_content.clear();
                                } else {
                                    yield Ok(AgentEvent::AiResponseChunk(chunk.to_string()));
                                }
                            } else {
                                tool_yaml_buffer.push_str(&chunk);
                                if chunk.contains("---") && tool_yaml_buffer.contains("tool_call:") {
                                    break;
                                }
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
                                        role: ChatRole::System,
                                        content: format!("---\n{}\n---", tool_output_message_content),
                                    });
                                } // Lock dropped

                                yield Ok(AgentEvent::Thinking("AIがツール結果を考慮中...".to_string()));

                                // Break here, let ChatSession re-evaluate and call again with updated history
                                break;
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
                                        role: ChatRole::System,
                                        content: format!("---\n{}\n---", error_message_content),
                                    });
                                } // Lock dropped

                                yield Ok(AgentEvent::Thinking("AIがツールエラーを考慮中...".to_string()));

                                break;
                            }
                        }
                    } else {
                        yield Ok(AgentEvent::YamlParseError(
                            "YAMLツール呼び出しのパースに失敗しました".to_string(),
                            tool_yaml_buffer.clone()
                        ));
                        yield Ok(AgentEvent::ToolBlockParseWarning(tool_yaml_buffer.clone()));
                        yield Ok(AgentEvent::AiResponseChunk(full_ai_response_content));
                        break;
                    }
                } else {
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
        if self
            .messages
            .last()
            .map_or(false, |m| m.role == ChatRole::User)
        {
            self.messages.pop();
        }
    }
}