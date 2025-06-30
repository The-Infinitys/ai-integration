use crate::modules::agent::api::{ChatMessage, ChatRole};
use crate::modules::agent::{AIAgent, AgentEvent};
use anyhow::Result;
use colored::*;
use futures_util::stream::StreamExt;
use std::io::{self, Write};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Represents the main application.
pub struct App {
    chat_session: ChatSession,
}

impl App {
    /// Creates a new App instance.
    pub fn new(ollama_base_url: String, default_ollama_model: String) -> Self {
        let chat_session = ChatSession::new(ollama_base_url, default_ollama_model);
        App { chat_session }
    }

    /// Runs the main application loop.
    pub async fn run(&mut self) -> std::io::Result<()> {
        println!("{}: {}", "Default Ollama Model".cyan().bold(), self.chat_session.current_model.cyan());

        println!("\n{}", "AI Integration Chat Session".purple().bold());
        println!("{}", "'/exit' と入力して終了します。".blue());
        println!("{}", "'/model <モデル名>' と入力してモデルを変更します。".blue());
        println!("{}", "'/list models' と入力して利用可能なモデルを表示します。".blue());
        println!("{}", "'/revert' と入力して最後のターンを元に戻します。".blue());


        loop {
            print!("\n{}: ", "あなた".blue().bold());
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim();

            if input.eq_ignore_ascii_case("/exit") {
                break;
            } else if input.starts_with("/model ") {
                let model_name = input.trim_start_matches("/model ").trim().to_string();
                if let Err(e) = self.chat_session.set_model(model_name).await {
                    eprintln!("{}: {}", "モデルの設定中にエラーが発生しました".red().bold(), e.to_string().red());
                }
                continue;
            } else if input.eq_ignore_ascii_case("/list models") {
                match self.chat_session.list_models().await {
                    Ok(models) => {
                        println!("\n{}", "利用可能なモデル:".yellow().bold());
                        if let Some(model_list) = models["models"].as_array() {
                            for model in model_list {
                                if let Some(name) = model["name"].as_str() {
                                    println!("- {}", name.yellow());
                                }
                            }
                        } else {
                            println!("{}", "モデルが見つからないか、予期しない応答形式です。".red());
                        }
                    }
                    Err(e) => {
                        eprintln!("{}: {:?}", "モデルのリスト中にエラーが発生しました".red().bold(), e.to_string().red());
                    }
                }
                continue;
            } else if input.eq_ignore_ascii_case("/revert") {
                self.chat_session.revert_last_turn().await;
                continue;
            }

            println!("{}: {}", "あなた".blue().bold(), input);

            self.chat_session.add_user_message(input.to_string()).await;

            if let Err(e) = self.chat_session.start_realtime_chat().await {
                eprintln!("{}: {}", "チャットセッション中にエラーが発生しました".red().bold(), e.to_string().red());
            }
        }

        println!("\n{}", "チャットセッションを終了しました。".purple().bold());
        Ok(())
    }
}

/// AIエージェントとの単一のチャットセッションを表します。
pub struct ChatSession {
    agent: Arc<Mutex<AIAgent>>, // tokio::sync::Mutexを使用
    session_messages: Vec<ChatMessage>,
    current_model: String,
}

impl ChatSession {
    /// 新しいチャットセッションを作成します。
    pub fn new(base_url: String, default_model: String) -> Self {
        let agent = Arc::new(Mutex::new(AIAgent::new(base_url, default_model.clone())));
        ChatSession {
            agent,
            session_messages: vec![],
            current_model: default_model,
        }
    }

    /// ユーザーメッセージをセッション履歴に追加します。
    pub async fn add_user_message(&mut self, content: String) {
        let mut agent_locked = self.agent.lock().await;
        let user_message = ChatMessage {
            role: ChatRole::User,
            content,
        };
        agent_locked.add_message_to_history(user_message.clone());
        self.session_messages.push(user_message);
    }

    /// ツール実行を伴うリアルタイムチャットセッションを開始および管理します。
    pub async fn start_realtime_chat(&mut self) -> Result<()> {
        let mut full_ai_response = String::new();
        let mut current_turn_messages = self.session_messages.clone();

        loop {
            let agent_arc_clone = self.agent.clone();

            let mut chat_stream =
                AIAgent::chat_with_tools_realtime(agent_arc_clone, current_turn_messages.clone())
                    .await?;

            full_ai_response.clear();
            let mut pending_display = String::new();
            let mut tool_output_received_this_turn = false;

            while let Some(event_result) = chat_stream.next().await {
                match event_result {
                    Ok(event) => {
                        match event {
                            AgentEvent::AiResponseChunk(chunk) => {
                                print!("{}", chunk.bright_green());
                                io::stdout().flush().unwrap();
                                full_ai_response.push_str(&chunk);
                                pending_display.push_str(&chunk);
                            }
                            AgentEvent::AddMessageToHistory(_message) => {
                                // This event means the AI wants to add its own message to history.
                                // It should have already been added internally by the agent before this event is produced.
                                // May not need to do anything here, or could be used for UI updates.
                            }
                            AgentEvent::ToolCallDetected(tool_call) => {
                                println!(
                                    "\n{}",
                                    "--- ツール呼び出しを検出しました ---".yellow().bold()
                                );
                                println!(
                                    "{}: {}",
                                    "ツール名".yellow(),
                                    tool_call.tool_name.yellow()
                                );
                                println!(
                                    "{}: {}",
                                    "パラメータ".yellow(),
                                    serde_yaml::to_string(&tool_call.parameters)
                                        .unwrap_or_else(
                                            |_| "パラメータのシリアライズエラー".to_string()
                                        )
                                        .yellow()
                                ); // ★修正：YAML形式で表示
                                println!("{}", "---------------------------------".yellow().bold());
                                io::stdout().flush().unwrap();
                                if !pending_display.is_empty() {
                                    full_ai_response.push_str(&pending_display);
                                    pending_display.clear();
                                }
                                tool_output_received_this_turn = true;
                            }
                            AgentEvent::ToolExecuting(tool_name) => {
                                println!(
                                    "{}: {}...",
                                    "ツールを実行中".cyan().bold(),
                                    tool_name.cyan()
                                );
                                io::stdout().flush().unwrap();
                                if !pending_display.is_empty() {
                                    full_ai_response.push_str(&pending_display);
                                    pending_display.clear();
                                }
                            }
                            AgentEvent::ToolResult(tool_name, result) => {
                                println!(
                                    "\n{}: {}",
                                    "--- ツール結果".green().bold(),
                                    tool_name.green().bold()
                                );
                                println!(
                                    "{}",
                                    serde_yaml::to_string(&result)
                                        .unwrap_or_else(
                                            |_| "ツール結果のシリアライズエラー".to_string()
                                        )
                                        .green()
                                ); // ★修正：YAML形式で表示
                                println!("{}", "-----------------------------".green().bold());
                                io::stdout().flush().unwrap();
                                tool_output_received_this_turn = true;
                            }
                            AgentEvent::ToolError(tool_name, error_message) => {
                                eprintln!(
                                    "\n{}: {}",
                                    "--- ツールエラー".red().bold(),
                                    tool_name.red().bold()
                                );
                                eprintln!("{}: {}", "エラー".red(), error_message.red());
                                eprintln!("{}", "--------------------------".red().bold());
                                io::stdout().flush().unwrap();
                                tool_output_received_this_turn = true;
                            }
                            AgentEvent::Thinking(message) => {
                                println!(
                                    "\n{}: {}",
                                    "AI思考中".magenta().bold(),
                                    message.magenta()
                                );
                                io::stdout().flush().unwrap();
                            }
                            AgentEvent::UserMessageAdded => {
                                // This is internal to the agent, mainly for history management.
                                // User message display is already handled in add_user_message.
                            }
                            AgentEvent::AttemptingToolDetection => {
                                println!("\n{}", "ツール検出を試みています...".yellow());
                                io::stdout().flush().unwrap();
                                if !pending_display.is_empty() {
                                    full_ai_response.push_str(&pending_display);
                                    pending_display.clear();
                                }
                            }
                            AgentEvent::PendingDisplayContent(content) => {
                                print!("{}", content.bright_green());
                                io::stdout().flush().unwrap();
                                full_ai_response.push_str(&content);
                            }
                            AgentEvent::ToolBlockParseWarning(yaml_content) => {
                                eprintln!(
                                    "\n{}:\n{}",
                                    "警告: ツールブロックYAMLをパースできませんでした"
                                        .red()
                                        .bold(),
                                    yaml_content.red()
                                );
                                io::stdout().flush().unwrap();
                                full_ai_response.push_str(&format!("\n---\n{}\n---", yaml_content));
                            }
                            AgentEvent::YamlParseError(error_msg, yaml_content) => {
                                eprintln!(
                                    "\n{}: {}\n{}:\n{}",
                                    "YAMLツール呼び出しのパースエラー".red().bold(),
                                    error_msg.red(),
                                    "コンテンツ".red(),
                                    yaml_content.red()
                                );
                                io::stdout().flush().unwrap();
                                full_ai_response.push_str(&format!(
                                    "\nツール呼び出しのパースエラー: {}\nコンテンツ:\n{}",
                                    error_msg, yaml_content
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "\n{}: {:?}",
                            "ストリームエラー".red().bold(),
                            e.to_string().red()
                        );
                        let error_message = ChatMessage {
                            role: ChatRole::System,
                            content: format!("AIストリーム中にエラーが発生しました: {:?}", e),
                        };
                        let mut agent_locked = self.agent.lock().await;
                        agent_locked.add_message_to_history(error_message.clone());
                        self.session_messages.push(error_message);
                        return Err(e.into());
                    }
                }
            }

            if !tool_output_received_this_turn {
                if !full_ai_response.is_empty() {
                    self.session_messages.push(ChatMessage {
                        role: ChatRole::Assistant,
                        content: full_ai_response.clone(),
                    });
                }
                break;
            } else {
                let agent_locked = self.agent.lock().await;
                current_turn_messages = agent_locked.messages.clone();
                full_ai_response.clear();
            }
        }
        Ok(())
    }

    // pub fn get_messages(&self) -> &Vec<ChatMessage> {
    //     &self.session_messages
    // }

    pub async fn revert_last_turn(&mut self) {
        let mut agent_locked = self.agent.lock().await;
        let initial_history_len = agent_locked.messages.len();

        agent_locked.revert_last_user_message();

        if self
            .session_messages
            .last()
            .is_some_and(|m| m.role == ChatRole::User)
        {
            self.session_messages.pop();
        }

        while let Some(msg) = agent_locked.messages.last() {
            if msg.role != ChatRole::User && agent_locked.messages.len() >= initial_history_len {
                agent_locked.messages.pop();
            } else {
                break;
            }
        }
        self.session_messages = agent_locked.messages.clone();
        println!("\n{}", "最後のターンを元に戻しました。".yellow());
        io::stdout().flush().unwrap();
    }

    pub async fn set_model(&mut self, model_name: String) -> Result<()> {
        let mut agent_locked = self.agent.lock().await;
        agent_locked.set_model(model_name.clone());
        self.current_model = model_name;
        println!(
            "{}: {}",
            "モデルを設定しました".cyan().bold(),
            self.current_model.cyan()
        );
        Ok(())
    }

    pub async fn list_models(&self) -> Result<serde_json::Value> {
        let agent_locked = self.agent.lock().await;
        agent_locked
            .list_available_models()
            .await
            .map_err(anyhow::Error::from)
    }
}