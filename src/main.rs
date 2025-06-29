// src/main.rs
use tokio;
use ai_integration::modules::agent::AIAgent;
use ai_integration::modules::chat::ChatSession;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let ollama_base_url = "http://localhost:11434".to_string();
    let default_ollama_model = "gemma3:latest".to_string(); // ご利用のモデル名に合わせる

    println!("Ollama API Base URL: {}", ollama_base_url);
    println!("Default Ollama Model: {}", default_ollama_model);

    // AIAgentを初期化
    let agent = AIAgent::new(ollama_base_url, default_ollama_model);
    
    // ChatSessionにAIAgentのインスタンスを渡す
    let mut chat_session = ChatSession::new(agent);

    if let Err(e) = chat_session.start_chat().await {
        eprintln!("チャット中にエラーが発生しました: {}", e);
    }
    Ok(())
}