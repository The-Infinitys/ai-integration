// src/main.rs

use ai_integration::modules::prompt::Prompt;
use ai_integration::modules::agent::{AIAgent, api::{AIApi, ApiClient, openai::OpenAIApi}, Character};
use ai_integration::modules::aurascript::AuraScriptRunner;
use std::io::{self, Write};
use tokio;
use futures::stream::StreamExt;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    println!("Hello from AI Agent project!");
    println!("Type your message and press Enter. Type '/exit' to quit.");
    println!("Type '/enable_aurascript_exec' to let AI execute commands, '/disable_aurascript_exec' to disable.");
    println!("\n--- AI Agent Initialization ---");

    let mut agent: AIAgent;
    let aura_script_runner_instance = AuraScriptRunner::new(); // Create the AuraScriptRunner instance here

    let ollama_api_candidate = OpenAIApi::new_from_ollama_list().await;

    // Define the new system prompt string to avoid repetition
    // 繰り返しを避けるために新しいシステムプロンプト文字列を定義
    let new_system_prompt = r#"あなたはAIアシスタントです。ユーザーの質問に簡潔に答えます。
必要に応じて、AuraScriptコマンドを使って外部ツールと対話できます。
AuraScriptコマンドは、`!コマンド` または `/コマンド` の形式で出力してください。
例えば、現在のディレクトリの内容を知りたい場合は `!ls -l` と出力できます。
ウェブ検索が必要な場合は `/web_search [検索クエリ]` と出力できます。
コマンドを実行する際は、応答全体をコマンドのみにしてください。
コマンドを実行した後、その出力が与えられ、それに基づいて思考し、最終的な回答を生成してください。
もしコマンドを実行する必要がない場合は、直接ユーザーに返信してください。
"#;


    if ollama_api_candidate.model == "llama2" && ollama_api_candidate.base_url == "http://localhost:11434/v1/chat/completions" {
        println!("[WARN] Ollama model detection failed or no specific model found. Falling back to default AIAgent configuration.");
        agent = AIAgent::default(); // This will use the new system prompt from AIAgent::default()
        println!("  Using API Base URL: {}", agent.api.config.get("base_url").unwrap_or(&"unknown".to_string()));
        println!("  Using Model: {}", agent.api.config.get("model").unwrap_or(&"unknown".to_string()));
    } else {
        println!("[INFO] Initializing with detected Ollama configuration.");
        let mut api = AIApi::new(ApiClient::OpenAI(ollama_api_candidate));
        api.add_config("model".to_string(), match &api.client { ApiClient::OpenAI(o) => o.model.clone() });
        api.add_config("base_url".to_string(), match &api.client { ApiClient::OpenAI(o) => o.base_url.clone() });
        agent = AIAgent::new(
            api,
            new_system_prompt.to_string(), // Use the new system prompt here
            aura_script_runner_instance,
            false, // Default: AI cannot execute commands autonomously
        );
        println!("  Using API Base URL: {}", agent.api.config.get("base_url").unwrap_or(&"unknown".to_string()));
        println!("  Using Model: {}", agent.api.config.get("model").unwrap_or(&"unknown".to_string()));
    }

    println!("-------------------------------");
    println!("Try commands like 'note test', 'get test note', 'history', '!ls -l', or '/echo Hello AuraScript!' for demonstration.");


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

        // Handle user control commands for AuraScript execution
        if trimmed_input.eq_ignore_ascii_case("/enable_aurascript_exec") {
            agent.set_can_execute_aurascript(true);
            println!("[CONFIG] AI is now allowed to execute AuraScript commands.");
            continue;
        } else if trimmed_input.eq_ignore_ascii_case("/disable_aurascript_exec") {
            agent.set_can_execute_aurascript(false);
            println!("[CONFIG] AI is now NOT allowed to execute AuraScript commands.");
            continue;
        } else if trimmed_input.eq_ignore_ascii_case("/exit") {
            println!("Exiting AI Agent. Goodbye!");
            break;
        }

        // Check if the input is a direct AuraScript command from the user
        if trimmed_input.starts_with('!') || trimmed_input.starts_with('/') {
            println!("\n[AuraScript] Detected direct user AuraScript command.");
            agent.add_message(Character::User, Character::Cmd, trimmed_input);

            match agent.aurascript_runner.run_script(trimmed_input).await {
                Ok(output) => {
                    println!("[AuraScript Output]:\n{}", output);
                    agent.add_message(Character::Cmd, Character::Agent, &output);
                }
                Err(e) => {
                    eprintln!("[AuraScript Error]: {}", e);
                    agent.add_message(Character::Cmd, Character::Agent, &format!("Error: {}", e));
                }
            }
        } else {
            // Not a direct AuraScript command, send to AI
            agent.add_message(Character::User, Character::Agent, trimmed_input);

            println!("\nAI Agent's Internal Prompt (input to AI):");
            if let Some(main_prompt) = agent.get_main_system_prompt() {
                println!("System: {}", main_prompt);
            }
            println!("User: {}", trimmed_input);

            println!("\nSending to AI (via {} model at {})...",
                agent.api.config.get("model").unwrap_or(&"unknown".to_string()),
                agent.api.config.get("base_url").unwrap_or(&"unknown".to_string()),
            );
            
            let ai_response_stream_result = agent.send_prompt_to_ai(trimmed_input).await;

            match ai_response_stream_result {
                Ok(mut stream) => {
                    let mut full_ai_response = String::new();
                    println!("AI Response (streaming):");
                    while let Some(chunk_result) = stream.next().await {
                        match chunk_result {
                            Ok(chunk) => {
                                print!("{}", chunk);
                                std::io::stdout().flush()?;
                                full_ai_response.push_str(&chunk);
                            }
                            Err(e) => {
                                eprintln!("\nError during AI streaming: {}", e);
                                full_ai_response.push_str(&format!("\nError: {}", e));
                                break;
                            }
                        }
                    }
                    println!();

                    // Check if AI's response is an AuraScript command and if execution is allowed
                    if agent.can_execute_aurascript && (full_ai_response.starts_with('!') || full_ai_response.starts_with('/')) {
                        println!("\n[AI Execution] AI generated an AuraScript command: \"{}\"", full_ai_response.trim());
                        agent.add_message(Character::Agent, Character::Cmd, &full_ai_response);

                        match agent.aurascript_runner.run_script(full_ai_response.trim()).await {
                            Ok(script_output) => {
                                println!("[AI Execution Output]:\n{}", script_output);
                                agent.add_message(Character::Cmd, Character::Agent, &script_output);
                                // TODO: In a full ReAct loop, this output would be fed back to the AI.
                            }
                            Err(e) => {
                                eprintln!("[AI Execution Error]: {}", e);
                                agent.add_message(Character::Cmd, Character::Agent, &format!("Error: {}", e));
                            }
                        }
                    } else {
                        // If not a command or execution is not allowed, just log AI's response as usual
                        agent.add_message(Character::Agent, Character::User, &full_ai_response);
                    }
                }
                Err(e) => {
                    eprintln!("Error initiating AI stream: {}", e);
                    agent.add_message(Character::Agent, Character::User, &format!("Error: {}", e));
                }
            }
        }

        // Example usage of notes and chat history (for demonstration purposes).
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
