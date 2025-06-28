// src/modules/agent/api/openai.rs
use reqwest::{Client, Error as ReqwestError};
use serde::{Deserialize, Serialize};
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::process::Command; // Use tokio's async Command for non-blocking execution
use std::str; // For converting command output to string

use super::AiService; // Import the AiService trait from the parent module

/// Configuration and client for interacting with the OpenAI API.
/// OpenAI API とやり取りするための設定とクライアント。
#[derive(Debug)] // Add Debug trait for easier inspection
pub struct OpenAIApi {
    pub client: Client, // HTTP client for making requests. リクエストを行うためのHTTPクライアント。
    pub api_key: String, // OpenAI API key. OpenAI APIキー。
    pub base_url: String, // Base URL for OpenAI chat completions endpoint. OpenAIチャット補完エンドポイントのベースURL。
    pub model: String, // The specific OpenAI model to use (e.g., "gpt-3.5-turbo"). 使用する特定のOpenAIモデル（例： "gpt-3.5-turbo"）。
}

impl OpenAIApi {
    /// Creates a new `OpenAIApi` instance.
    /// 新しい `OpenAIApi` インスタンスを作成します。
    ///
    /// # Arguments
    /// * `api_key` - Your OpenAI API key. Empty string means no Authorization header will be sent.
    ///             あなたのOpenAI APIキー。空文字列の場合、Authorizationヘッダーは送信されません。
    /// * `model` - The name of the OpenAI model to use (e.g., "gpt-3.5-turbo", "gpt-4"). 使用する特定のOpenAIモデル（例： "gpt-3.5-turbo"、 "gpt-4"）。
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: "https://api.openai.com/v1/chat/completions".to_string(), // Standard endpoint
            model: model.into(),
        }
    }

    /// Attempts to create an `OpenAIApi` instance by detecting available Ollama models.
    /// Ollamaで利用可能なモデルを検出して `OpenAIApi` インスタンスを作成しようとします。
    /// If `ollama list` command is successful and models are found, the first model listed
    /// will be used. Otherwise, it falls back to the standard `OpenAIApi::default()`.
    /// `ollama list` コマンドが成功しモデルが検出された場合、リストの最初のモデルが使用されます。
    /// それ以外の場合は、標準の `OpenAIApi::default()` にフォールバックします。
    pub async fn new_from_ollama_list() -> Self {
        println!("[INFO] Attempting to detect Ollama models...");
        let output = Command::new("ollama")
            .arg("list")
            .output()
            .await;

        match output {
            Ok(output) => {
                if output.status.success() {
                    let stdout = str::from_utf8(&output.stdout).unwrap_or("");
                    let mut lines = stdout.lines().skip(1); // Skip header line (NAME ID SIZE MODIFIED)
                    if let Some(first_model_line) = lines.next() {
                        // Example line: "gemma3:latest    a2af6cc3eb7f    3.3 GB    43 hours ago"
                        // Split by whitespace and take the first part which is the model name.
                        if let Some(model_name) = first_model_line.split_whitespace().next() {
                            println!("[INFO] Detected Ollama model: '{}'. Using it for default.", model_name);
                            return Self::new(
                                "".to_string(), // No API key needed for local Ollama
                                model_name.to_string(),
                            );
                        }
                    }
                    eprintln!("[WARN] 'ollama list' executed successfully but no models found or parsing failed. Falling back to default 'llama2'.");
                    Self::default()
                } else {
                    eprintln!("[WARN] 'ollama list' command failed with status: {}. Stderr: {}. Falling back to default 'llama2'.",
                               output.status, str::from_utf8(&output.stderr).unwrap_or(""));
                    Self::default()
                }
            }
            Err(e) => {
                eprintln!("[WARN] Could not execute 'ollama' command (is Ollama installed and in PATH?): {}. Falling back to default 'llama2'.", e);
                Self::default()
            }
        }
    }
}

/// Provides a default `OpenAIApi` configuration suitable for local OpenAI-compatible endpoints (e.g., Ollama).
/// ローカルのOpenAI互換エンドポイント（例: Ollama）に適したデフォルトの `OpenAIApi` 設定を提供します。
impl Default for OpenAIApi {
    fn default() -> Self {
        Self {
            client: Client::new(),
            api_key: "".to_string(), // Default to an empty API key (no header sent)
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

        // Create the request builder.
        // リクエストビルダーを作成。
        let mut request_builder = self.client.post(&self.base_url);

        // ONLY add Authorization header if api_key is not empty.
        // api_keyが空でない場合にのみAuthorizationヘッダーを追加。
        if !self.api_key.is_empty() {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", self.api_key));
        }

        // Send the HTTP POST request to OpenAI.
        // OpenAI に HTTP POST リクエストを送信。
        let response = request_builder
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
