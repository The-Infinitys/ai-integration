// src/modules/agent.rs
pub mod api;
// Removed unused ApiClient and AiService from this import as they are not directly used in AIAgent's public API
use api::AIApi;
use chrono::{self, Utc};
use std::collections::HashMap;
use futures::stream::BoxStream;

type NoteTag = Vec<String>;

/// Represents the AI Agent itself.
/// AIエージェント自身を表します。
pub struct AIAgent {
    pub system: HashMap<String, String>,
    pub chat: Vec<Message>,
    pub api: AIApi,
    pub note: Vec<(NoteTag, Note)>,
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
    pub fn new(api: AIApi, initial_system_prompt: impl Into<String>) -> Self {
        let mut system_map = HashMap::new();
        system_map.insert("main_system_prompt".to_string(), initial_system_prompt.into());

        AIAgent {
            system: system_map,
            chat: Vec::new(),
            api,
            note: Vec::new(),
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

    /// Sends a user prompt to the configured AI API and returns a stream of the AI's response chunks.
    /// 設定されたAI APIにユーザープロンプトを送信し、AIの応答チャンクのストリームを返します。
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
        system_map.insert(
            "main_system_prompt".to_string(),
            "You are a helpful AI assistant. Respond concisely and avoid using external commands unless explicitly asked.".to_string(),
        );

        let chat_history = Vec::new();
        let note_history = Vec::new();

        Self {
            system: system_map,
            chat: chat_history,
            api: AIApi::default(),
            note: note_history,
        }
    }
}
