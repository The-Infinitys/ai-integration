// src/modules/agent/api/ollama.rs
use bytes::Bytes;
use colored::*;
use futures_util::StreamExt;
use futures_util::stream::{Stream, TryStreamExt};
use reqwest::{Client, Error as ReqwestError};
use serde::{Deserialize, Serialize};
use serde_json::Error as SerdeJsonError;
use std::boxed::Box;
use std::fmt;
use std::pin::Pin;

#[derive(Serialize, Default)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool, // default_true関数は削除されました
    pub options: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    User,
    System,
    #[default]
    Assistant,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}
impl fmt::Display for ChatMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", "Role".green().bold(), self.role)?;
        write!(f, "{}: |\n  {}", "Content".cyan().bold(), self.content)
    }
}
impl fmt::Display for ChatRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User => write!(f, "{}", "You".yellow()),
            Self::Assistant => write!(f, "{}", "AI".green()),
            Self::System => write!(f, "{}", "System".cyan()),
        }
    }
}

#[derive(Deserialize, Default)]
#[allow(dead_code)]
pub struct ChatCompletionResponse {
    pub model: String,
    pub created_at: String,
    pub message: Option<ChatMessage>,
    pub done: bool,
    pub total_duration: Option<u64>,
}

#[derive(Debug)]
pub enum OllamaApiError {
    Reqwest(ReqwestError),
    SerdeJson(SerdeJsonError),
    ApiError(String),
    StreamError(String),
    IoError(std::io::Error),
    #[allow(dead_code)]
    NoMessageFound,
}

impl From<ReqwestError> for OllamaApiError {
    fn from(err: ReqwestError) -> Self {
        OllamaApiError::Reqwest(err)
    }
}

impl From<SerdeJsonError> for OllamaApiError {
    fn from(err: serde_json::Error) -> Self {
        OllamaApiError::SerdeJson(err)
    }
}

impl From<std::io::Error> for OllamaApiError {
    fn from(err: std::io::Error) -> Self {
        OllamaApiError::IoError(err)
    }
}

impl std::fmt::Display for OllamaApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OllamaApiError::Reqwest(e) => write!(f, "Reqwest error: {}", e),
            OllamaApiError::SerdeJson(e) => write!(f, "JSON parsing error: {}", e),
            OllamaApiError::ApiError(msg) => write!(f, "API error: {}", msg),
            OllamaApiError::IoError(e) => write!(f, "IO error: {}", e),
            OllamaApiError::StreamError(msg) => write!(f, "Stream error: {}", msg),
            OllamaApiError::NoMessageFound => write!(f, "No message content found in API response"),
        }
    }
}

impl std::error::Error for OllamaApiError {}

#[derive(Debug, Clone)]
pub struct OllamaApi {
    client: Client,
    base_url: String,
    default_model: String,
}

impl OllamaApi {
    pub fn new(base_url: String, default_model: String) -> Self {
        let client = Client::new();
        OllamaApi {
            client,
            base_url,
            default_model,
        }
    }

    pub fn set_model(&mut self, model_name: String) {
        self.default_model = model_name;
    }

    pub async fn list_models(&self) -> Result<serde_json::Value, OllamaApiError> {
        let url = format!("{}/api/tags", self.base_url);
        let response = self.client.get(&url).send().await?.json().await?;
        Ok(response)
    }

    pub async fn get_chat_completion_stream(
        &self,
        messages: Vec<ChatMessage>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, OllamaApiError>> + Send>>, OllamaApiError>
    {
        let request_body = ChatCompletionRequest {
            model: self.default_model.clone(),
            messages,
            stream: true,
            options: Some(serde_json::json!({
                "temperature": 0.7,
            })),
        };

        let url = format!("{}/api/chat", self.base_url);
        let response = self.client.post(&url).json(&request_body).send().await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown API error".to_string());
            return Err(OllamaApiError::ApiError(format!(
                "APIリクエストが失敗しました: ステータス {} - {}",
                status, error_text
            )));
        }

        let body_stream = response.bytes_stream();

        let stream = body_stream
            .map_err(OllamaApiError::Reqwest)
            .and_then(|bytes: Bytes| async move {
                let s = String::from_utf8(bytes.to_vec()).map_err(|e| {
                    OllamaApiError::StreamError(format!("Invalid UTF-8 sequence: {}", e))
                })?;

                // Ollamaのストリームには "data: " プレフィックスが付与される場合があります
                let trimmed_s = s.trim();
                let json_str = trimmed_s.strip_prefix("data: ").unwrap_or(trimmed_s);

                if json_str == "[DONE]" {
                    return Ok(None);
                }

                let response_obj: ChatCompletionResponse = serde_json::from_str(json_str)?;

                if let Some(message) = response_obj.message {
                    if !message.content.is_empty() {
                        return Ok(Some(message.content));
                    }
                }
                Ok(None)
            })
            .try_filter_map(|opt_content| async move { Ok(opt_content) })
            .boxed();

        Ok(stream)
    }
}
