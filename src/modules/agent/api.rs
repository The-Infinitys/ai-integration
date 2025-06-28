// src/modules/agent/api.rs
pub mod openai; // New module for OpenAI
use std::collections::HashMap;
use async_trait::async_trait; // For async traits

/// Defines the interface for AI services.
/// AIサービスのためのインターフェースを定義します。
/// This trait allows `AIAgent` to interact with different AI models polymorphically.
/// このトレイトにより、`AIAgent` は異なるAIモデルと多態的にやり取りできます。
#[async_trait]
pub trait AiService {
    /// Sends a vector of messages (e.g., system, user) to the AI and returns the AI's response text.
    /// (システム、ユーザーなどの) メッセージのベクターをAIに送信し、AIの応答テキストを返します。
    /// Returns `Ok(response_text)` on success, `Err(error_message)` on failure.
    /// 成功時には `Ok(response_text)` を、失敗時には `Err(error_message)` を返します。
    async fn send_messages(&self, messages: Vec<serde_json::Value>) -> Result<String, String>;
}

/// An enum to hold different concrete API client implementations.
/// 異なる具体的なAPIクライアント実装を保持するためのEnumです。
/// This allows `AIApi` to manage different AI backend services.
/// これにより、`AIApi` は異なるAIバックエンドサービスを管理できます。
pub enum ApiClient {
    OpenAI(openai::OpenAIApi),
    // Add other API clients (e.g., GeminiAIApi, OllamaApi) here as needed.
    // 必要に応じて、他のAPIクライアント（例：GeminiAIApi、OllamaApi）をここに追加します。
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
    /// The actual API client instance (e.g., OpenAI, Gemini).
    /// 実際のAPIクライアントインスタンス（例：OpenAI、Gemini）
    pub client: ApiClient,
    /// Additional configuration like model names, timeouts, etc.
    /// モデル名、タイムアウトなどの追加設定
    pub config: HashMap<String, String>,
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

// The `ApiType` enum is now less directly involved in dispatch but can still be used for
// configuration or informational purposes if needed elsewhere.
// `ApiType` enumは、ディスパッチに直接関与することは少なくなりましたが、必要に応じて
// 他の場所で設定や情報目的で使用できます。
pub enum ApiType {
    OpenAI,
    // Other types could be added here for conceptual grouping.
    // 他のタイプを概念的なグループ化のためにここに追加できます。
}
