// src/modules/agent.rs
pub mod api;
use api::{AIApi, ApiClient};
use chrono::{self, Utc};
use std::collections::HashMap;
use futures::stream::BoxStream;

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
    Cmd,
}

impl AIAgent {
    /// Creates a new `AIAgent` instance.
    /// 新しい `AIAgent` インスタンスを作成します。
    ///
    /// # Arguments
    /// * `api` - The API configuration for the agent to use.
    /// * `initial_system_prompt` - The initial system prompt to set for the agent.
    /// * `aurascript_runner` - The AuraScript runner instance for executing commands.
    /// * `can_execute_aurascript` - Whether the agent is allowed to execute AuraScript commands autonomously.
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
            can_execute_aurascript, // Initialize the flag
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

    /// Sends a user prompt to the configured AI API and returns a stream of the AI's response chunks.
    pub async fn send_prompt_to_ai(&self, user_prompt: &str) -> Result<BoxStream<'static, Result<String, String>>, String> {
        let mut messages: Vec<serde_json::Value> = Vec::new();

        if let Some(system_msg) = self.get_main_system_prompt() {
            messages.push(serde_json::json!({
                "role": "system",
                "content": system_msg
            }));
        }

        messages.push(serde_json::json!({
            "role": "user",
            "content": user_prompt
        }));

        self.api.client.as_ai_service().send_messages(messages).await
    }
}

impl Default for AIAgent {
    fn default() -> Self {
        let mut system_map = HashMap::new();
        // Updated system prompt to teach AI about AuraScript.
        // AuraScriptについてAIに教えるためにシステムプロンプトを更新。
        system_map.insert(
            "main_system_prompt".to_string(),
            r#"あなたはAIアシスタントです。ユーザーの質問に簡潔に答えます。
必要に応じて、AuraScriptコマンドを使って外部ツールと対話できます。
AuraScriptコマンドは、`!コマンド` または `/コマンド` の形式で出力してください。
例えば、現在のディレクトリの内容を知りたい場合は `!ls -l` と出力できます。
ウェブ検索が必要な場合は `/web_search [検索クエリ]` と出力できます。
コマンドを実行する際は、応答全体をコマンドのみにしてください。
コマンドを実行した後、その出力が与えられ、それに基づいて思考し、最終的な回答を生成してください。
もしコマンドを実行する必要がない場合は、直接ユーザーに返信してください。
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
