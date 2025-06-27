// src/main.rs

use ai_integration::{ChatApp, ChatInterfaceType};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let interface_type = if std::env::var("AI_AGENT_UI").unwrap_or_default() == "tui" {
        println!("TUIモードで起動します...");
        ChatInterfaceType::Tui
    } else {
        println!("CLIモードで起動します。 'exit' または 'quit' で終了します。");
        ChatInterfaceType::Cli
    };

    let mut app = ChatApp::with_interface(interface_type);
    app.run().await?;

    println!("AIエージェントCLIを終了します。");
    Ok(())
}
