// src/main.rs
use std::env;
use std::pin::Pin;

// 新しいモジュールをインポート
mod ai;
mod aurascript;
mod config;
mod help;
mod prompt; // aurascriptモジュールをインポート

use ai::{AIGenerator, GeminiChat, OllamaChat, OpenAIChat};
use config::Config;

#[tokio::main] // main関数を非同期にする
async fn main() {
    let args: Vec<String> = env::args().collect();

    // 設定を初期化 (APIキーがハードコードされている警告が表示されます)
    let config = Config::new();

    match args.get(1).map(|s| s.as_str()) {
        Some("help") => {
            help::display_help();
        }
        Some("prompt") => {
            if let Some(provider_name_str) = args.get(2) {
                let generator: Pin<Box<dyn AIGenerator>> = {
                    let provider_enum = match provider_name_str.as_str() {
                        "openai" => ai::AIProvider::OpenAI,
                        "ollama" => ai::AIProvider::Ollama,
                        "gemini" => ai::AIProvider::Gemini,
                        _ => {
                            eprintln!(
                                "エラー: 不明なAIプロバイダー '{}' です。'openai', 'ollama', 'gemini' のいずれかを指定してください。",
                                provider_name_str
                            );
                            return;
                        }
                    };

                    match provider_enum {
                        ai::AIProvider::OpenAI => Box::pin(
                            OpenAIChat::new(&config)
                                .expect("OpenAIクライアントの初期化に失敗しました"),
                        ),
                        ai::AIProvider::Ollama => Box::pin(OllamaChat::new(&config)),
                        ai::AIProvider::Gemini => Box::pin(
                            GeminiChat::new(&config)
                                .expect("Geminiクライアントの初期化に失敗しました"),
                        ),
                    }
                };
                if let Err(e) = prompt::start_chat_loop(generator).await {
                    eprintln!("チャットエラー: {}", e);
                }
            } else {
                eprintln!(
                    "エラー: 'prompt' コマンドにはAIプロバイダーを指定する必要があります (例: cargo run -- prompt openai)"
                );
                help::display_help();
            }
        }
        _ => {
            // デフォルトのAuraScript実行をaurascriptモジュールに移動
            aurascript::run_default_script(config).await;
        }
    }
}
