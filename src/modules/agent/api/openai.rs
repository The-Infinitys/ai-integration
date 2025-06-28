// src/modules/agent/api/openai.rs
use async_trait::async_trait;
use bytes::BytesMut;
use futures::stream::{BoxStream, StreamExt, TryStreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::str;
use tokio::process::Command;

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
            base_url: "https://api.openai.com/v1/chat/completions".to_string(), // Standard endpoint (HTTPS)
            model: model.into(),
        }
    }
    /// Creates a new `OpenAIApi` instance configured for local Ollama.
    /// ローカルOllama用に設定された新しい `OpenAIApi` インスタンスを作成します。
    /// This uses a base URL of `http://localhost:11434/v1/chat/completions` and no API key.
    /// これは `http://localhost:11434/v1/chat/completions` のベースURLを使用し、APIキーは不要です。
    pub fn local(model: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: "".into(), // No API key for local Ollama
            base_url: "http://localhost:11434/v1/chat/completions".to_string(), // Corrected to HTTP
            model: model.into(),
        }
    }

    /// Attempts to create an `OpenAIApi` instance by detecting available Ollama models.
    /// Ollamaで利用可能なモデルを検出して `OpenAIApi` インスタンスを作成しようとします。
    pub async fn new_from_ollama_list() -> Self {
        println!("[INFO] Attempting to detect Ollama models...");
        let output = Command::new("ollama").arg("list").output().await;

        match output {
            Ok(output) => {
                if output.status.success() {
                    let stdout = str::from_utf8(&output.stdout).unwrap_or("");
                    let mut lines = stdout.lines().skip(1);
                    if let Some(first_model_line) = lines.next() {
                        if let Some(model_name) = first_model_line.split_whitespace().next() {
                            println!(
                                "[INFO] Detected Ollama model: '{}'. Using it for default.",
                                model_name
                            );
                            // Use Self::local with the detected model name
                            return Self::local(model_name.to_string());
                        }
                    }
                    eprintln!(
                        "[WARN] 'ollama list' executed successfully but no models found or parsing failed. Falling back to default 'llama2'."
                    );
                    Self::default()
                } else {
                    eprintln!(
                        "[WARN] 'ollama list' command failed with status: {}. Stderr: {}. Falling back to default 'llama2'.",
                        output.status,
                        str::from_utf8(&output.stderr).unwrap_or("")
                    );
                    Self::default()
                }
            }
            Err(e) => {
                eprintln!(
                    "[WARN] Could not execute 'ollama' command (is Ollama installed and in PATH?): {}. Falling back to default 'llama2'.",
                    e
                );
                Self::default()
            }
        }
    }
}

/// Provides a default `OpenAIApi` configuration suitable for local OpenAI-compatible endpoints (e.g., Ollama).
/// ローカルのOpenAI互換エンドポイント（例: Ollama）に適したデフォルトの `OpenAIApi` 設定を提供します。
impl Default for OpenAIApi {
    fn default() -> Self {
        // Now calling the `local` constructor for consistency
        Self::local("llama2".to_string())
    }
}

/// Implement the `AiService` trait for `OpenAIApi` to send messages to OpenAI.
/// `OpenAIApi` 用に `AiService` トレイトを実装し、OpenAI にメッセージを送信します。
#[async_trait]
impl AiService for OpenAIApi {
    /// Sends messages to the OpenAI Chat Completions API and returns a stream of its response text chunks.
    /// OpenAI Chat Completions API にメッセージを送信し、AI の応答テキストチャンクのストリームを返します。
    async fn send_messages(
        &self,
        messages: Vec<serde_json::Value>,
    ) -> Result<BoxStream<'static, Result<String, String>>, String> {
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
            // index: u32,
            delta: DeltaContent,
            #[serde(default)]
            finish_reason: Option<String>,
        }

        #[derive(Deserialize, Debug)]
        struct DeltaContent {
            #[serde(default)]
            content: Option<String>,
            #[serde(default)]
            #[allow(dead_code)]
            role: Option<String>,
        }

        // Build the request body, enabling streaming.
        let request_body = ChatCompletionRequest {
            model: self.model.clone(),
            messages,
            stream: true,
        };

        let mut request_builder = self.client.post(&self.base_url);

        if !self.api_key.is_empty() {
            request_builder =
                request_builder.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let response = request_builder
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "No response body".to_string());
            return Err(format!(
                "API returned an error: Status={}, Body={}",
                status, text
            ));
        }

        let byte_stream = response.bytes_stream();

        let initial_state = (BytesMut::new(), byte_stream);

        let processed_stream = futures::stream::try_unfold(
            initial_state,
            move |(mut buffer, mut byte_stream)| async move {
                loop {
                    // Debug: Current buffer content
                    // println!("[DEBUG] Current buffer: {:?}", String::from_utf8_lossy(&buffer));

                    // Try to find a complete line in the buffer (looking for `\n` as primary delimiter)
                    // バッファ内で完全な行を見つけようと試みる（主区切り文字として `\n` を探す）
                    if let Some(mut _newline_pos) = buffer.iter().position(|&b| b == b'\n') {
                        // Check if it's a CRLF. If so, include the \r in the split.
                        // CRLFかどうかをチェック。もしそうなら、\r もスプリットに含める。
                        let mut line_length = _newline_pos + 1; // Length including the '\n'
                        if _newline_pos > 0 && buffer[_newline_pos - 1] == b'\r' {
                            _newline_pos -= 1; // Adjust position to start of \r
                            line_length += 1; // Include \r in the length to remove
                        }

                        let line_bytes = buffer.split_to(line_length); // Split including the newline sequence
                        let line_str = str::from_utf8(&line_bytes)
                            .map_err(|e| format!("Invalid UTF-8 in stream: {}", e))?
                            .trim();

                        // Debug: Parsed line string
                        // println!("[DEBUG] Parsed line: \"{}\"", line_str);

                        if let Some(json_str) = line_str.strip_prefix("data: ") {
                            // Debug: JSON string to parse
                            // println!("[DEBUG] JSON string to parse: \"{}\"", json_str);

                            if json_str == "[DONE]" {
                                // Debug: [DONE] received
                                // println!("[DEBUG] [DONE] received. Terminating stream.");
                                return Ok(None); // This terminates the stream
                            } else {
                                match serde_json::from_str::<ChatCompletionChunk>(json_str) {
                                    Ok(chunk) => {
                                        if let Some(choice) = chunk.choices.into_iter().next() {
                                            if let Some(content) = choice.delta.content {
                                                // Debug: Content found
                                                // println!("[DEBUG] Content found: \"{}\"", content);
                                                // Return the content as Ok(String) and continue with the remaining state
                                                return Ok(Some((content, (buffer, byte_stream))));
                                            } else if choice.finish_reason.is_some() {
                                                // Debug: Finish reason without content
                                                // println!("[DEBUG] Finish reason without content. Terminating stream cleanly.");
                                                // No content but a finish reason means this is likely the last chunk
                                                return Ok(None); // Terminate the stream cleanly
                                            } else {
                                                // Debug: No content, no finish reason. Continue processing buffer.
                                                // println!("[DEBUG] No content, no finish_reason. Continuing...");
                                                // No content and no finish_reason, just continue to next line/chunk
                                                continue;
                                            }
                                        } else {
                                            // Debug: No choices in chunk. Continue processing buffer.
                                            // println!("[DEBUG] No choices in chunk. Continuing...");
                                            // No choices or first choice is empty, continue processing buffer
                                            continue;
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "Failed to parse JSON chunk: {} (Line: {})",
                                            e, json_str
                                        );
                                        return Err(format!("Failed to parse JSON chunk: {}", e));
                                    }
                                }
                            }
                        }
                        // If it's not a data line, continue loop to process more lines in buffer
                        // データ行でない場合、バッファ内のさらに多くの行を処理するためにループを続行
                        // println!("[DEBUG] Non-data line or unrecognized. Continuing...");
                        continue; // Process next line in buffer
                    } else {
                        // No complete line in buffer, try to read more from the underlying byte_stream
                        // バッファに完全な行がないため、基になる byte_stream からさらに読み込もうと試みる
                        // println!("[DEBUG] No complete line in buffer. Reading more from byte_stream...");
                        match byte_stream.next().await {
                            Some(chunk_result) => {
                                let chunk = chunk_result
                                    .map_err(|e| format!("Error reading stream chunk: {}", e))?;
                                // Debug: Bytes read from stream
                                // println!("[DEBUG] Read {} bytes from stream. Adding to buffer.", chunk.len());
                                buffer.extend_from_slice(&chunk); // Add new bytes to buffer
                            }
                            None => {
                                // End of underlying byte stream, and no more complete lines in buffer
                                // println!("[DEBUG] End of byte_stream. Terminating stream.");
                                return Ok(None); // Terminate the stream
                            }
                        }
                    }
                }
            },
        )
        .map_err(|e| e.to_string())
        .boxed();

        Ok(processed_stream)
    }
}
