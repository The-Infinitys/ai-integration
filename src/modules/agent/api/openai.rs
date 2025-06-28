// src/modules/agent/api/openai.rs
use reqwest::{Client, Error as ReqwestError};
use serde::{Deserialize, Serialize};
use async_trait::async_trait;
use std::collections::HashMap;

use super::AiService; // Import the AiService trait from the parent module

/// Configuration and client for interacting with the OpenAI API.
/// OpenAI API とやり取りするための設定とクライアント。
pub struct OpenAIApi {
    client: Client, // HTTP client for making requests. リクエストを行うためのHTTPクライアント。
    api_key: String, // OpenAI API key. OpenAI APIキー。
    pub base_url: String, // Base URL for OpenAI chat completions endpoint. OpenAIチャット補完エンドポイントのベースURL。
    pub model: String, // The specific OpenAI model to use (e.g., "gpt-3.5-turbo"). 使用する特定のOpenAIモデル（例： "gpt-3.5-turbo"）。
}

impl OpenAIApi {
    /// Creates a new `OpenAIApi` instance.
    /// 新しい `OpenAIApi` インスタンスを作成します。
    ///
    /// # Arguments
    /// * `api_key` - Your OpenAI API key. あなたのOpenAI APIキー。
    /// * `model` - The name of the OpenAI model to use (e.g., "gpt-3.5-turbo", "gpt-4"). 使用するOpenAIモデルの名前（例： "gpt-3.5-turbo"、 "gpt-4"）。
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: "https://api.openai.com/v1/chat/completions".to_string(), // Standard endpoint
            model: model.into(),
        }
    }
}

/// Provides a default `OpenAIApi` configuration suitable for local OpenAI-compatible endpoints (e.g., Ollama).
/// ローカルのOpenAI互換エンドポイント（例: Ollama）に適したデフォルトの `OpenAIApi` 設定を提供します。
impl Default for OpenAIApi {
    fn default() -> Self {
        Self {
            client: Client::new(),
            api_key: "sk-DUMMY_KEY_FOR_LOCAL_USE".to_string(), // A dummy key, often not needed for local LLMs
            base_url: "http://localhost:11434/v1/chat/completions".to_string(), // Default for Ollama etc.
            model: "llama2".to_string(), // Common default model for local LLMs via Ollama
        }
    }
}

/// Implement the `AiService` trait for `OpenAIApi` to send messages to OpenAI.
/// `OpenAIApi` 用に `AiService` トレイトを実装し、OpenAI にメッセージを送信します。
#[async_trait]
impl AiService for OpenAIApi {
    /// Sends messages to the OpenAI Chat Completions API and returns the AI's response content.
    /// OpenAI Chat Completions API にメッセージを送信し、AI の応答コンテンツを返します。
    ///
    /// The `messages` vector should be an array of JSON objects, typically in the format:
    /// `{"role": "system", "content": "..."}`
    /// `{"role": "user", "content": "..."}`
    /// `{"role": "assistant", "content": "..."}`
    /// `messages` ベクターは、通常以下の形式のJSONオブジェクトの配列である必要があります。
    /// `{"role": "system", "content": "..."}`
    /// `{"role": "user", "content": "..."}`
    /// `{"role": "assistant", "content": "..."}`
    ///
    /// # Returns
    /// * `Ok(String)`: The content of the AI's response message. AIの応答メッセージのコンテンツ。
    /// * `Err(String)`: An error message if the API call fails or the response is invalid. API呼び出しが失敗した場合、または応答が無効な場合のエラーメッセージ。
    async fn send_messages(&self, messages: Vec<serde_json::Value>) -> Result<String, String> {
        // Structs for serializing the request body.
        // リクエストボディをシリアライズするための構造体。
        #[derive(Serialize)]
        struct ChatCompletionRequest {
            model: String,
            messages: Vec<serde_json::Value>,
        }

        // Structs for deserializing the response body from OpenAI.
        // OpenAI からの応答ボディをデシリアライズするための構造体。
        #[derive(Deserialize)]
        struct ChatCompletionResponse {
            choices: Vec<Choice>,
        }

        #[derive(Deserialize)]
        struct Choice {
            message: MessageContent,
        }

        #[derive(Deserialize)]
        struct MessageContent {
            content: String,
        }

        // Build the request body.
        // リクエストボディを構築。
        let request_body = ChatCompletionRequest {
            model: self.model.clone(),
            messages,
        };

        // Send the HTTP POST request to OpenAI.
        // OpenAI に HTTP POST リクエストを送信。
        let response = self.client
            .post(&self.base_url)
            .header("Authorization", format!("Bearer {}", self.api_key)) // Add API key to Authorization header. 認証ヘッダーにAPIキーを追加。
            .json(&request_body) // Serialize request_body to JSON and set as body. request_body を JSON にシリアライズしてボディとして設定。
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?; // Handle network or request building errors. ネットワークまたはリクエスト構築エラーを処理。

        // Check if the response status is successful.
        // 応答ステータスが成功しているかチェック。
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_else(|_| "No response body".to_string());
            return Err(format!("API returned an error: Status={}, Body={}", status, text));
        }

        // Parse the successful response body.
        // 成功した応答ボディをパース。
        let response_body: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response JSON: {}", e))?; // Handle JSON parsing errors. JSONパースエラーを処理。

        // Extract the content from the first choice.
        // 最初の選択肢からコンテンツを抽出。
        response_body.choices.into_iter()
            .next() // Get the first choice. 最初の選択肢を取得。
            .map(|choice| choice.message.content) // Get the content from the message. メッセージからコンテンツを取得。
            .ok_or_else(|| "No choices found in AI response".to_string()) // Handle cases where no choices are returned. 選択肢が返されないケースを処理。
    }
}
