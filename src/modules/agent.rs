// src/modules/agent.rs
pub mod api;
// Removed AiService and ApiClient as they are not directly used in AIAgent's module top level
use api::AIApi;
use chrono::{self, Utc};
use std::collections::HashMap;
use futures::stream::BoxStream;
use futures::StreamExt;
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
        system_map.insert("main_system_prompt".to_string(), initial_system_prompt.into());

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
        self.system.insert("main_system_prompt".to_string(), new_prompt.into());
    }

    /// Sets whether the agent can execute AuraScript commands autonomously.
    /// エージェントがAuraScriptコマンドを自律的に実行できるかどうかを設定します。
    pub fn set_can_execute_aurascript(&mut self, enabled: bool) {
        self.can_execute_aurascript = enabled;
    }

    /// Sends the current chat history (and system prompt) to the AI API and returns a stream of its response chunks.
    /// 現在のチャット履歴（およびシステムプロンプト）をAI APIに送信し、AIの応答チャンクのストリームを返します。
    pub async fn get_ai_response_stream(&self) -> Result<BoxStream<'static, Result<String, String>>, String> {
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

        self.api.client.as_ai_service().send_messages(messages).await
    }

    /// Processes a user's input and drives the AI's "think-act-observe" loop.
    /// ユーザーの入力を処理し、AIの「思考-行動-観察」ループを駆動します。
    /// This method will interact with the AI, potentially execute commands,
    /// and continue looping until the AI provides a final (non-command) response.
    /// Returns the final AI response string.
    pub async fn process_user_input_and_react(&mut self, user_input: &str) -> Result<String, String> {
        // Add initial user input to chat history
        self.add_message(Character::User, Character::Agent, user_input);

        let mut loop_count = 0; // Prevent infinite loops in case of AI misbehavior
        const MAX_LOOP_ITERATIONS: u8 = 5; // Limit loop iterations

        // Loop for AI's turns (might include command execution and subsequent AI responses)
        loop {
            loop_count += 1;
            if loop_count > MAX_LOOP_ITERATIONS {
                let err_msg = format!("AI reached max loop iterations ({}). Terminating.", MAX_LOOP_ITERATIONS);
                eprintln!("[AI Loop Error]: {}", err_msg);
                self.add_message(Character::Agent, Character::User, &err_msg);
                return Err(err_msg);
            }

            // Display AI's thinking process to user
            println!("\n[AI Thinking] Sending chat history to AI...");
            println!("  Model: {}", self.api.config.get("model").unwrap_or(&"unknown".to_string()));
            println!("  Base URL: {}", self.api.config.get("base_url").unwrap_or(&"unknown".to_string()));

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

            // 2. Analyze AI's response for tagged AuraScript commands
            let mut command_blocks_executed = false;
            let mut current_pos = 0; // Fix: Initialize current_pos here.
            let start_tag = "<aurascript>";
            let end_tag = "</aurascript>";

            if self.can_execute_aurascript {
                // Check for existence of any aurascript tags
                if full_ai_response.contains(start_tag) && full_ai_response.contains(end_tag) {
                    while let Some(start_tag_idx) = full_ai_response[current_pos..].find(start_tag) {
                        let content_start = current_pos + start_tag_idx + start_tag.len();
                        if let Some(end_tag_idx) = full_ai_response[content_start..].find(end_tag) {
                            let block_end_absolute_idx = content_start + end_tag_idx; // Absolute index of end tag start
                            let aurascript_content = &full_ai_response[content_start..block_end_absolute_idx];

                            // Log any text before the aurascript block as AI's thought
                            let pre_aurascript_text = &full_ai_response[current_pos..(current_pos + start_tag_idx)];
                            if !pre_aurascript_text.trim().is_empty() {
                                println!("[AI Thought]: {}", pre_aurascript_text.trim());
                                self.add_message(Character::Agent, Character::User, pre_aurascript_text.trim());
                            }

                            println!("[AI Action] Detected AuraScript block:");
                            // No need to print the raw content, just indicate its presence
                            // self.add_message(Character::Agent, Character::Cmd, aurascript_content); // Log the block itself if needed

                            let commands: Vec<&str> = aurascript_content.lines()
                                                        .filter(|line| !line.trim().is_empty())
                                                        .collect();

                            if commands.is_empty() {
                                eprintln!("[Tool Error]: AuraScript block was empty. Continuing AI loop.");
                                self.add_message(Character::Cmd, Character::Agent, "Error: AuraScript block was empty.");
                                current_pos = block_end_absolute_idx + end_tag.len();
                                continue;
                            }

                            // Execute each command within the block
                            for command_line in commands {
                                println!("[Tool Execution] Running command: \"{}\"", command_line);
                                self.add_message(Character::Agent, Character::Cmd, command_line); // Log individual command
                                match self.aurascript_runner.run_script(command_line).await {
                                    Ok(script_output) => {
                                        println!("[Tool Output]:\n{}", script_output);
                                        self.add_message(Character::Cmd, Character::Agent, &script_output);
                                        command_blocks_executed = true;
                                    }
                                    Err(e) => {
                                        eprintln!("[Tool Error]: {}", e);
                                        self.add_message(Character::Cmd, Character::Agent, format!("Error executing AI-generated command '{}': {}", command_line, e));
                                        command_blocks_executed = true;
                                    }
                                }
                            }
                            current_pos = block_end_absolute_idx + end_tag.len();
                        } else {
                            eprintln!("[AI Action] Warning: Found '{}' tag but no closing '{}' tag. Treating remaining response as final.", start_tag, end_tag);
                            command_blocks_executed = false;
                            break; // Exit loop, treat full_ai_response (from current_pos onwards) as final
                        }
                    }
                } else {
                    // No aurascript tags found at all.
                    // If AI has any non-command text, log it as final response.
                    command_blocks_executed = false;
                }
            }


            if command_blocks_executed {
                // If any command block was executed, loop again to let AI respond to the new context.
                continue;
            } else {
                // No command blocks executed in this turn, or execution not allowed.
                // Treat the remaining AI response (if any) as a final message to the user.
                let final_ai_response_part = &full_ai_response[current_pos..].trim();

                if final_ai_response_part.starts_with("USER:") {
                    let final_response_text = final_ai_response_part["USER:".len()..].trim().to_string();
                    self.add_message(Character::Agent, Character::User, &final_response_text);
                    println!("\nAI Final Response: {}", final_response_text);
                    return Ok(final_response_text);
                } else if !final_ai_response_part.is_empty() {
                    eprintln!("[AI Format Warning] AI did not output a command block or start with 'USER:'. Treating remaining response as final response.");
                    self.add_message(Character::Agent, Character::User, *final_ai_response_part);
                    println!("\nAI Final Response: {}", final_ai_response_part);
                    return Ok(final_ai_response_part.to_string());
                } else {
                    eprintln!("[AI Loop Error]: AI did not provide a command or a final response. Terminating to prevent infinite loop.");
                    self.add_message(Character::Agent, Character::User, "Error: AI did not provide a command or a final response. Terminating.");
                    return Err("AI did not provide a command or a final response.".to_string());
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
            r#"あなたはAIアシスタントです。ユーザーの質問に簡潔に答えます。
あなたの目標は、ユーザーの要求を理解し、必要に応じてツールを利用して、最終的な回答を提供することです。
あなたは「思考 (Thought)」、「行動 (Action)」、「観察 (Observation)」のループで動作します。

**思考のステップ (Thought):**
ユーザーの要求を分析し、最も適切な次のステップを決定します。
利用可能なツール（AuraScriptコマンド）が、あなたの知識だけでは答えられない、または情報を確認する必要がある場合に役立つかどうか判断します。
ツールが必要ない場合、直接ユーザーに最終応答を生成します。

**行動のステップ (Action):**
AuraScriptコマンドを実行する場合は、**必ずコマンドをXMLタグ `<aurascript>` と `</aurascript>` で囲んでください。**
これらのタグの間に、`!` で始まるシェルコマンド（例: `!ls -l`）または `/` で始まるカスタムツール（例: `/web_search Rust programming`）を1行に1つずつ記述できます。
**重要:** `<aurascript>...</aurascript>` ブロックを生成したら、**その直後に他のテキストや思考を続けないでください。** システムがコマンドを実行し、その結果をあなたに提供するまで待機します。

現在利用可能なカスタムツール: `echo [テキスト]`, `web_search [クエリ]`

**ユーザーに直接応答する場合:**
あなたの応答はユーザーに向けられます。**応答の前に `USER: ` と明確に書いてください。** これがあなたの「行動」の最終ステップであり、推論ループを終了します。

**観察のステップ (Observation):**
あなたがコマンドブロックを出力した後、システムはその中のコマンドを順番に実行し、それぞれの出力がチャット履歴の「Command Output:」というプレフィックスを持つシステムメッセージとしてあなたに提供されます。あなたはこれを受け取り、次の思考と行動を決定します。

**例（思考-行動-観察ループのログ）:**
ユーザー: Rustの現在の安定版のバージョンは何ですか？

AIの思考: Rustの現在の安定版バージョンを知るにはウェブ検索が必要だ。
AIの行動:
<aurascript>
/web_search Rust current stable version
</aurascript>

(システムがコマンドを実行し、チャット履歴に結果を追加)
Command Output: Web search results for 'Rust current stable version': Rust 1.79.0 (stable) released on 2024-06-13.

AIの思考: ウェブ検索結果からRustの現在の安定版バージョンが分かった。これをユーザーに伝えることができる。
AIの行動: USER: Rustの現在の安定版バージョンは 1.79.0 です。

不明な点がある場合、またはコマンドの実行結果が不十分な場合は、さらにツールを実行するか、明確な質問をして情報を集めてください。
"#
            .to_string(),
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
