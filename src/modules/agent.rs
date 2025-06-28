// src/modules/agent.rs
pub mod api;
// Removed AiService and ApiClient as they are not directly used in AIAgent's module top level
use api::AIApi;
use chrono::{self, Utc};
use futures::StreamExt;
use futures::stream::BoxStream;
use std::collections::HashMap;
use std::io::Write; // Write for .flush()

use crate::modules::aurascript::AuraScriptRunner;

type NoteTag = Vec<String>;

/// Represents the AI Agent itself.
/// AIエージェント自身を表します。
pub struct AIAgent {
    pub system: HashMap<String, String>,
    pub chat: Vec<Message>,
    pub api: AIApi,
    pub note: Vec<(NoteTag, Note)>,
    pub aurascript_runner: AuraScriptRunner,
    /// A flag indicating whether the AI is allowed to execute AuraScript commands autonomously.
    /// AIがAuraScriptコマンドを自律的に実行できるかどうかを示すフラグ。
    pub can_execute_aurascript: bool,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub title: String,
    pub data: String,
}

impl Note {
    pub fn new(title: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            data: data.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub from: Character,
    pub to: Character,
    pub date: chrono::DateTime<chrono::Utc>,
    pub text: String,
}

impl Message {
    pub fn new(from: Character, to: Character, text: impl Into<String>) -> Self {
        Self {
            from,
            to,
            date: Utc::now(),
            text: text.into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Character {
    Agent,
    User,
    Cmd, // Represents output from a command line used by the AI. AIが使用するコマンドラインからの出力であることを表す
}

impl AIAgent {
    /// Creates a new `AIAgent` instance.
    /// 新しい `AIAgent` インスタンスを作成します。
    pub fn new(
        api: AIApi,
        initial_system_prompt: impl Into<String>,
        aurascript_runner: AuraScriptRunner,
        can_execute_aurascript: bool,
    ) -> Self {
        let mut system_map = HashMap::new();
        system_map.insert(
            "main_system_prompt".to_string(),
            initial_system_prompt.into(),
        );

        AIAgent {
            system: system_map,
            chat: Vec::new(),
            api,
            note: Vec::new(),
            aurascript_runner,
            can_execute_aurascript,
        }
    }

    pub fn add_message(&mut self, from: Character, to: Character, text: impl Into<String>) {
        let message = Message::new(from, to, text);
        self.chat.push(message);
    }

    pub fn add_note(&mut self, tags: NoteTag, title: impl Into<String>, data: impl Into<String>) {
        let note = Note::new(title, data);
        self.note.push((tags, note));
    }

    pub fn get_notes_by_tags(&self, search_tags: &[String]) -> Vec<&Note> {
        let mut found_notes = Vec::new();
        for (note_tags, note) in &self.note {
            if search_tags.iter().any(|st| note_tags.contains(st)) {
                found_notes.push(note);
            }
        }
        found_notes
    }

    pub fn get_chat_history(&self) -> &Vec<Message> {
        &self.chat
    }

    pub fn get_main_system_prompt(&self) -> Option<&String> {
        self.system.get("main_system_prompt")
    }

    pub fn update_main_system_prompt(&mut self, new_prompt: impl Into<String>) {
        self.system
            .insert("main_system_prompt".to_string(), new_prompt.into());
    }

    /// Sets whether the agent can execute AuraScript commands autonomously.
    /// エージェントがAuraScriptコマンドを自律的に実行できるかどうかを設定します。
    pub fn set_can_execute_aurascript(&mut self, enabled: bool) {
        self.can_execute_aurascript = enabled;
    }

    /// Sends the current chat history (and system prompt) to the AI API and returns a stream of its response chunks.
    /// 現在のチャット履歴（およびシステムプロンプト）をAI APIに送信し、AIの応答チャンクのストリームを返します。
    pub async fn get_ai_response_stream(
        &self,
    ) -> Result<BoxStream<'static, Result<String, String>>, String> {
        let mut messages: Vec<serde_json::Value> = Vec::new();

        if let Some(system_msg) = self.get_main_system_prompt() {
            messages.push(serde_json::json!({
                "role": "system",
                "content": system_msg
            }));
        }

        for msg in &self.chat {
            let role = match msg.from {
                Character::User => "user",
                Character::Agent => "assistant",
                Character::Cmd => "system", // Command output treated as system context for AI.
            };
            let content = if msg.from == Character::Cmd {
                format!("Command Output:\n{}", msg.text)
            } else {
                msg.text.clone()
            };
            messages.push(serde_json::json!({
                "role": role,
                "content": content
            }));
        }

        self.api
            .client
            .as_ai_service()
            .send_messages(messages)
            .await
    }

    /// Processes a user's input and drives the AI's "think-act-observe" loop.
    /// ユーザーの入力を処理し、AIの「思考-行動-観察」ループを駆動します。
    /// This method will interact with the AI, potentially execute commands,
    /// and continue looping until the AI provides a final (non-command) response.
    /// Returns the final AI response string.
    pub async fn process_user_input_and_react(
        &mut self,
        user_input: &str,
    ) -> Result<String, String> {
        // Add initial user input to chat history
        self.add_message(Character::User, Character::Agent, user_input);

        let mut loop_count = 0; // Prevent infinite loops in case of AI misbehavior
        const MAX_LOOP_ITERATIONS: u8 = 5; // Limit loop iterations

        // Loop for AI's turns (might include command execution and subsequent AI responses)
        loop {
            loop_count += 1;
            if loop_count > MAX_LOOP_ITERATIONS {
                let err_msg = format!(
                    "AI reached max loop iterations ({}). Terminating.",
                    MAX_LOOP_ITERATIONS
                );
                eprintln!("[AI Loop Error]: {}", err_msg);
                self.add_message(Character::Agent, Character::User, &err_msg);
                return Err(err_msg);
            }

            // Display AI's thinking process to user
            println!("\n[AI Thinking] Sending chat history to AI...");
            println!(
                "  Model: {}",
                self.api
                    .config
                    .get("model")
                    .unwrap_or(&"unknown".to_string())
            );
            println!(
                "  Base URL: {}",
                self.api
                    .config
                    .get("base_url")
                    .unwrap_or(&"unknown".to_string())
            );

            let ai_response_stream_result = self.get_ai_response_stream().await;

            let full_ai_response = match ai_response_stream_result {
                Ok(mut stream) => {
                    let mut accumulated_response = String::new();
                    print!("[AI Response] (streaming): ");
                    std::io::stdout().flush().map_err(|e| e.to_string())?;
                    while let Some(chunk_result) = stream.next().await {
                        match chunk_result {
                            Ok(chunk) => {
                                print!("{}", chunk);
                                std::io::stdout().flush().map_err(|e| e.to_string())?;
                                accumulated_response.push_str(&chunk);
                            }
                            Err(e) => {
                                eprintln!("\nError during AI streaming: {}", e);
                                accumulated_response.push_str(&format!("\nError: {}", e));
                                return Err(format!("AI streaming error: {}", e));
                            }
                        }
                    }
                    println!();
                    accumulated_response.trim().to_string()
                }
                Err(e) => {
                    eprintln!("Error initiating AI stream: {}", e);
                    self.add_message(Character::Agent, Character::User, format!("Error: {}", e));
                    return Err(format!("AI stream initiation error: {}", e));
                }
            };

            // 新しい判定方式: USER: で始まる→ユーザー応答, COMMAND: で始まる→AuraScriptコマンド実行
            let trimmed = full_ai_response.trim_start();
            // USER: または COMMAND: の位置を探す
            let user_idx = trimmed.find("USER:");
            let cmd_idx = trimmed.find("COMMAND:");
            match (user_idx, cmd_idx) {
                (Some(u), Some(c)) => {
                    if u < c {
                        // USER: が先
                        let user_content = &trimmed[u + 5..];
                        let final_response_text = user_content.trim().to_string();
                        self.add_message(Character::Agent, Character::User, &final_response_text);
                        println!("\nAI Final Response: {}", final_response_text);
                        return Ok(final_response_text);
                    } else {
                        // COMMAND: が先
                        if self.can_execute_aurascript {
                            let cmd_content = &trimmed[c + 8..];
                            let commands: Vec<&str> = cmd_content
                                .lines()
                                .map(|line| line.trim())
                                .filter(|line| !line.is_empty())
                                .collect();
                            if commands.is_empty() {
                                let error_msg = "[Tool Error]: COMMAND: ブロックが空です。コマンドを1行ずつ記述してください。";
                                eprintln!("{}", error_msg);
                                self.add_message(Character::Cmd, Character::Agent, error_msg);
                                return Err(error_msg.to_string());
                            }
                            for command_line in commands {
                                println!("[Tool Execution] Running command: \"{}\"", command_line);
                                self.add_message(Character::Agent, Character::Cmd, command_line);
                                match self.aurascript_runner.run_script(command_line).await {
                                    Ok(script_output) => {
                                        println!("[Tool Output]:\n{}", script_output);
                                        self.add_message(Character::Cmd, Character::Agent, &script_output);
                                    }
                                    Err(e) => {
                                        eprintln!("[Tool Error]: {}", e);
                                        self.add_message(
                                            Character::Cmd,
                                            Character::Agent,
                                            format!(
                                                "Error executing AI-generated command '{}': {}",
                                                command_line, e
                                            ),
                                        );
                                    }
                                }
                            }
                            continue;
                        } else {
                            let error_msg = "[Permission Error]: AuraScriptコマンドの自動実行は許可されていません。";
                            eprintln!("{}", error_msg);
                            self.add_message(Character::Agent, Character::User, error_msg);
                            return Err(error_msg.to_string());
                        }
                    }
                }
                (Some(u), None) => {
                    let user_content = &trimmed[u + 5..];
                    let final_response_text = user_content.trim().to_string();
                    self.add_message(Character::Agent, Character::User, &final_response_text);
                    println!("\nAI Final Response: {}", final_response_text);
                    return Ok(final_response_text);
                }
                (None, Some(c)) => {
                    if self.can_execute_aurascript {
                        let cmd_content = &trimmed[c + 8..];
                        let commands: Vec<&str> = cmd_content
                            .lines()
                            .map(|line| line.trim())
                            .filter(|line| !line.is_empty())
                            .collect();
                        if commands.is_empty() {
                            let error_msg = "[Tool Error]: COMMAND: ブロックが空です。コマンドを1行ずつ記述してください。";
                            eprintln!("{}", error_msg);
                            self.add_message(Character::Cmd, Character::Agent, error_msg);
                            return Err(error_msg.to_string());
                        }
                        for command_line in commands {
                            println!("[Tool Execution] Running command: \"{}\"", command_line);
                            self.add_message(Character::Agent, Character::Cmd, command_line);
                            match self.aurascript_runner.run_script(command_line).await {
                                Ok(script_output) => {
                                    println!("[Tool Output]:\n{}", script_output);
                                    self.add_message(Character::Cmd, Character::Agent, &script_output);
                                }
                                Err(e) => {
                                    eprintln!("[Tool Error]: {}", e);
                                    self.add_message(
                                        Character::Cmd,
                                        Character::Agent,
                                        format!(
                                            "Error executing AI-generated command '{}': {}",
                                            command_line, e
                                        ),
                                    );
                                }
                            }
                        }
                        continue;
                    } else {
                        let error_msg = "[Permission Error]: AuraScriptコマンドの自動実行は許可されていません。";
                        eprintln!("{}", error_msg);
                        self.add_message(Character::Agent, Character::User, error_msg);
                        return Err(error_msg.to_string());
                    }
                }
                (None, None) => {
                    let error_msg = "[AI Format Warning] AIの出力はUSER:またはCOMMAND:で始めてください。";
                    eprintln!("{}", error_msg);
                    self.add_message(Character::Agent, Character::User, error_msg);
                    return Err(error_msg.to_string());
                }
            }
        }
    }
}

impl Default for AIAgent {
    fn default() -> Self {
        let mut system_map = HashMap::new();
        system_map.insert(
            "main_system_prompt".to_string(),
            include_str!("../default-prompt.md").to_string(),
        );

        let chat_history = Vec::new();
        let note_history = Vec::new();

        let default_aurascript_runner = AuraScriptRunner::default();

        Self {
            system: system_map,
            chat: chat_history,
            api: AIApi::default(),
            note: note_history,
            aurascript_runner: default_aurascript_runner,
            can_execute_aurascript: false, // Default to not executing commands autonomously
        }
    }
}
