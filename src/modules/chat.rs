// src/modules/chat.rs
use crate::modules::agent::AIAgent; // AIAgentを直接利用
use std::io::{self, Write};
use futures_util::StreamExt; // StreamExtトレイトをインポート

pub struct ChatSession {
    agent: AIAgent, // AIApiではなくAIAgentを持つ
}

impl ChatSession {
    pub fn new(agent: AIAgent) -> Self {
        ChatSession {
            agent,
        }
    }

    pub async fn start_chat(&mut self) -> Result<(), String> {
        println!("チャットを開始します。'exit' と入力すると終了します。");

        // Ollamaで使用可能なモデルを一覧表示
        println!("\n利用可能なOllamaモデル:");
        match self.agent.list_models().await { // agentを介してモデルリストを取得
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

            let mut full_ai_response = String::new();
            // agentを介してチャットストリームを開始
            match self.agent.chat_stream(user_input.to_string()).await {
                Ok(mut stream) => {
                    while let Some(chunk_result) = stream.next().await {
                        match chunk_result {
                            Ok(chunk) => {
                                print!("{}", chunk);
                                full_ai_response.push_str(&chunk);
                                io::stdout().flush().map_err(|e| format!("出力のフラッシュに失敗しました: {}", e))?;
                            },
                            Err(e) => {
                                eprintln!("\nストリームからの読み込みエラー: {:?}", e);
                                break;
                            }
                        }
                    }
                    println!(); // AIの応答の最後に改行
                    // AIの最終応答をagentに通知して履歴に追加させる
                    self.agent.add_ai_response(full_ai_response);
                },
                Err(e) => {
                    eprintln!("\nAIとの通信エラー: {:?}", e);
                    // エラー時はagentに最後のユーザーメッセージを削除させる
                    self.agent.revert_last_user_message();
                }
            }
        }
        Ok(())
    }
}