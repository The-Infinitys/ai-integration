// src/modules/chat/api.rs

use async_trait::async_trait;
use std::error::Error;
use serde::{Deserialize, Serialize}; // JSONのシリアライズ/デシリアライズ用
use reqwest::Client; // HTTPリクエスト用

/// `AIAgentApi`トレイトは、AIエージェントとやり取りするための基本的なインターフェースを定義します。
/// 将来的に異なるAIモデルやサービス（例：OpenAI, Gemini, その他のカスタムAI）を使用する際に、
/// このトレイトを実装することで、コアロジックを変更せずにAIバックエンドを切り替えられます。
#[async_trait]
pub trait AIAgentApi {
    /// ユーザーの入力に基づいてAIからの応答を非同期に取得します。
    ///
    /// # 引数
    /// * `user_input` - ユーザーからのテキスト入力。
    ///
    /// # 戻り値
    /// `Result<String, Box<dyn Error>>` - AIからの応答文字列、またはエラー。
    async fn get_ai_response(&self, user_input: &str) -> Result<String, Box<dyn Error>>;
}

/// Ollama APIへのリクエストボディの構造体
#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    prompt: String,
    stream: bool,
}

/// Ollama APIからのレスポンスボディの構造体
#[derive(Deserialize)]
struct OllamaChatResponse {
    response: String,
    // 必要に応じて他のフィールドも追加できます (e.g., done, contextなど)
}

/// `OllamaAIAgentApi`は`AIAgentApi`トレイトのOllama実装です。
/// OllamaサーバーとHTTPで通信し、AIの応答を取得します。
pub struct OllamaAIAgentApi {
    client: Client,
    ollama_url: String,
    model_name: String,
}

impl OllamaAIAgentApi {
    /// 新しい`OllamaAIAgentApi`のインスタンスを作成します。
    ///
    /// # 引数
    /// * `ollama_url` - OllamaサーバーのURL (例: "http://localhost:11434")。
    /// * `model_name` - 使用するOllamaモデルの名前 (例: "llama2")。
    pub fn new(ollama_url: String, model_name: String) -> Self {
        OllamaAIAgentApi {
            client: Client::new(),
            ollama_url,
            model_name,
        }
    }
}

#[async_trait]
impl AIAgentApi for OllamaAIAgentApi {
    async fn get_ai_response(&self, user_input: &str) -> Result<String, Box<dyn Error>> {
        let request_body = OllamaChatRequest {
            model: self.model_name.clone(),
            prompt: user_input.to_string(),
            stream: false, // ストリーミングではなく、完全な応答を待機します
        };

        let request_url = format!("{}/api/generate", self.ollama_url);

        // Ollama APIへのPOSTリクエストを送信
        let response = self.client.post(&request_url)
            .json(&request_body)
            .send()
            .await?
            .error_for_status()?; // HTTPステータスが2xx以外の場合はエラーを返します

        // レスポンスボディをJSONとしてパース
        let ollama_response: OllamaChatResponse = response.json().await?;

        Ok(ollama_response.response)
    }
}
