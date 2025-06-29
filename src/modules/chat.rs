// src/modules/chat.rs
use crate::modules::agent::AIAgent;
use std::io::{self, Write};
use futures_util::StreamExt;

pub struct ChatSession {
    agent: AIAgent,
}

impl ChatSession {
    pub fn new(agent: AIAgent) -> Self {
        ChatSession {
            agent,
        }
    }

    pub async fn start_chat(&mut self) -> Result<(), String> {
        println!("チャットを開始します。'exit' と入力すると終了します。");

        println!("\n利用可能なOllamaモデル:");
        match self.agent.list_models().await {
            Ok(models) => {
                if let Some(tags) = models["models"].as_array() {
                    for model in tags {
                        if let Some(name) = model["name"].as_str() {
                            println!("- {}", name);
                        }
                    }
                }
            },
            Err(e) => eprintln!("モデル一覧の取得に失敗しました: {:?}", e),
        }
        println!("\n");

        loop {
            print!("あなた: ");
            io::stdout().flush().map_err(|e| format!("出力のフラッシュに失敗しました: {}", e))?;

            let mut user_input = String::new();
            io::stdin().read_line(&mut user_input)
                .map_err(|e| format!("入力の読み込みに失敗しました: {}", e))?;
            let user_input = user_input.trim();

            if user_input.eq_ignore_ascii_case("exit") {
                println!("チャットを終了します。");
                break;
            }

            print!("AI: ");
            io::stdout().flush().map_err(|e| format!("出力のフラッシュに失敗しました: {}", e))?;

            match self.agent.chat_with_tools(user_input.to_string()).await {
                Ok(mut stream) => {
                    while let Some(chunk_result) = stream.next().await {
                        match chunk_result {
                            Ok(chunk) => {
                                print!("{}", chunk);
                                io::stdout().flush().map_err(|e| format!("出力のフラッシュに失敗しました: {}", e))?;
                            },
                            Err(e) => {
                                eprintln!("\nストリームからの読み込みエラー: {:?}", e);
                                break;
                            }
                        }
                    }
                    println!();
                },
                Err(e) => {
                    eprintln!("\nAIとの通信エラー: {:?}", e);
                    self.agent.revert_last_user_message();
                }
            }
        }
        Ok(())
    }
}