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
    println!("\n--- AI Agent Initialization ---");

    // Attempt to create OpenAI API client by detecting Ollama models.
    // Ollamaモデルを検出してOpenAI APIクライアントを作成しようとします。
    // This will call 'ollama list' and use the first detected model,
    // or fall back to OpenAIApi::default() if detection fails.
    // これにより、「ollama list」が呼び出され、検出された最初のモデルが使用されます。
    // 検出に失敗した場合は、OpenAIApi::default() にフォールバックします。
    let openai_client = OpenAIApi::new_from_ollama_list().await;

    println!("  Using API Base URL: {}", openai_client.base_url);
    println!("  Using Model: {}", openai_client.model);
    println!("-------------------------------");
    println!("Try commands like 'note test', 'get test note', or 'history' for demonstration.");


    // Wrap the OpenAI client in ApiClient enum.
    // OpenAIクライアントをApiClient enumでラップします。
    let mut api = AIApi::new(ApiClient::OpenAI(openai_client));
    // Store the detected/default model and base_url in AIApi's config for easy access/logging.
    // 検出された/デフォルトのモデルとbase_urlをAIApiの設定に保存し、アクセス/ロギングを容易にします。
    api.add_config(
        "model".to_string(),
        match &api.client {
            ApiClient::OpenAI(o) => o.model.clone(),
        }
    );
    api.add_config(
        "base_url".to_string(),
        match &api.client {
            ApiClient::OpenAI(o) => o.base_url.clone(),
        }
    );

    // Create an AIAgent instance with the API configuration and an initial system prompt.
    // API設定と初期システムプロンプトを持つAIAgentインスタンスを作成します。
    let mut agent = AIAgent::new(
        api,
        "You are a helpful AI assistant. Respond concisely and avoid using external commands unless explicitly asked.".to_string(), // AIエージェントの役割
    );

    // Initial system prompt display (optional, moved here for clarity)
    // 初期システムプロンプトの表示（オプション、明確化のためにここに移動）
    if let Some(main_prompt) = agent.get_main_system_prompt() {
        println!("\nAgent's Initial System Prompt: {}", main_prompt);
    }


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
            agent.api.config.get("base_url").unwrap_or(&"unknown".to_string()),
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
