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
    println!("\n--- AI Agent Initialization ---");

    let mut agent: AIAgent;
    let aura_script_runner_instance = AuraScriptRunner::new(); // Create the AuraScriptRunner instance here

    let ollama_api_candidate = OpenAIApi::new_from_ollama_list().await;

    if ollama_api_candidate.model == "llama2" && ollama_api_candidate.base_url == "http://localhost:11434/v1/chat/completions" {
        println!("[WARN] Ollama model detection failed or no specific model found. Falling back to default AIAgent configuration.");
        // Pass the runner instance to AIAgent::default() (which is updated to accept it)
        agent = AIAgent::default();
        // Manually set the aurascript_runner for the default agent if it was re-created
        // (AIAgent::default() now creates its own, so we're consistent)
        // Note: AIAgent::default() creates its own AuraScriptRunner now, no need to re-assign here.
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
            aura_script_runner_instance, // Pass the created runner instance
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

        if trimmed_input.eq_ignore_ascii_case("/exit") {
            println!("Exiting AI Agent. Goodbye!");
            break;
        }

        if trimmed_input.starts_with('!') || trimmed_input.starts_with('/') {
            println!("\n[AuraScript] Detected AuraScript command.");
            agent.add_message(Character::User, Character::Cmd, trimmed_input);

            // Access AuraScriptRunner through the agent
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
            match agent.send_prompt_to_ai(trimmed_input).await {
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

                    agent.add_message(Character::Agent, Character::User, &full_ai_response);
                }
                Err(e) => {
                    eprintln!("Error initiating AI stream: {}", e);
                    agent.add_message(Character::Agent, Character::User, &format!("Error: {}", e));
                }
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
