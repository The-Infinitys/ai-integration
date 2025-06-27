// src/modules/chat/cli.rs

use super::Chat; // 親モジュールで定義されたChatトレイトをインポート
use async_trait::async_trait;
use std::error::Error;
use std::io::{self, Write};

/// `CommandLineChat`は`Chat`トレイトを実装し、CLIでの入出力を行います。
pub struct CommandLineChat;

#[async_trait]
impl Chat for CommandLineChat {
    async fn get_user_input(&self) -> Result<String, Box<dyn Error>> {
        print!(">> "); // プロンプト表示
        io::stdout().flush()?; // プロンプトをすぐに表示

        let mut input = String::new();
        io::stdin().read_line(&mut input)?; // ユーザーからの入力を読み込む
        Ok(input.trim().to_string()) // 前後の空白を削除して返す
    }

    async fn display_ai_response(&self, response: &str) -> Result<(), Box<dyn Error>> {
        println!("{}", response); // AIの応答を表示
        Ok(())
    }
}
