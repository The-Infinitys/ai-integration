// src/modules/agent/api/openai.rs
use reqwest::Client;
use serde::{Deserialize, Serialize};
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::process::Command;
use std::str;
use futures::stream::{self, BoxStream, StreamExt, TryStreamExt};
use bytes::BytesMut;

use super::AiService;

/// Configuration and client for interacting with the OpenAI API.
/// OpenAI API とやり取りするための設定とクライアント。
#[derive(Debug)]
pub struct OpenAIApi {
    pub client: Client,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl OpenAIApi {
    /// Creates a new `OpenAIApi` instance.
    /// 新しい `OpenAIApi` インスタンスを作成します。
    ///
    /// # Arguments
    /// * `api_key` - Your OpenAI API key. Empty string means no Authorization header will be sent.
    ///             あなたのOpenAI APIキー。空文字列の場合、Authorizationヘッダーは送信されません。
    /// * `model` - The name of the OpenAI model to use (e.g., "gpt-3.5-turbo", "gpt-4").
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: "https://api.openai.com/v1/chat/completions".to_string(),
            model: model.into(),
        }
    }
    pub fn local(model: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: "".into(),
            base_url: "https://localhost:11434/v1/chat/completions".to_string(),
            model: model.into(),
        }
    }

    /// Attempts to create an `OpenAIApi` instance by detecting available Ollama models.
    /// Ollamaで利用可能なモデルを検出して `OpenAIApi` インスタンスを作成しようとします。
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
                    let mut lines = stdout.lines().skip(1);
                    if let Some(first_model_line) = lines.next() {
                        if let Some(model_name) = first_model_line.split_whitespace().next() {
                            println!("[INFO] Detected Ollama model: '{}'. Using it for default.", model_name);
                            return Self::local(
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
            api_key: "".to_string(),
            base_url: "http://localhost:11434/v1/chat/completions".to_string(),
            model: "llama2".to_string(),
        }
    }
}

/// Implement the `AiService` trait for `OpenAIApi` to send messages to OpenAI.
/// `OpenAIApi` 用に `AiService` トレイトを実装し、OpenAI にメッセージを送信します。
#[async_trait]
impl AiService for OpenAIApi {
    /// Sends messages to the OpenAI Chat Completions API and returns a stream of its response text chunks.
    /// OpenAI Chat Completions API にメッセージを送信し、AI の応答テキストチャンクのストリームを返します。
    async fn send_messages(&self, messages: Vec<serde_json::Value>) -> Result<BoxStream<'static, Result<String, String>>, String> {
        // Structs for serializing the request body.
        #[derive(Serialize)]
        struct ChatCompletionRequest {
            model: String,
            messages: Vec<serde_json::Value>,
            stream: bool,
        }

        // Structs for deserializing the streaming response deltas from OpenAI.
        #[derive(Deserialize, Debug)]
        struct ChatCompletionChunk {
            choices: Vec<Choice>,
        }

        #[derive(Deserialize, Debug)]
        struct Choice {
            delta: DeltaContent,
        }

        #[derive(Deserialize, Debug)]
        struct DeltaContent {
            #[serde(default)]
            content: Option<String>,
        }

        // Build the request body, enabling streaming.
        let request_body = ChatCompletionRequest {
            model: self.model.clone(),
            messages,
            stream: true,
        };

        let mut request_builder = self.client.post(&self.base_url);

        if !self.api_key.is_empty() {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let response = request_builder
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_else(|_| "No response body".to_string());
            return Err(format!("API returned an error: Status={}, Body={}", status, text));
        }

        // Move `response.bytes_stream()` into the initial state of `try_unfold`.
        // `response.bytes_stream()` を `try_unfold` の初期状態に移動します。
        let initial_state = (BytesMut::new(), response.bytes_stream());

        // State for try_unfold: (BytesMut buffer, Stream of Bytes)
        // try_unfold の状態: (BytesMut バッファ, バイトのストリーム)
        let processed_stream = futures::stream::try_unfold(initial_state, move |(mut buffer, mut byte_stream)| async move {
            loop {
                // Try to find a complete line in the buffer
                if let Some(newline_pos) = buffer.windows(2).position(|w| w == b"\r\n") {
                    let line_bytes = buffer.split_to(newline_pos + 2); // Include \r\n
                    let line_str = str::from_utf8(&line_bytes).map_err(|_| "Invalid UTF-8 in stream".to_string())?.trim();

                    if line_str.starts_with("data: ") {
                        let json_str = &line_str[6..];
                        if json_str == "[DONE]" {
                            // Signal end of stream
                            return Ok(None); // This terminates the stream
                        } else {
                            match serde_json::from_str::<ChatCompletionChunk>(json_str) {
                                Ok(chunk) => {
                                    if let Some(content) = chunk.choices.into_iter().next().and_then(|choice| choice.delta.content) {
                                        // Return the content as Ok(String) and continue with the remaining state
                                        // コンテンツを Ok(String) として返し、残りの状態で続行
                                        return Ok(Some((content, (buffer, byte_stream)))); // Return content, and the (buffer, byte_stream) tuple as the next state
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Failed to parse JSON chunk: {} (Line: {})", e, json_str);
                                    // Propagate parsing error as the error of the stream
                                    return Err(format!("Failed to parse JSON chunk: {}", e));
                                }
                            }
                        }
                    }
                    // If it's not a data line or no content was extracted, continue loop to process more lines in buffer
                } else {
                    // No complete line in buffer, try to read more from the underlying byte_stream
                    match byte_stream.next().await {
                        Some(chunk_result) => {
                            let chunk = chunk_result.map_err(|e| format!("Error reading stream chunk: {}", e))?;
                            buffer.extend_from_slice(&chunk); // Add new bytes to buffer
                        }
                        None => {
                            // End of underlying byte stream, and no more complete lines in buffer
                            return Ok(None); // Terminate the stream
                        }
                    }
                }
            }
        })
        .map_err(|e| e.to_string()) // Convert any internal error from try_unfold into a String error for the outer Result
        .boxed();

        Ok(processed_stream)
    }
}
