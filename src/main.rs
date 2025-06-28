// src/main.rs

use ai_integration::modules::prompt::Prompt; // Import the Prompt module
use ai_integration::modules::agent::{AIAgent, api::AIApi, Character}; // Import AIAgent, AIApi, Character
use std::io::{self, Write}; // Import the standard I/O library for input

fn main() -> Result<(), std::io::Error> {
    println!("Hello from AI Agent project!");
    println!("Type your message and press Enter. Type '/exit' to quit.");
    println!("Try commands like 'note test', 'get test note', or 'history' for demonstration.");

    // Initialize AIApi (currently a dummy instance)
    // AIApiを初期化（現在はダミーインスタンス）
    let api = AIApi::new();

    // Create an AIAgent instance
    // AIAgentインスタンスを作成
    let mut agent = AIAgent::new(
        api,
        "You are a helpful AI assistant. Please respond concisely.",
    );

    loop {
        print!("\nUser Input: ");
        // Ensure the prompt is displayed before reading input.
        // 入力を読み込む前にプロンプトが表示されるようにする。
        std::io::stdout().flush()?;
        let mut user_input = String::new();

        // Read a line from standard input
        // 標準入力から1行読み込み
        io::stdin()
            .read_line(&mut user_input)?; // Use '?' for error propagation

        // Trim whitespace and check for exit command
        // 前後の空白を削除し、終了コマンドをチェック
        let trimmed_input = user_input.trim();
        if trimmed_input.eq_ignore_ascii_case("/exit") {
            println!("Exiting AI Agent. Goodbye!");
            break; // Exit the loop
        }

        // Add user message to agent's chat history
        // ユーザーメッセージをエージェントのチャット履歴に追加
        agent.add_message(Character::User, Character::Agent, trimmed_input);

        // For now, we simulate the AI's internal prompt generation based on user input.
        // In a real application, this would involve sending `trimmed_input` (or a more complex prompt
        // derived from chat history) to the AI API and getting a response.
        // 現時点では、ユーザー入力に基づいてAIの内部プロンプト生成をシミュレートします。
        // 実際のアプリケーションでは、これは`trimmed_input`（またはチャット履歴から派生した
        // より複雑なプロンプト）をAI APIに送信し、応答を取得する作業になります。
        println!("\nAI Agent's Internal Prompt (simulated input to AI):");
        if let Some(main_prompt) = agent.get_main_system_prompt() {
            println!("System: {}", main_prompt);
        }
        println!("User: {}", trimmed_input);

        // Simulate an AI response and add it to chat history
        // AIの応答をシミュレートし、チャット履歴に追加
        let simulated_ai_response = format!("AI processed: \"{}\"", trimmed_input);
        agent.add_message(Character::Agent, Character::User, &simulated_ai_response);
        println!("\nAI Agent Response (simulated): {}", simulated_ai_response);

        // Example usage of notes and chat history (for testing purposes)
        // ノートとチャット履歴の利用例（テスト目的）
        if trimmed_input.contains("note test") {
            agent.add_note(vec!["test".to_string(), "example".to_string()], "Test Note Title", "This is the content of a test note.");
            println!("Note added: 'Test Note Title' with tags 'test', 'example'");
        } else if trimmed_input.contains("get test note") {
            let notes = agent.get_notes_by_tags(&["test".to_string()]);
            if !notes.is_empty() {
                println!("Found notes with tag 'test':");
                for note in notes {
                    println!("  Title: '{}', Data: '{}'", note.title, note.data);
                }
            } else {
                println!("No notes found with tag 'test'.");
            }
        } else if trimmed_input.eq_ignore_ascii_case("history") {
            println!("\n--- Chat History ---");
            if agent.get_chat_history().is_empty() {
                println!("No chat messages yet.");
            } else {
                for msg in agent.get_chat_history() {
                    println!("[{}] {:?} -> {:?}: {}", msg.date.format("%Y-%m-%d %H:%M:%S"), msg.from, msg.to, msg.text);
                }
            }
            println!("--------------------");
        }
    }
    Ok(())
}
