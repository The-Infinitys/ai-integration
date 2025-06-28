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
                                "sk-DUMMY_KEY_FOR_LOCAL_USE", // API key not typically needed for local Ollama, but included for API compatibility
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
            api_key: "sk-DUMMY_KEY_FOR_LOCAL_USE".to_string(), // A dummy key, often not needed for local LLMs
            base_url: "http://localhost:11434/v1/chat/completions".to_string(), // Default for Ollama etc.
            model: "llama2".to_string(), // Common default model for local LLMs via Ollama
        }
    }
}

// ... (AiService impl remains the same)
#[async_trait]
impl AiService for OpenAIApi {
    async fn send_messages(&self, messages: Vec<serde_json::Value>) -> Result<String, String> {
        // ... (implementation remains the same)
        #[derive(Serialize)]
        struct ChatCompletionRequest {
            model: String,
            messages: Vec<serde_json::Value>,
        }

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

        let request_body = ChatCompletionRequest {
            model: self.model.clone(),
            messages,
        };

        let response = self.client
            .post(&self.base_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_else(|_| "No response body".to_string());
            return Err(format!("API returned an error: Status={}, Body={}", status, text));
        }

        let response_body: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response JSON: {}", e))?;

        response_body.choices.into_iter()
            .next()
            .map(|choice| choice.message.content)
            .ok_or_else(|| "No choices found in AI response".to_string())
    }
}
