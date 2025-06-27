// src/lib.rs

pub mod modules;
pub mod utils;

// modules::chat::* ではなく、modules::chat::{cli, tui} のように直接参照
use modules::chat::{
    Chat,
    api::{AIAgentApi, OllamaAIAgentApi}, // OllamaAIAgentApiとAIAgentApiをインポート
    interface::cli::CommandLineChat,
};
use std::{error::Error, fmt};

/// `ChatApp`構造体はCLIチャットアプリケーション全体を管理します。
pub struct ChatApp {
    chat_interface: Box<dyn Chat>,
    ai_agent_api: Box<dyn AIAgentApi>, // AIエージェントAPIのインスタンスを追加
}

impl ChatApp {
    /// 新しい`ChatApp`のインスタンスを作成します。
    /// デフォルトではCLIインターフェースとOllama APIを使用します。
    pub fn new() -> Self {
        ChatApp {
            chat_interface: Box::new(CommandLineChat),
            ai_agent_api: Box::new(OllamaAIAgentApi::default()),
        }
    }

    /// 特定のチャットインターフェースとAIエージェントAPIを指定して`ChatApp`を作成します。
    pub fn with_interface_and_api(
        interface_type: ChatInterfaceType,
        api_type: AIAgentApiType,
    ) -> Self {
        let chat_interface: Box<dyn Chat> = match interface_type {
            ChatInterfaceType::Cli => Box::new(CommandLineChat),
            _ => {
                eprintln!("{} isn't supported yet. fall back!", interface_type);
                Box::new(CommandLineChat)
            }
        };

        let ai_agent_api: Box<dyn AIAgentApi> = match api_type {
            AIAgentApiType::Ollama => Box::new(OllamaAIAgentApi::default()), // 将来的に他のAPIタイプが追加される可能性があります
        };

        ChatApp {
            chat_interface,
            ai_agent_api,
        }
    }

    /// チャットアプリケーションの実行を開始します。
    /// ユーザーからの入力を受け取り、AIの応答を表示します。
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

            // AIエージェントのロジックを呼び出す
            let ai_response = self.ai_agent_api.get_ai_response(&user_input).await?;

            // AIの応答を表示する
            self.chat_interface
                .display_ai_response(&ai_response)
                .await?;
        }
        Ok(())
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

/// 使用するAIエージェントAPIのタイプを列挙します。
pub enum AIAgentApiType {
    Ollama,
    // Gemini, OpenAIなど、将来的に追加される可能性があります
}

impl fmt::Display for AIAgentApiType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Ollama => "ollama",
            }
        )
    }
}
