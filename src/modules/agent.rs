// src/modules/agent.rs
pub mod api;
use api::{AIApi, AiService, ApiClient};
use chrono::{self, Utc};
use std::collections::HashMap;

type NoteTag = Vec<String>;

/// Represents the AI Agent itself.
/// AIエージェント自身を表します。
pub struct AIAgent {
    /// System prompt. Stores data for the AI to understand its role and functions.
    /// システムプロンプト。AIが自分の役割や自分の機能を理解するためのデータを入れておく。
    /// Using a HashMap for flexibility, e.g., "main_prompt", "persona", etc.
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Character {
    Agent, // Represents the AI itself. AI自身を表す
    User,  // Represents the user using the AI. AIを使用しているユーザーを表す
    Cmd,   // Represents output from a command line used by the AI. AIが使用するコマンドラインからの出力であることを表す
}

impl AIAgent {
    /// Creates a new `AIAgent` instance.
    /// 新しい `AIAgent` インスタンスを作成します。
    ///
    /// # Arguments
    /// * `api` - The API configuration for the agent to use. エージェントが使用するAPI設定。
    /// * `initial_system_prompt` - The initial system prompt to set for the agent. エージェントに設定する初期システムプロンプト。
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
    ///
    /// # Arguments
    /// * `from` - The sender of the message. メッセージの送信者。
    /// * `to` - The recipient of the message. メッセージの受信者。
    /// * `text` - The content of the message. メッセージの内容。
    pub fn add_message(&mut self, from: Character, to: Character, text: impl Into<String>) {
        let message = Message::new(from, to, text);
        self.chat.push(message);
    }

    /// Adds a new note with associated tags to the agent's memory.
    /// エージェントのメモリに、関連付けられたタグを持つ新しいメモを追加します。
    ///
    /// # Arguments
    /// * `tags` - A vector of strings representing the tags for the note. ノートのタグを表す文字列のベクター。
    /// * `title` - The title of the note. ノートのタイトル。
    /// * `data` - The content of the note. ノートの内容。
    pub fn add_note(&mut self, tags: NoteTag, title: impl Into<String>, data: impl Into<String>) {
        let note = Note::new(title, data);
        self.note.push((tags, note));
    }

    /// Retrieves notes that contain any of the specified tags.
    /// 指定されたタグのいずれかを含むメモを取得します。
    ///
    /// # Arguments
    /// * `search_tags` - A slice of strings representing the tags to search for. 検索するタグを表す文字列のスライス。
    ///
    /// # Returns
    /// A vector of references to `Note` objects that match the search tags.
    /// 検索タグに一致する `Note` オブジェクトへの参照のベクター。
    pub fn get_notes_by_tags(&self, search_tags: &[String]) -> Vec<&Note> {
        let mut found_notes = Vec::new();
        for (note_tags, note) in &self.note {
            // Check if any search tag is present in the note's tags
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

    /// Sends a user prompt to the configured AI API and returns the AI's response.
    /// 設定されたAI APIにユーザープロンプトを送信し、AIの応答を返します。
    /// This method constructs the messages in a format suitable for the AI model,
    /// typically including the system prompt and the user's current input.
    /// このメソッドは、AIモデルに適した形式でメッセージを構築します。
    /// 通常、システムプロンプトとユーザーの現在の入力が含まれます。
    ///
    /// # Arguments
    /// * `user_prompt` - The text content of the user's current prompt. ユーザーの現在のプロンプトのテキスト内容。
    ///
    /// # Returns
    /// * `Ok(String)`: The AI's response text. AIの応答テキスト。
    /// * `Err(String)`: An error message if communication with the AI fails. AIとの通信が失敗した場合のエラーメッセージ。
    pub async fn send_prompt_to_ai(&self, user_prompt: &str) -> Result<String, String> {
        // Construct messages in OpenAI's expected format (or other AI's format).
        // OpenAIが期待する形式（または他のAIの形式）でメッセージを構築。
        let mut messages: Vec<serde_json::Value> = Vec::new();

        // Add system message if present, as the first message.
        // システムメッセージがあれば、最初のメッセージとして追加。
        if let Some(system_msg) = self.get_main_system_prompt() {
            messages.push(serde_json::json!({
                "role": "system",
                "content": system_msg
            }));
        }

        // Add the current user's prompt as a user message.
        // 現在のユーザーのプロンプトをユーザーメッセージとして追加。
        messages.push(serde_json::json!({
            "role": "user",
            "content": user_prompt
        }));

        // Use the AiService trait to send these constructed messages.
        // AiServiceトレイトを使って構築されたこれらのメッセージを送信。
        self.api.client.as_ai_service().send_messages(messages).await
    }
}

/// Provides a default `AIAgent` with basic configuration and a default API client.
/// 基本的な設定とデフォルトのAPIクライアントを持つデフォルトの `AIAgent` を提供します。
impl Default for AIAgent {
    fn default() -> Self {
        let mut system_map = HashMap::new();
        // Set a default system prompt for the agent.
        // エージェントのデフォルトシステムプロンプトを設定します。
        system_map.insert(
            "main_system_prompt".to_string(),
            "You are a helpful AI assistant. Respond concisely and avoid using external commands unless explicitly asked.".to_string(),
        );

        // Initialize chat and note history as empty.
        // チャットとノートの履歴を空で初期化します。
        // This ensures that `Character::Cmd` messages are not present by default
        // as part of the agent's initial state or history.
        // これにより、エージェントの初期状態や履歴に `Character::Cmd` メッセージが
        // デフォルトで存在しないことが保証されます。
        let chat_history = Vec::new();
        let note_history = Vec::new();

        Self {
            system: system_map,
            chat: chat_history,
            api: AIApi::default(), // Use the default AIApi (which uses OpenAI's default)
            note: note_history,
        }
    }
}
