use crate::modules::agent::api::{AIProvider, ChatMessage, ChatRole};
use crate::modules::chat::ChatSession;
use anyhow::Result;
use colored::*;
use futures_util::stream::StreamExt;
use rustyline::error::ReadlineError;
use rustyline::Editor;
use std::io::{self, Write};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::{SyntaxSet};
use syntect::util::{LinesWithEndings, as_24_bit_terminal_escaped};

pub async fn run_cli(provider: AIProvider, base_url: String, default_model: String) -> Result<()> {
    let mut chat_session = ChatSession::new(provider, base_url, default_model.clone());

    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme = ThemeSet::load_defaults().themes["base16-ocean.dark"].clone();

    // Display initial messages
    let initial_messages = chat_session.get_messages().await;
    for message in initial_messages {
        if message.role == ChatRole::System && message.content.contains("TOOLS_YAML_SCHEMA") {
            // Skip the main system prompt
            continue;
        }
        print_message(&message, &syntax_set, &theme);
    }

    let mut rl = Editor::<(), _>::new()?;
    if rl.load_history("history.txt").is_err() {
        println!("No previous history.");
    }

    loop {
        let readline = rl.readline("\n> ");
        let input = match readline {
            Ok(line) => {
                rl.add_history_entry(line.as_str());
                line
            }
            Err(ReadlineError::Interrupted) => {
                println!("Ctrl-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("Ctrl-D");
                break;
            }
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            }
        };

        let input = input.trim().to_string();

        if input.is_empty() {
            continue;
        }

        if input.starts_with('/') {
            handle_command(&mut chat_session, &input).await?;
        } else {
            chat_session.add_user_message(input.clone()).await;
            let mut stream = chat_session.start_realtime_chat().await?;

            let mut full_ai_response = String::new();
            let mut full_tool_output = String::new();

            while let Some(event_result) = stream.next().await {
                match event_result {
                    Ok(event) => {
                        match event {
                            crate::modules::agent::AgentEvent::AiResponseChunk(chunk) => {
                                full_ai_response.push_str(&chunk);
                                print!("{}", chunk);
                                io::stdout().flush()?;
                            }
                            crate::modules::agent::AgentEvent::ToolCallDetected(tool_call) => {
                                println!("\n--- Tool Call: {} ---", tool_call.tool_name.cyan().bold());
                                println!("{}", serde_yaml::to_string(&tool_call.parameters).unwrap_or_default().yellow());
                                full_tool_output.push_str(&format!("\n--- Tool Call: {} ---\n{}", tool_call.tool_name, serde_yaml::to_string(&tool_call.parameters).unwrap_or_default()));
                            }
                            crate::modules::agent::AgentEvent::ToolExecuting(name) => {
                                println!("Executing: {}...", name.green());
                            }
                            crate::modules::agent::AgentEvent::ToolResult(tool_name, result) => {
                                println!("\n--- Tool Result ({}) ---", tool_name.cyan().bold());
                                println!("{}", serde_yaml::to_string(&result).unwrap_or_default().yellow());
                                full_tool_output.push_str(&format!("\n--- Tool Result ({}) ---\n{}", tool_name, serde_yaml::to_string(&result).unwrap_or_default()));
                            }
                            crate::modules::agent::AgentEvent::ToolError(tool_name, error_message) => {
                                eprintln!("\n--- Tool Error ({}) ---", tool_name.red().bold());
                                eprintln!("Error: {}", error_message.red());
                                full_tool_output.push_str(&format!("\n--- Tool Error ({}) ---\nError: {}", tool_name, error_message));
                            }
                            crate::modules::agent::AgentEvent::Thinking(msg) => {
                                println!("Thinking: {}", msg.blue());
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        eprintln!("Error during stream: {}", e.to_string().red());
                        break;
                    }
                }
            }
            println!(); // Newline after AI response
        }
    }
    rl.save_history("history.txt")?;
    Ok(())
}

async fn handle_command(chat_session: &mut ChatSession, command: &str) -> Result<()> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    let command_name = parts.first().unwrap_or(&"");

    match *command_name {
        "/exit" | "/quit" => {
            println!("Exiting.");
            std::process::exit(0);
        }
        "/model" => {
            if let Some(model_name) = parts.get(1) {
                chat_session.set_model(model_name.to_string()).await?;
                println!("Model set to: {}", model_name.green());
            } else {
                println!("{}", "Usage: /model <model_name>".yellow());
            }
        }
        "/list" if parts.get(1) == Some(&"models") => {
            match chat_session.list_models().await {
                Ok(models) => {
                    println!("{}", "Available Models:".cyan().bold());
                    if let Some(model_list) = models["models"].as_array() {
                        for model in model_list {
                            if let Some(name) = model["name"].as_str() {
                                println!("- {}", name.blue());
                            }
                        }
                    } else {
                        println!("{}", "No models found or unexpected response format.".yellow());
                    }
                }
                Err(e) => {
                    eprintln!("Error listing models: {}", e.to_string().red());
                }
            }
        }
        "/revert" => {
            chat_session.revert_last_turn().await;
            println!("{}", "Last turn reverted.".green());
        }
        "/clear" => {
            chat_session.clear_history().await;
            println!("{}", "Chat history cleared.".green());
        }
        "/log" => {
            let log_path = chat_session.get_log_path().await;
            let message = match log_path {
                Some(path) => format!("Log file is at: {}", path.green()),
                None => "Logging is not configured.".to_string().yellow().to_string(),
            };
            println!("{}", message);
        }
        "/help" => {
            println!("{}", "Available commands:".cyan().bold());
            println!("- /help: Show this help message");
            println!("- /shell <command>: Execute a shell command via the AI");
            println!("- /model <model_name>: Switch AI model");
            println!("- /list models: List available models");
            println!("- /revert: Undo your last message and the AI's response");
            println!("- /clear: Clear the chat history");
            println!("- /log: Show the path to the current log file");
            println!("- /exit or /quit: Exit the application");
        }
        _ => {
            println!("Unknown command: {}", command_name.red());
        }
    }
    Ok(())
}

fn print_message(message: &ChatMessage, syntax_set: &SyntaxSet, theme: &Theme) {
    let mut in_code_block = false;
    let mut code_block_lang = "txt";

    let role_prefix = match message.role {
        ChatRole::User => "You: ".yellow().bold(),
        ChatRole::Assistant => "AI: ".green().bold(),
        ChatRole::System => "System: ".cyan().bold(),
        ChatRole::Tool => "Tool: ".blue().bold(),
    };

    for line_str in LinesWithEndings::from(&message.content) {
        if line_str.trim().starts_with("```") {
            in_code_block = !in_code_block;
            if in_code_block {
                let lang_specifier = line_str.trim().trim_start_matches("```").trim();
                if !lang_specifier.is_empty() {
                    code_block_lang = lang_specifier;
                } else {
                    code_block_lang = "txt";
                }
            }
            println!("{}", line_str.trim_end()); // Print code block markers as-is
            continue;
        }

        if in_code_block {
            let syntax = syntax_set
                .find_syntax_by_token(code_block_lang)
                .unwrap_or_else(|| syntax_set.find_syntax_by_extension("txt").unwrap());
            let mut highlighter = HighlightLines::new(syntax, theme);
            let highlighted_line = match highlighter.highlight_line(line_str.trim_end(), syntax_set) {
                Ok(regions) => as_24_bit_terminal_escaped(&regions[..], false),
                Err(_) => line_str.to_string(), // Fallback to raw line if highlighting fails
            };
            println!("{}", highlighted_line);
        } else {
            // For non-code blocks, apply role prefix and color
            if line_str.trim().is_empty() {
                println!(); // Preserve empty lines
            } else {
                println!("{}{}", role_prefix, line_str.trim_end());
            }
        }
    }
}
