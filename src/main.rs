// src/main.rs

use ai_integration::modules::prompt::Prompt;
use ai_integration::modules::agent::{AIAgent, api::{AIApi, ApiClient, openai::OpenAIApi}, Character};
use std::io::{self, Write};
use tokio;
use futures::stream::StreamExt; // For iterating over streams

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    println!("Hello from AI Agent project!");
    println!("Type your message and press Enter. Type '/exit' to quit.");
    println!("\n--- AI Agent Initialization ---");

    let mut agent: AIAgent;

    let ollama_api_candidate = OpenAIApi::new_from_ollama_list().await;

    if ollama_api_candidate.model == "llama2" && ollama_api_candidate.base_url == "http://localhost:11434/v1/chat/completions" {
        println!("[WARN] Ollama model detection failed or no specific model found. Falling back to default AIAgent configuration.");
        agent = AIAgent::default();
        println!("  Using API Base URL: {}", agent.api.config.get("base_url").unwrap_or(&"unknown".to_string()));
        println!("  Using Model: {}", agent.api.config.get("model").unwrap_or(&"unknown".to_string()));
    } else {
        println!("[INFO] Initializing with detected Ollama configuration.");
        let mut api = AIApi::new(ApiClient::OpenAI(ollama_api_candidate));
        api.add_config("model".to_string(), match &api.client { ApiClient::OpenAI(o) => o.model.clone() });
        api.add_config("base_url".to_string(), match &api.client { ApiClient::OpenAI(o) => o.base_url.clone() });
        agent = AIAgent::new(
            api,
            "You are a helpful AI assistant. Respond concisely and avoid using external commands unless explicitly asked.".to_string(),
        );
        println!("  Using API Base URL: {}", agent.api.config.get("base_url").unwrap_or(&"unknown".to_string()));
        println!("  Using Model: {}", agent.api.config.get("model").unwrap_or(&"unknown".to_string()));
    }

    println!("-------------------------------");
    println!("Try commands like 'note test', 'get test note', or 'history' for demonstration.");


    if let Some(main_prompt) = agent.get_main_system_prompt() {
        println!("\nAgent's Initial System Prompt: {}", main_prompt);
    }


    loop {
        print!("\nUser Input: ");
        std::io::stdout().flush()?;
        let mut user_input = String::new();

        io::stdin()
            .read_line(&mut user_input)?;

        let trimmed_input = user_input.trim();

        if trimmed_input.eq_ignore_ascii_case("/exit") {
            println!("Exiting AI Agent. Goodbye!");
            break;
        }

        agent.add_message(Character::User, Character::Agent, trimmed_input);

        println!("\nAI Agent's Internal Prompt (input to AI):");
        if let Some(main_prompt) = agent.get_main_system_prompt() {
            println!("System: {}", main_prompt);
        }
        println!("User: {}", trimmed_input);

        println!("\nAI Response (streaming):");
        let mut full_ai_response = String::new(); // To accumulate the full response. 完全な応答を蓄積するため。

        // Send the user's prompt to the actual AI API via the agent's method, getting a stream back.
        // ユーザーのプロンプトをエージェントのメソッドを介して実際のAI APIに送信し、ストリームを返します。
        match agent.send_prompt_to_ai(trimmed_input).await {
            Ok(mut stream) => {
                // Iterate over the stream of chunks.
                // チャンクのストリームを反復処理。
                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            print!("{}", chunk); // Print each chunk as it arrives. 各チャンクが到着するたびに出力。
                            std::io::stdout().flush()?; // Flush to ensure immediate display. 即時表示を保証するためにフラッシュ。
                            full_ai_response.push_str(&chunk); // Accumulate the full response. 完全な応答を蓄積。
                        }
                        Err(e) => {
                            eprintln!("\nError during AI streaming: {}", e);
                            full_ai_response.push_str(&format!("\nError: {}", e));
                            break; // Stop streaming on error. エラー時にストリーミングを停止。
                        }
                    }
                }
                println!(); // Add a newline after the streaming output. ストリーミング出力後に改行を追加。

                // After streaming, add the full accumulated response to chat history.
                // ストリーミング後、蓄積された完全な応答をチャット履歴に追加。
                agent.add_message(Character::Agent, Character::User, &full_ai_response);
            }
            Err(e) => {
                // If there's an error before streaming even starts.
                // ストリーミングが開始される前にエラーが発生した場合。
                eprintln!("Error initiating AI stream: {}", e);
                agent.add_message(Character::Agent, Character::User, &format!("Error: {}", e));
            }
        }

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
