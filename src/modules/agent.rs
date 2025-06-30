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

// ログファイル保存のために追加
use std::fs::OpenOptions;
use std::io::Write;
use chrono::Local;
use std::path::PathBuf;
use dirs::home_dir; // `dirs` クレートが必要です。Cargo.tomlに`dirs = "5.0"`を追加してください。


/// AIがツール呼び出しを記述するYAMLの構造体
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AiToolCall {
    pub tool_name: String,
    pub parameters: Value,
}

/// エージェントからチャットセッションに送られるイベントの種類
// #[derive(Debug)] // デバッグ出力が冗長になるためコメントアウト
#[allow(dead_code)] // 使用されていないバリアントがあっても警告を出さない
pub enum AgentEvent {
    /// AIの応答のチャンク（通常のテキスト）
    AiResponseChunk(String),
    /// AIが自身のメッセージを履歴に追加したいことを示す (現在未使用)
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
    /// ツールブロックの検出を試みているメッセージ (現在未使用)
    AttemptingToolDetection,
    /// 通常のAIのテキストとして表示する保留中のコンテンツ (現在未使用)
    #[allow(dead_code)]
    PendingDisplayContent(String),
    /// ツールブロックをパースできなかった場合の警告 (現在未使用)
    ToolBlockParseWarning(String),
    /// YAMLツール呼び出しのパースに失敗したエラー (現在未使用、Noneを返しているため)
    YamlParseError(String, String), // error message, yaml content
}

/// AIの応答からツール呼び出しを抽出・パースするヘルパー関数
/// `---` または ` ``` の開始/終了マーカー、および ` ```tool_call:` や ` ```yaml` のような言語指定に対応。
fn extract_tool_call_from_response(response_content: &str) -> Option<AiToolCall> {
    let lines: Vec<&str> = response_content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed_line = lines[i].trim();

        // 開始マーカーの検出: "---" または "```"
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
            let content_after_marker = trimmed_line
                .strip_prefix("---")
                .or_else(|| trimmed_line.strip_prefix("```"));
            if let Some(remainder) = content_after_marker {
                let stripped_remainder = remainder.trim();
                // 残りの部分が空でなく、かつ言語指定やtool_call:と判断できる場合、この行の残りをコンテンツに含める
                if !stripped_remainder.is_empty()
                    && (stripped_remainder.starts_with("tool_call:")
                        || stripped_remainder == "yaml"
                        || stripped_remainder == "json")
                {
                    block_content.push_str(stripped_remainder);
                    block_content.push('\n');
                }
            }

            // ブロックの本体を抽出
            while current_line_idx < lines.len() {
                let inner_trimmed_line = lines[current_line_idx].trim();

                // 終了マーカーの検出
                if inner_trimmed_line == "---" || inner_trimmed_line == "```" {
                    // `tool_call:` キーが含まれているか確認
                    if block_content.contains("tool_call:") {
                        #[derive(Debug, Deserialize)]
                        struct OuterToolCall {
                            tool_call: AiToolCall,
                        }
                        match serde_yaml::from_str::<OuterToolCall>(&block_content) {
                            Ok(outer_call) => return Some(outer_call.tool_call),
                            Err(_) => {
                                // パースエラーが発生した場合、このブロックはツール呼び出しとして認識しない
                                // そのため、Noneを返して他のブロックを試すか、通常のテキストとして扱う
                                return None;
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

/// AIエージェントのメイン構造体
pub struct AIAgent {
    api: AIApi, // Ollama APIクライアント (プライベート)
    pub messages: Vec<ChatMessage>, // チャット履歴
    pub tool_manager: ToolManager, // ツール管理
    default_prompt_template: String, // デフォルトのシステムプロンプトテンプレート
    log_file_path: Option<PathBuf>, // ログファイルのパス
}

impl AIAgent {
    /// 新しいAIAgentインスタンスを作成
    pub fn new(base_url: String, default_model: String) -> Self {
        let api = AIApi::new(base_url, default_model);
        let mut tool_manager = ToolManager::new();

        // 利用可能なツールを登録
        tool_manager.register_tool(tools::shell::ShellTool);
        tool_manager.register_tool(tools::www::search::SearchEngineTool);
        tool_manager.register_tool(tools::www::browse::WebPageBrowser);
        tool_manager.register_tool(tools::files::info::InfoTool);
        tool_manager.register_tool(tools::files::read::ReadTool);
        tool_manager.register_tool(tools::files::write::WriteTool);
        tool_manager.register_tool(tools::utils::weather::WeatherTool);

        // デフォルトのプロンプトテンプレートを読み込む
        let default_prompt_template = include_str!("default-prompt.md").to_string();

        // ログファイルを初期化
        let log_file_path = Self::initialize_log_file();

        AIAgent {
            api,
            messages: vec![],
            tool_manager,
            default_prompt_template,
            log_file_path, // 初期化したパスを設定
        }
    }

    /// ログファイルの初期化とパス生成
    fn initialize_log_file() -> Option<PathBuf> {
        if let Some(mut home) = home_dir() {
            home.push(".cache");
            home.push("ai-integration");

            // ディレクトリが存在しない場合は作成
            if let Err(e) = std::fs::create_dir_all(&home) {
                eprintln!("Failed to create log directory {}: {}", home.display(), e);
                return None;
            }

            // 現在の日時でファイル名を生成 (yyyy-mm-dd_hh-mm-ss.log)
            let now = Local::now();
            let file_name = format!("{}.log", now.format("%Y-%m-%d_%H-%M-%S"));
            home.push(file_name);

            println!("Log file will be saved at: {}", home.display());
            Some(home)
        } else {
            eprintln!("Could not determine home directory for logging.");
            None
        }
    }

    /// ログファイルにメッセージを書き込むヘルパー関数
    fn write_message_to_log(&self, message: &ChatMessage) {
        if let Some(ref path) = self.log_file_path {
            // ログエントリのフォーマット: [タイムスタンプ] ロール: コンテンツ
            let log_entry = format!(
                "[{}] {}: {}\n---\n", // 各メッセージの終わりに "---" を追加して区切りを明確にする
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                message.role,
                message.content
            );
            // ファイルを追記モードで開き、存在しない場合は作成
            match OpenOptions::new().create(true).append(true).open(path) {
                Ok(mut file) => {
                    if let Err(e) = file.write_all(log_entry.as_bytes()) {
                        eprintln!("Failed to write to log file {}: {}", path.display(), e);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to open log file {}: {}", path.display(), e);
                }
            }
        }
    }

    /// エージェントの内部APIを通じてモデルを設定する公開メソッド
    pub fn set_model(&mut self, model_name: String) {
        self.api.set_model(model_name);
    }

    /// 利用可能なモデルをリストアップ
    pub async fn list_available_models(&self) -> Result<serde_json::Value, OllamaApiError> {
        self.api.list_models().await
    }

    /// ツール使用を伴うリアルタイムチャットセッションを開始
    /// この関数は、AIの応答をストリームし、ツール呼び出しを検出して実行し、その結果をAIにフィードバックして次の思考を促します。
    pub async fn chat_with_tools_realtime(
        self_arc_mutex: Arc<Mutex<Self>>,
        initial_messages: Vec<ChatMessage>, // 初期メッセージ (変更可能)
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<AgentEvent, OllamaApiError>> + Send>>,
        OllamaApiError,
    > {
        let agent_stream = async_stream::stream! {
            """            let mut agent_locked = self_arc_mutex.lock().await;
            let mut loop_messages = agent_locked.messages.clone();

            // --- システムプロンプトを注入 (最初の一度のみ) ---
            let has_system_prompt_already = loop_messages.iter().any(|msg| {
                msg.role == ChatRole::System && msg.content.contains("You are a helpful assistant")
            });

            if !has_system_prompt_already {
                let tool_manager_schemas = agent_locked.tool_manager.get_tool_json_schemas();
                let formatted_prompt = agent_locked
                    .default_prompt_template
                    .replace("{{TOOLS_JSON_SCHEMA}}", &tool_manager_schemas.to_string());

                let system_message = ChatMessage {
                    role: ChatRole::System,
                    content: formatted: formatted_prompt,
                };

                // ユーザーメッセージの直前、またはリストの先頭に挿入
                let insert_index = loop_messages
                    .iter()
                    .rposition(|m| m.role == ChatRole::User)
                    .unwrap_or(0);
                loop_messages.insert(insert_index, system_message.clone());
                agent_locked.messages.insert(insert_index, system_message);
            }

            let api_clone = agent_locked.api.clone();
            drop(agent_locked); // Mutexを早めに解放

            loop {
                // --- 3. AI応答ストリームを取得 ---
                let mut ai_response_stream = api_clone
                    .get_chat_completion_stream(loop_messages.clone())
                    .await?;

                // --- 4. AIからのストリームを処理し、ツール呼び出しが検出されたら中断 ---
                let mut full_ai_response_content = String::new();
                let mut call_tool_option: Option<AiToolCall> = None;

                'stream_loop: while let Some(chunk_result) = ai_response_stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            full_ai_response_content.push_str(&chunk);
                            yield Ok(AgentEvent::AiResponseChunk(chunk.clone()));

                            if let Some(call_tool) = extract_tool_call_from_response(&full_ai_response_content) {
                                call_tool_option = Some(call_tool);
                                break 'stream_loop;
                            }
                        }
                        Err(e) => {
                            yield Err(e);
                            return;
                        }
                    }
                }

                // --- 5. AIの完全な応答を履歴に追加 ---
                let assistant_message = ChatMessage {
                    role: ChatRole::Assistant,
                    content: full_ai_response_content.clone(),
                };
                {
                    let mut agent_locked = self_arc_mutex.lock().await;
                    // ツール呼び出しがない場合のみ、ここでアシスタントメッセージを追加
                    // ツール呼び出しがある場合は、TUI側で追加される
                    if call_tool_option.is_none() {
                        agent_locked.add_message_to_history(assistant_message.clone());
                    }
                }
                loop_messages.push(assistant_message);


                // --- 6. ツール呼び出しが検出された場合、それを実行 ---
                if let Some(call_tool) = call_tool_option {
                    yield Ok(AgentEvent::ToolCallDetected(call_tool.clone()));
                    yield Ok(AgentEvent::ToolExecuting(call_tool.tool_name.clone()));

                    let tool_result_outcome = {
                        let agent_locked = self_arc_mutex.lock().await;
                        agent_locked.tool_manager.execute_tool(
                            &call_tool.tool_name,
                            call_tool.parameters.clone()
                        ).await
                    };

                    let (tool_message, event) = match tool_result_outcome {
                        Ok(tool_result) => {
                            let content = serde_yaml::to_string(&serde_json::json!({
                                "tool_result": { "tool_name": &call_tool.tool_name, "result": &tool_result }
                            })).unwrap_or_default();
                            (
                                ChatMessage {
                                    role: ChatRole::System,
                                    content: format!("---
{}
---", content),
                                },
                                AgentEvent::ToolResult(call_tool.tool_name.clone(), tool_result)
                            )
                        }
                        Err(e) => {
                            let error_message = format!("{:?}", e);
                            let content = serde_yaml::to_string(&serde_json::json!({
                                "tool_error": { "tool_name": &call_tool.tool_name, "error": &error_message }
                            })).unwrap_or_default();
                            (
                                ChatMessage {
                                    role: ChatRole::System,
                                    content: format!("---
{}
---", content),
                                },
                                AgentEvent::ToolError(call_tool.tool_name.clone(), error_message)
                            )
                        }
                    };

                    yield Ok(event);
                    {
                        let mut agent_locked = self_arc_mutex.lock().await;
                        agent_locked.add_message_to_history(tool_message.clone());
                    }
                    loop_messages.push(tool_message);


                    yield Ok(AgentEvent::Thinking("AI is considering the tool's result...".to_string()));
                    // ループを続行してAIに再度思考させる
                } else {
                    // ツール呼び出しがなければループを終了
                    break;
                }""
        };

        Ok(Box::pin(agent_stream))
    }

    /// メッセージを履歴に追加し、ログファイルにも書き込む
    pub fn add_message_to_history(&mut self, message: ChatMessage) {
        // メッセージを履歴に追加する前にログに書き込む
        self.write_message_to_log(&message);
        self.messages.push(message);
    }

    /// 最後のユーザーメッセージとそれに続くAIの応答/ツール実行結果を履歴から削除
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
            // ログの整合性のため、履歴から削除されたメッセージはログファイルからは消さない方針とする。
        }
        // ユーザーメッセージが見つからない場合は何もしない
    }
}
