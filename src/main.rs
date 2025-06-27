// src/main.rs

use ai_integration::{AIAgentApiType, ChatApp, ChatInterfaceType};
use std::error::Error; // Box<dyn Error>のために必要

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let interface_type = if std::env::var("AI_AGENT_UI").unwrap_or_default() == "tui" {
        println!("TUIモードで起動します...");
        ChatInterfaceType::Tui
    } else {
        println!("CLIモードで起動します。 'exit' または 'quit' で終了します。");
        ChatInterfaceType::Cli
    };

    // 現時点ではOllamaをデフォルトとして固定します
    let api_type = AIAgentApiType::Ollama;

    let mut app = ChatApp::with_interface_and_api(interface_type, api_type);
    app.run().await?;

    println!("AIエージェントCLIを終了します。");
    Ok(())
}
