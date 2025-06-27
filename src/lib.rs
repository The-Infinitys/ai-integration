// src/lib.rs

pub mod modules;

// modules::chat::* ではなく、modules::chat::{cli, tui} のように直接参照
use modules::chat::{Chat, cli::CommandLineChat};
use std::{error::Error, fmt};

/// `ChatApp`構造体はCLIチャットアプリケーション全体を管理します。
pub struct ChatApp {
    chat_interface: Box<dyn Chat>,
    // どのインターフェースを使用するかを示すフィールドを追加することもできます
    // current_interface_type: ChatInterfaceType,
}

impl ChatApp {
    /// 新しい`ChatApp`のインスタンスを作成します。
    /// デフォルトではCLIインターフェースを使用します。
    pub fn new() -> Self {
        ChatApp {
            chat_interface: Box::new(CommandLineChat),
        }
    }

    /// 特定のチャットインターフェースを指定して`ChatApp`を作成します。
    pub fn with_interface(interface_type: ChatInterfaceType) -> Self {
        let chat_interface: Box<dyn Chat> = match interface_type {
            ChatInterfaceType::Cli => Box::new(CommandLineChat),
            _ => {
                eprintln!("{} isn't supported yet. fall back!", interface_type);
                Box::new(CommandLineChat)
            }
        };
        ChatApp { chat_interface }
    }

    /// チャットアプリケーションの実行を開始します。
    /// ユーザーからの入力を受け取り、AIの応答（現在はダミー）を表示します。
    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        loop {
            // ユーザーからの入力を受け取る
            let user_input = self.chat_interface.get_user_input().await?;

            // 終了コマンドのチェック
            if user_input.trim().eq_ignore_ascii_case("exit")
                || user_input.trim().eq_ignore_ascii_case("quit")
            {
                break;
            }

            // ここでAIエージェントのロジックを呼び出す
            // 現時点ではダミーの応答を返します
            let ai_response = self.process_ai_agent_logic(&user_input).await?;

            // AIの応答を表示する
            self.chat_interface
                .display_ai_response(&ai_response)
                .await?;
        }
        Ok(())
    }

    /// AIエージェントのダミーロジック。
    /// 将来的には、LLM呼び出し、ツール実行、コード編集などがここに入ります。
    async fn process_ai_agent_logic(&self, input: &str) -> Result<String, Box<dyn Error>> {
        // ここに実際のAIエージェントの処理を記述
        // 例えば、LLMへのAPIコール、ファイル操作、コマンド実行など
        let response = format!("AIからの応答: 「{}」について考え中です...", input);
        Ok(response)
    }
}

impl Default for ChatApp {
    fn default() -> Self {
        Self::new()
    }
}

/// 使用するチャットインターフェースのタイプを列挙します。
pub enum ChatInterfaceType {
    Cli,
    Tui,
    Gui,
}
impl fmt::Display for ChatInterfaceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Gui => "gui",
                Self::Cli => "cli",
                Self::Tui => "tui",
            }
        )
    }
}
