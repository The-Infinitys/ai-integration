// src/main.rs

use ai_integration::modules::prompt::Prompt; // Promptモジュールをインポート
// AIAgent, AIApi, ApiClient, openai::OpenAIApi, Character をインポート
use ai_integration::modules::agent::{AIAgent, api::{AIApi, ApiClient, openai::OpenAIApi}, Character};
use std::io::{self, Write}; // 標準I/Oライブラリをインポート
use tokio; // async mainのためにtokioをインポート

// Define the async main function as we will be making async HTTP calls.
// 非同期HTTP呼び出しを行うため、非同期のメイン関数を定義します。
#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    println!("Hello from AI Agent project!");
    println!("Type your message and press Enter. Type '/exit' to quit.");
    println!("\n--- Using Default AI Agent Configuration ---");
    println!("  API Base URL: http://localhost:11434/v1/chat/completions");
    println!("  Default Model: llama2 (or similar local LLM)");
    println!("  No OpenAI API Key needed for local setup.");
    println!("------------------------------------------");
    println!("Try commands like 'note test', 'get test note', or 'history' for demonstration.");


    // Create an AIAgent instance using the default configuration.
    // This will automatically use AIApi::default(), which in turn uses OpenAIApi::default()
    // pointing to http://localhost:11434/v1/chat/completions.
    // デフォルト設定を使用してAIAgentインスタンスを作成します。
    // これにより、AIApi::default()が自動的に使用され、
    // それはOpenAIApi::default()を使用してhttp://localhost:11434/v1/chat/completionsを指します。
    let mut agent = AIAgent::default();

    // You can inspect the default configuration (optional)
    // デフォルト設定を検査できます（オプション）
    if let Some(main_prompt) = agent.get_main_system_prompt() {
        println!("\nAgent's Initial System Prompt: {}", main_prompt);
    }
    // Note: To access the model name, you'd typically need to downcast or add a getter in AIApi
    // println!("Agent's API Model: {}", agent.api.config.get("model").unwrap_or(&"unknown".to_string()));


    loop {
        print!("\nUser Input: ");
        // Ensure the prompt is displayed before reading input.
        // 入力を読み込む前にプロンプトが表示されるようにします。
        std::io::stdout().flush()?;
        let mut user_input = String::new();

        // Read a line from standard input. Handle potential errors with '?'.
        // 標準入力から1行読み込みます。潜在的なエラーは '?' で処理します。
        io::stdin()
            .read_line(&mut user_input)?;

        // Trim whitespace from the input.
        // 入力から空白をトリムします。
        let trimmed_input = user_input.trim();

        // Check for the exit command.
        // 終了コマンドをチェックします。
        if trimmed_input.eq_ignore_ascii_case("/exit") {
            println!("Exiting AI Agent. Goodbye!");
            break; // Exit the loop. ループを終了します。
        }

        // Add user message to agent's chat history.
        // ユーザーメッセージをエージェントのチャット履歴に追加します。
        agent.add_message(Character::User, Character::Agent, trimmed_input);

        // Simulate internal prompt generation (though the real prompt goes to AI API).
        // 内部プロンプト生成をシミュレートします（実際のプロンプトはAI APIに送信されます）。
        println!("\nAI Agent's Internal Prompt (input to AI):");
        if let Some(main_prompt) = agent.get_main_system_prompt() {
            println!("System: {}", main_prompt);
        }
        println!("User: {}", trimmed_input);

        // Send the user's prompt to the actual AI API via the agent's method.
        // ユーザーのプロンプトをエージェントのメソッドを介して実際のAI APIに送信します。
        println!("\nSending to AI (via {} model at {})...",
            agent.api.config.get("model").unwrap_or(&"unknown".to_string()),
            match &agent.api.client {
                ApiClient::OpenAI(o) => o.base_url.clone(), // Access base_url through specific client type
            }
        );
        match agent.send_prompt_to_ai(trimmed_input).await {
            Ok(ai_response_text) => {
                // If successful, add AI's response to chat history.
                // 成功した場合、AIの応答をチャット履歴に追加します。
                agent.add_message(Character::Agent, Character::User, &ai_response_text);
                println!("AI Response: {}", ai_response_text);
            }
            Err(e) => {
                // If there's an error, print it and add an error message to chat history.
                // エラーが発生した場合、それを表示し、チャット履歴にエラーメッセージを追加します。
                eprintln!("Error communicating with AI: {}", e);
                agent.add_message(Character::Agent, Character::User, &format!("Error: {}", e));
            }
        }

        // Example usage of notes and chat history (for demonstration purposes).
        // ノートとチャット履歴の使用例（デモンストレーション目的）。
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
    Ok(()) // Return Ok(()) on successful execution. 正常終了時にOk(())を返します。
}
