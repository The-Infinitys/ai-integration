// src/modules/chat.rs
use crate::modules::agent::AIAgent;
use std::io::{self, Write};
use futures_util::StreamExt;
use colored::*;

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
        println!("{}", "チャットを開始します。".green().bold());
        println!("{}", "スラッシュコマンド: /exit, /help, /info".cyan());
        println!("{}", "シェルコマンド: !<command>".cyan());

        println!("\n{}", "利用可能なOllamaモデル:".purple().bold());
        match self.agent.list_models().await {
            Ok(models) => {
                if let Some(tags) = models["models"].as_array() {
                    for model in tags {
                        if let Some(name) = model["name"].as_str() {
                            println!("- {}", name.yellow());
                        }
                    }
                }
            },
            Err(e) => eprintln!("{} {:?}", "モデル一覧の取得に失敗しました:".red().bold(), e),
        }
        println!("\n");

        loop {
            print!("{}", "あなた: ".blue().bold());
            io::stdout().flush().map_err(|e| format!("出力のフラッシュに失敗しました: {}", e))?;

            let mut user_input = String::new();
            io::stdin().read_line(&mut user_input)
                .map_err(|e| format!("入力の読み込みに失敗しました: {}", e))?;
            let user_input = user_input.trim();

            if user_input.is_empty() {
                continue;
            }

            // スラッシュコマンドの処理
            if user_input.starts_with('/') {
                match user_input {
                    "/exit" => {
                        println!("{}", "チャットを終了します。".green());
                        break;
                    },
                    "/help" => {
                        println!("{}", "\n--- ヘルプ ---".green().bold());
                        println!("{}", "  /exit       : チャットを終了します。".cyan());
                        println!("{}", "  /help       : このヘルプメッセージを表示します。".cyan());
                        println!("{}", "  /info       : エージェントの現在の状態を表示します。(未実装)".cyan());
                        println!("{}", "  !<command>  : シェルコマンドを実行し、その結果をAIに渡します。例: !ls -l".cyan());
                        println!("{}", "---------------".green().bold());
                        continue;
                    },
                    "/info" => {
                        println!("{}", "AIエージェント情報: (未実装)".yellow());
                        continue;
                    },
                    _ => {
                        println!("{}", "不明なコマンドです。/help で利用可能なコマンドを確認してください。".red());
                        continue;
                    }
                }
            }

            // シェルコマンドの処理
            if user_input.starts_with('!') {
                let command_and_args = &user_input[1..]; // '!' を除く
                println!("{}", "AI: ".green().bold()); // AIのプロンプトを先に表示

                println!("{}", "  シェルコマンドを実行中...".truecolor(128, 128, 128)); // グレー
                
                let shell_result = self.agent.tool_manager.execute_tool(
                    "shell",
                    serde_json::json!({
                        "command": command_and_args.split_whitespace().next().unwrap_or(""),
                        "args": command_and_args.split_whitespace().skip(1).collect::<Vec<&str>>()
                    })
                ).await;

                match shell_result {
                    Ok(result) => {
                        let result_str = serde_json::to_string_pretty(&result).unwrap_or_default();
                        println!("{}", format!("  コマンド結果:\n{}", result_str).truecolor(128, 128, 128)); // グレー
                        
                        let feedback_message = format!(
r#"---
tool_result:
  tool_name: shell
  result: |
    {}
---"#,
                            result_str
                        );
                        self.agent.add_ai_response(feedback_message);

                        println!("{}", "  AIがツール結果を考慮中...".normal()); // 思考（通常の文字）
                        
                        // chat_with_tools内部でループが管理されているため、ユーザーはAIの次の出力を待つ
                        match self.agent.chat_with_tools("ツール実行結果に基づいて次のアクションをしてください。".to_string()).await {
                             Ok(mut stream) => {
                                while let Some(chunk_result) = stream.next().await {
                                    match chunk_result {
                                        Ok(chunk) => {
                                            print!("{}", chunk.bold()); // AIの最終応答を太字で表示
                                            io::stdout().flush().map_err(|e| format!("出力のフラッシュに失敗しました: {}", e))?;
                                        },
                                        Err(e) => {
                                            eprintln!("\n{} {:?}", "ストリームからの読み込みエラー:".red().bold(), e);
                                            break;
                                        }
                                    }
                                }
                                println!();
                            },
                            Err(e) => {
                                eprintln!("\n{} {:?}", "AIとの通信エラー:".red().bold(), e);
                            }
                        }
                    },
                    Err(e) => {
                        eprintln!("{} {:?}", "シェルコマンド実行エラー:".red().bold(), e);
                    }
                }
                continue;
            }

            // 通常のAIチャット処理
            print!("{}", "AI: ".green().bold());
            io::stdout().flush().map_err(|e| format!("出力のフラッシュに失敗しました: {}", e))?;

            // AIの思考中表示 (AIが実際にツールを呼び出す前の初期フェーズ)
            println!("{}", "  AIが思考中...".normal()); // 思考（通常の文字）

            match self.agent.chat_with_tools(user_input.to_string()).await {
                Ok(mut stream) => {
                    // ここで、AIからのストリームがツール呼び出しの場合はagent.rs側で処理され、
                    // 最終的なテキスト応答だけが返ってくることを期待している。
                    // ツール呼び出しが発生した場合、agent.rsのループ内で表示される。
                    while let Some(chunk_result) = stream.next().await {
                        match chunk_result {
                            Ok(chunk) => {
                                print!("{}", chunk.bold()); // AIの最終応答を太字で表示
                                io::stdout().flush().map_err(|e| format!("出力のフラッシュに失敗しました: {}", e))?;
                            },
                            Err(e) => {
                                eprintln!("\n{} {:?}", "ストリームからの読み込みエラー:".red().bold(), e);
                                break;
                            }
                        }
                    }
                    println!();
                },
                Err(e) => {
                    eprintln!("\n{} {:?}", "AIとの通信エラー:".red().bold(), e);
                    self.agent.revert_last_user_message();
                }
            }
        }
        Ok(())
    }
}