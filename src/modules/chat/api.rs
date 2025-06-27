// src/modules/chat/api.rs

use async_trait::async_trait;
use futures_util::TryStreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::io::{self, Write}; // For flushing stdout
use tokio::io::{AsyncBufReadExt, BufReader}; // For streaming
use tokio_util::io::StreamReader; // To convert reqwest stream to tokio's AsyncRead

/// `AIAgentApi` trait defines the basic interface for interacting with an AI agent.
#[async_trait]
pub trait AIAgentApi {
    /// Asynchronously retrieves a response from the AI based on user input.
    ///
    /// # Arguments
    /// * `user_input` - The text input from the user.
    ///
    /// # Returns
    /// `Result<String, Box<dyn Error>>` - The AI's response string, or an error.
    async fn get_ai_response(&mut self, user_input: &str) -> Result<String, Box<dyn Error>>;
}

// Struct for the request body to the Ollama chat/completions endpoint
#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<Message>,
    stream: bool,
}

// Struct for a chat message (includes role and content)
#[derive(Serialize, Deserialize, Clone)]
struct Message {
    role: String,
    content: String,
}

// Ollama streaming response struct
#[derive(Deserialize)]
struct OllamaStreamResponse {
    // Add `serde(default)` so the `choices` field defaults to an empty vector
    // if it's not present in the response.
    #[serde(default)]
    choices: Vec<StreamChoice>,
    done: Option<bool>,
    // Other fields like `id`, `object`, `created`, `model`, `system_fingerprint` are ignored here
    // as they are not specifically needed.
}

// Internal struct for a choice within a streaming response
#[derive(Deserialize)]
struct StreamChoice {
    // For streaming responses, the partial content is in a `delta` field.
    delta: Message,
    // index: Option<u32>,
    // finish_reason: Option<String>,
}

/// `OllamaAIAgentApi` is an Ollama implementation of the `AIAgentApi` trait.
/// It communicates with an Ollama server via HTTP to get AI responses.
pub struct OllamaAIAgentApi {
    client: Client,
    ollama_url: String,
    model_name: String,
    chat_history: Vec<Message>, // Stores conversation history
}

impl OllamaAIAgentApi {
    /// Creates a new instance of `OllamaAIAgentApi`.
    ///
    /// # Arguments
    /// * `ollama_url` - The URL of the Ollama server (e.g., "http://localhost:11434").
    /// * `model_name` - The name of the Ollama model to use (e.g., "llama2").
    pub fn new(ollama_url: String, model_name: String) -> Self {
        OllamaAIAgentApi {
            client: Client::new(),
            ollama_url,
            model_name,
            chat_history: vec![Message {
                role: "system".to_string(),
                content: "You are a helpful assistant.".to_string(),
            }],
        }
    }
}

impl Default for OllamaAIAgentApi {
    fn default() -> Self {
        // Rust's asynchronous runtime is not available within the Default trait,
        // so `Command::new` here will be a blocking call. While blocking I/O
        // should generally be avoided outside of an async context (like `tokio::main`),
        // it is tolerated here for a simplified default implementation.
        let model_name = {
            let output = std::process::Command::new("ollama")
                .arg("list")
                .output()
                .expect("Failed to execute ollama list command");

            let stdout = String::from_utf8_lossy(&output.stdout);
            let model_line = stdout
                .lines()
                .nth(1) // Get the second line (after the header)
                .and_then(|line| line.split_whitespace().next()) // Get the first word (model name)
                .unwrap_or("llama2"); // Default to llama2 if unable to get
            model_line.to_string()
        }
        .to_string();
        Self::new("http://localhost:11434".to_string(), model_name)
    }
}

#[async_trait]
impl AIAgentApi for OllamaAIAgentApi {
    // Use &mut self to update chat_history
    async fn get_ai_response(&mut self, user_input: &str) -> Result<String, Box<dyn Error>> {
        // Add user message to history
        self.chat_history.push(Message {
            role: "user".to_string(),
            content: user_input.to_string(),
        });

        let request_body = OllamaChatRequest {
            model: self.model_name.clone(),
            messages: self.chat_history.clone(), // Include current history in the request
            stream: true,                        // Enable streaming
        };

        let request_url = format!("{}/v1/chat/completions", self.ollama_url);

        // Send POST request to Ollama API
        let response = self
            .client
            .post(&request_url)
            .json(&request_body)
            .send()
            .await?;

        // Check HTTP status code and return a detailed message on error
        if !response.status().is_success() {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to get response body".to_string());
            return Err(format!("Ollama APIリクエストが失敗しました。ステータス: {}, ボディ: {}. Ollamaサーバーが {} で実行されており、モデル '{}' が利用可能であることを確認してください。", status, text, self.ollama_url, self.model_name).into());
        }

        // Helper function to convert `reqwest::Error` to `std::io::Error`
        fn reqwest_error_to_io_error(e: reqwest::Error) -> std::io::Error {
            io::Error::other(e)
        }

        // Process the response stream
        let byte_stream = response.bytes_stream().map_err(reqwest_error_to_io_error);

        // Create a reader that implements `AsyncRead` using `StreamReader`
        let mut reader = BufReader::new(StreamReader::new(byte_stream));

        let mut full_response_content = String::new();
        let mut buffer = String::new();

        loop {
            buffer.clear();
            let bytes_read = reader.read_line(&mut buffer).await?;
            if bytes_read == 0 {
                break; // End of stream
            }

            let line_content = buffer.trim();
            if line_content.is_empty() {
                continue; // Skip empty lines
            }

            // Remove "data: " prefix
            let json_str = if line_content.starts_with("data: ") {
                &line_content[6..] // Skip "data: "
            } else {
                line_content
            };

            // Handle "[DONE]" message indicating end of stream
            if json_str == "[DONE]" {
                break;
            }

            // Parse the JSON line received from the stream
            // Ollama's streaming usually contains complete JSON objects per line,
            // but error handling is included for empty or incomplete JSON lines.
            match serde_json::from_str::<OllamaStreamResponse>(json_str) {
                Ok(stream_response) => {
                    // Get message content from the first choice's DELTA
                    if let Some(choice) = stream_response.choices.into_iter().next() {
                        // Print the content immediately
                        print!("{}", choice.delta.content);
                        io::stdout().flush()?; // Flush to display immediately
                        full_response_content.push_str(&choice.delta.content);
                    }
                    // Break if `done: true` is received
                    if stream_response.done == Some(true) {
                        break;
                    }
                }
                Err(e) => {
                    // JSON parsing errors might occur with incomplete JSON lines at the end of the stream, etc.
                    // Log them but continue processing.
                    eprintln!(
                        "OllamaストリームからのJSON行のパースに失敗しました: {:?}, 行: '{}'",
                        e, line_content
                    );
                    continue;
                }
            }
        }

        // After the loop, print a newline to ensure the next prompt is on a new line
        println!(); 

        // Add the assistant's final response to the chat history
        self.chat_history.push(Message {
            role: "assistant".to_string(),
            content: full_response_content.clone(),
        });

        Ok(full_response_content)
    }
}
