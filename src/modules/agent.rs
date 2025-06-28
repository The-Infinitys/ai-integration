// src/modules/agent.rs
pub mod api;
use api::AIApi;
use chrono::{self, Utc}; // Import Utc for current time
use std::collections::HashMap;

// Using a type alias for Vec<String> as originally defined.
// The 'note' field will be a Vec of (NoteTag, Note) tuples, as Vec<String> cannot be a HashMap key directly.
// 元々定義されていたVec<String>の型エイリアスを使用。
// Vec<String>は直接HashMapのキーにできないため、「note」フィールドは(NoteTag, Note)タプルのVecとなる。
type NoteTag = Vec<String>;

/// Represents the AI Agent itself.
/// AIエージェント自身を表します。
pub struct AIAgent {
    /// System prompt. Stores data for the AI to understand its role and functions.
    /// システムプロンプト。AIが自分の役割や自分の機能を理解するためのデータを入れておく。
    /// Using a HashMap for flexibility, e.g., "main_prompt", "persona", etc.
    /// 柔軟性のためにHashMapを使用（例: "main_prompt"、"persona"など）。
    pub system: HashMap<String, String>,
    /// Chat history.
    /// チャット履歴
    pub chat: Vec<Message>,
    /// Information about the API used to send data to the AI.
    /// AIに対してデータを送信したりするために使用するAPIの情報を保存する
    pub api: AIApi,
    /// AI uses this to write down notes. Stored as a vector of (tags, note) tuples.
    /// AIがメモを書き記すのに使う。 (タグ, ノート) のタプルベクターとして保存される。
    pub note: Vec<(NoteTag, Note)>,
}

/// Represents a single note written by the AI.
/// AIによって書き記された単一のメモを表します。
#[derive(Debug, Clone)] // Add Debug and Clone for easier use
pub struct Note {
    pub title: String, // Represents the title of the note. ノートのタイトルを表す
    pub data: String,  // Represents the content of the note. ノートの内容を表す
}

impl Note {
    /// Creates a new `Note` instance.
    /// 新しい `Note` インスタンスを作成します。
    pub fn new(title: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            data: data.into(),
        }
    }
}

/// Represents a message in the chat history.
/// チャット履歴におけるメッセージを表します。
#[derive(Debug, Clone)] // Add Debug and Clone for easier use
pub struct Message {
    pub from: Character,                     // Who the message is from. 誰からのメッセージか
    pub to: Character,                       // Who the message is directed to. 誰に向けたメッセージか
    pub date: chrono::DateTime<chrono::Utc>, // When the message was sent. いつメッセージが送信されたかを表す
    pub text: String,                        // Content of the message. メッセージの内容
}

impl Message {
    /// Creates a new `Message` instance.
    /// 新しい `Message` インスタンスを作成します。
    pub fn new(from: Character, to: Character, text: impl Into<String>) -> Self {
        Self {
            from,
            to,
            date: Utc::now(), // Automatically set current UTC time. 現在のUTC時刻を自動設定
            text: text.into(),
        }
    }
}

/// Represents the character (sender/receiver) of a message.
/// メッセージの登場人物（送信者/受信者）を表します。
#[derive(Debug, PartialEq, Eq, Clone, Copy)] // Add traits for comparison and copying
pub enum Character {
    Agent, // Represents the AI itself. AI自身を表す
    User,  // Represents the user using the AI. AIを使用しているユーザーを表す
    Cmd,   // Represents output from a command line used by the AI. AIが使用するコマンドラインからの出力であることを表す
}

impl AIAgent {
    /// Creates a new `AIAgent` instance.
    /// 新しい `AIAgent` インスタンスを作成します。
    pub fn new(api: AIApi, initial_system_prompt: impl Into<String>) -> Self {
        let mut system_map = HashMap::new();
        // Insert the initial system prompt under a well-known key.
        // 既知のキーで初期システムプロンプトを挿入。
        system_map.insert("main_system_prompt".to_string(), initial_system_prompt.into());

        AIAgent {
            system: system_map,
            chat: Vec::new(),
            api,
            note: Vec::new(), // Initialize as an empty vector of (tags, note) tuples.
        }
    }

    /// Adds a message to the agent's chat history.
    /// エージェントのチャット履歴にメッセージを追加します。
    pub fn add_message(&mut self, from: Character, to: Character, text: impl Into<String>) {
        let message = Message::new(from, to, text);
        self.chat.push(message);
    }

    /// Adds a new note with associated tags to the agent's memory.
    /// エージェントのメモリに、関連付けられたタグを持つ新しいメモを追加します。
    pub fn add_note(&mut self, tags: NoteTag, title: impl Into<String>, data: impl Into<String>) {
        let note = Note::new(title, data);
        self.note.push((tags, note));
    }

    /// Retrieves notes that contain any of the specified tags.
    /// 指定されたタグのいずれかを含むメモを取得します。
    pub fn get_notes_by_tags(&self, search_tags: &[String]) -> Vec<&Note> {
        let mut found_notes = Vec::new();
        for (note_tags, note) in &self.note {
            // Check if any search tag is present in the note's tags
            // 検索タグのいずれかがノートのタグに含まれているかチェック
            if search_tags.iter().any(|st| note_tags.contains(st)) {
                found_notes.push(note);
            }
        }
        found_notes
    }

    /// Retrieves the current chat history.
    /// 現在のチャット履歴を取得します。
    pub fn get_chat_history(&self) -> &Vec<Message> {
        &self.chat
    }

    /// Retrieves the main system prompt.
    /// メインシステムプロンプトを取得します。
    pub fn get_main_system_prompt(&self) -> Option<&String> {
        self.system.get("main_system_prompt")
    }

    /// Updates the main system prompt.
    /// メインシステムプロンプトを更新します。
    pub fn update_main_system_prompt(&mut self, new_prompt: impl Into<String>) {
        self.system.insert("main_system_prompt".to_string(), new_prompt.into());
    }

    // Placeholder for a method that would send a prompt to the AI API.
    // This method would typically involve `AIApi`.
    // AI APIにプロンプトを送信するメソッドのプレースホルダー。
    // このメソッドは通常、`AIApi`を使用します。
    // pub async fn send_prompt(&self, prompt: &str) -> Result<String, String> {
    //     // self.api.send_request(prompt).await
    //     Err("Not yet implemented: AI API integration".to_string())
    // }
}
