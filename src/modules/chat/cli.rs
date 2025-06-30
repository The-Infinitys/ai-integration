use crate::modules::agent::api::ChatRole;
use crate::modules::agent::AgentEvent;
use crate::modules::chat::ChatSession; // 親モジュールからChatSessionをインポート
use anyhow::Result;
use colored::*;
use futures_util::stream::StreamExt;
use std::io::{self, Write};

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
        println!(
            "{}: {}",
            "Default Ollama Model".cyan().bold(),
            self.chat_session.current_model.cyan()
        );

        println!("\n{}", "AI Integration Chat Session".purple().bold());
        println!("{}", "'/exit' と入力して終了します。".blue());
        println!(
            "{}",
            "'/model <モデル名>' と入力してモデルを変更します。".blue()
        );
        println!(
            "{}",
            "'/list models' と入力して利用可能なモデルを表示します。".blue()
        );
        println!(
            "{}",
            "'/revert' と入力して最後のターンを元に戻します。".blue()
        );

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
                    eprintln!(
                        "{}: {}",
                        "モデルの設定中にエラーが発生しました".red().bold(),
                        e.to_string().red()
                    );
                } else {
                    println!(
                        "{}: {}",
                        "モデルを設定しました".cyan().bold(),
                        self.chat_session.current_model.cyan()
                    );
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
                            println!(
                                "{}",
                                "モデルが見つからないか、予期しない応答形式です。".red()
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "{}: {:?}",
                            "モデルのリスト中にエラーが発生しました".red().bold(),
                            e.to_string().red()
                        );
                    }
                }
                continue;
            } else if input.eq_ignore_ascii_case("/revert") {
                self.chat_session.revert_last_turn().await;
                println!("\n{}", "最後のターンを元に戻しました。".yellow());
                continue;
            }

            println!("{}: {}", "あなた".blue().bold(), input);

            self.chat_session.add_user_message(input.to_string()).await;

            if let Err(e) = self.handle_chat_session().await {
                eprintln!(
                    "{}: {}",
                    "チャットセッション中にエラーが発生しました".red().bold(),
                    e.to_string().red()
                );
            }
        }

        println!("\n{}", "チャットセッションを終了しました。".purple().bold());
        Ok(())
    }

    async fn handle_chat_session(&mut self) -> Result<()> {
        let mut full_ai_response = String::new();
        let mut tool_output_received_this_turn = false;

        let mut stream = self.chat_session.start_realtime_chat().await?;

        while let Some(event_result) = stream.next().await {
            match event_result {
                Ok(event) => {
                    match event {
                        AgentEvent::AiResponseChunk(chunk) => {
                            print!("{}", chunk.bright_green());
                            io::stdout().flush().unwrap();
                            full_ai_response.push_str(&chunk);
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
                                    .unwrap_or_else(|_| "パラメータのシリアライズエラー".to_string())
                                    .yellow()
                            );
                            println!("{}", "---------------------------------".yellow().bold());
                            io::stdout().flush().unwrap();
                            tool_output_received_this_turn = true;
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
                                    .unwrap_or_else(|_| "ツール結果のシリアライズエラー".to_string())
                                    .green()
                            );
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
                        // 他のイベントも必要に応じてここで処理
                        _ => {}
                    }
                }
                Err(e) => {
                    eprintln!(
                        "\n{}: {:?}",
                        "ストリームエラー".red().bold(),
                        e.to_string().red()
                    );
                    return Err(e.into());
                }
            }
        }

        if !tool_output_received_this_turn {
            if !full_ai_response.is_empty() {
                self.chat_session
                    .add_assistant_message_to_history(full_ai_response)
                    .await;
            }
        }

        Ok(())
    }
}
