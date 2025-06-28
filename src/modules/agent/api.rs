// src/modules/agent/api.rs
pub mod openai;
use std::collections::HashMap; // Removed unused HashMap as it's not directly used in the module's top level now
use async_trait::async_trait;
use futures::stream::BoxStream;

/// Defines the interface for AI services.
/// AIサービスのためのインターフェースを定義します。
/// This trait allows `AIAgent` to interact with different AI models polymorphically.
#[async_trait]
pub trait AiService {
    /// Sends a vector of messages (e.g., system, user) to the AI and returns a stream of its response text chunks.
    /// (システム、ユーザーなどの) メッセージのベクターをAIに送信し、AIの応答テキストチャンクのストリームを返します。
    /// Each item in the stream is a `Result<String, String>`, where `Ok(String)` is a text chunk
    /// and `Err(String)` indicates an error during streaming.
    /// ストリームの各アイテムは `Result<String, String>` で、`Ok(String)` はテキストチャンク、
    /// `Err(String)` はストリーミング中のエラーを示します。
    async fn send_messages(&self, messages: Vec<serde_json::Value>) -> Result<BoxStream<'static, Result<String, String>>, String>;
}

/// An enum to hold different concrete API client implementations.
/// 異なる具体的なAPIクライアント実装を保持するためのEnumです。
pub enum ApiClient {
    OpenAI(openai::OpenAIApi),
    // Add other API clients here as needed.
}

impl ApiClient {
    /// Returns a reference to the inner `AiService` trait object.
    /// 内部の `AiService` トレイトオブジェクトへの参照を返します。
    pub fn as_ai_service(&self) -> &dyn AiService {
        match self {
            ApiClient::OpenAI(client) => client,
        }
    }
}

/// Manages the chosen API client and its specific configuration.
/// 選択されたAPIクライアントとその特定の設定を管理します。
pub struct AIApi {
    pub client: ApiClient,
    pub config: HashMap<String, String>, // HashMap is used here, so keep this import
}

impl AIApi {
    /// Creates a new `AIApi` instance with the specified API client.
    /// 指定されたAPIクライアントで新しい `AIApi` インスタンスを作成します。
    pub fn new(client: ApiClient) -> Self {
        Self {
            client,
            config: HashMap::new(),
        }
    }

    /// Adds configuration information (e.g., model name, specific endpoint) to the `AIApi` instance.
    /// 設定情報（例：モデル名、特定のエンドポイント）を`AIApi`インスタンスに追加します。
    pub fn add_config(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.config.insert(key.into(), value.into());
    }
}

/// Provides a default `AIApi` configuration using the default OpenAI client.
/// デフォルトのOpenAIクライアントを使用するデフォルトの `AIApi` 設定を提供します。
impl Default for AIApi {
    fn default() -> Self {
        let mut config = HashMap::new();
        config.insert("model".to_string(), "llama2".to_string());
        config.insert("base_url".to_string(), "http://localhost:11434/v1/chat/completions".to_string());
        Self {
            client: ApiClient::OpenAI(openai::OpenAIApi::default()),
            config,
        }
    }
}

pub enum ApiType {
    OpenAI,
}
