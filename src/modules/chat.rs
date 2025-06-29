// src/modules/chat.rs
use crate::modules::agent::AIAgent;
use std::io::{self, Write};
use futures_util::StreamExt;
use colored::*; // colored クレートをインポート

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
                        // ここにAIエージェントの内部状態（メッセージ履歴など）を表示するロジックを追加できます
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

                println!("{}", "  シェルコマンドを実行中...".yellow());
                // shellツールに直接コマンドを渡す
                // この部分は agent.rs に移動した方が良いかもしれませんが、まずはここで簡易的に実装
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
                        println!("{}", format!("  コマンド結果:\n{}", result_str).green());
                        // シェルコマンドの結果をAIにフィードバック
                        // ここでAIにフィードバックするためのChatMessageを作成し、agentのmessagesに追加
                        let feedback_message = format!(
r#"---
tool_result:
  tool_name: shell
  result: |
    {}
---"#,
                            result_str
                        );
                        self.agent.add_ai_response(feedback_message); // AIの応答として追加し、次のAIのターンで考慮させる

                        // シェルコマンドの結果を受けてAIに推論させるため、再度チャット処理を呼び出す
                        // これは新しい入力としてではなく、既存のチャットコンテキストの継続として扱う
                        // chat_with_tools内部でループが管理されているため、ユーザーはAIの次の出力を待つ
                        println!("{}", "  AIがコマンド結果に基づいて推論中...".yellow());
                        // chat_with_toolsのループがツール結果をシステムメッセージとして追加し、AIに次の応答を促すため、
                        // ここでは単にAIの出力を待つことになる
                        match self.agent.chat_with_tools("ツール実行結果に基づいて次のアクションをしてください。".to_string()).await { // ダミーのユーザー入力
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
                        // エラーもAIにフィードバックできますが、今回はユーザーに直接表示
                    }
                }
                continue; // シェルコマンド処理後は次のユーザー入力を待つ
            }

            // 通常のAIチャット処理
            print!("{}", "AI: ".green().bold());
            io::stdout().flush().map_err(|e| format!("出力のフラッシュに失敗しました: {}", e))?;

            match self.agent.chat_with_tools(user_input.to_string()).await {
                Ok(mut stream) => {
                    while let Some(chunk_result) = stream.next().await {
                        match chunk_result {
                            Ok(chunk) => {
                                print!("{}", chunk.bold());
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