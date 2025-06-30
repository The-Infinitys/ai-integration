// src/main.rs
use ai_integration::modules::chat::ChatSession;
use colored::*;
use std::io::{self, Write};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let ollama_base_url = "http://localhost:11434".to_string();
    let default_ollama_model = "gemma3:latest".to_string(); // ご利用のモデル名に合わせる

    println!("{}: {}", "Ollama API Base URL".cyan().bold(), ollama_base_url.cyan());
    println!("{}: {}", "Default Ollama Model".cyan().bold(), default_ollama_model.cyan());

    let mut chat_session = ChatSession::new(ollama_base_url, default_ollama_model);

    println!("\n{}", "AI Integration Chat Session".purple().bold());
    println!("{}", "'exit' と入力して終了します。".blue());
    println!("{}", "'model <モデル名>' と入力してモデルを変更します。".blue());
    println!("{}", "'list models' と入力して利用可能なモデルを表示します。".blue());

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
            if let Err(e) = chat_session.set_model(model_name).await {
                eprintln!("{}: {}", "モデルの設定中にエラーが発生しました".red().bold(), e.to_string().red());
            }
            continue;
        } else if input.eq_ignore_ascii_case("/list models") {
            match chat_session.list_models().await {
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
        }

        // ★修正：ユーザー入力の表示はここで行い、ChatSession::add_user_messageからは削除
        println!("{}: {}", "あなた".blue().bold(), input);
        
        chat_session.add_user_message(input.to_string()).await;

        if let Err(e) = chat_session.start_realtime_chat().await {
            eprintln!("{}: {}", "チャットセッション中にエラーが発生しました".red().bold(), e.to_string().red());
        }
    }

    println!("\n{}", "チャットセッションを終了しました。".purple().bold());
    Ok(())
}