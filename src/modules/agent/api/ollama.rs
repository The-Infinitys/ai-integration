// src/modules/agent/api/ollama.rs
use reqwest::{Client, Error as ReqwestError};
use serde::{Deserialize, Serialize};
use serde_json::Error as SerdeJsonError;
use futures_util::stream::{Stream, TryStreamExt};
use futures_util::StreamExt;
use bytes::Bytes;

use std::pin::Pin;
use std::boxed::Box;

#[derive(Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
    pub options: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    User,
    System,
    Assistant,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Deserialize, Debug)]
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
    NoMessageFound,
}

impl From<ReqwestError> for OllamaApiError {
    fn from(err: ReqwestError) -> Self {
        OllamaApiError::Reqwest(err)
    }
}

impl From<SerdeJsonError> for OllamaApiError {
    fn from(err: SerdeJsonError) -> Self {
        OllamaApiError::SerdeJson(err)
    }
}

impl From<std::io::Error> for OllamaApiError {
    fn from(err: std::io::Error) -> Self {
        OllamaApiError::IoError(err)
    }
}

// Make the struct public
pub struct OllamaApi { // <--- Added 'pub' here
    client: Client,
    base_url: String,
    default_model: String,
}

impl OllamaApi {
    pub fn new(base_url: String, default_model: String) -> Self {
        let client = Client::new();
        OllamaApi { client, base_url, default_model }
    }

    pub async fn list_models(&self) -> Result<serde_json::Value, OllamaApiError> {
        let url = format!("{}/api/tags", self.base_url);
        let response = self.client.get(&url).send().await?.json().await?;
        Ok(response)
    }

    pub async fn get_chat_completion_stream(
        &self,
        messages: Vec<ChatMessage>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, OllamaApiError>> + Send>>, OllamaApiError> {
        let request_body = ChatCompletionRequest {
            model: self.default_model.clone(),
            messages,
            stream: true,
            options: Some(serde_json::json!({
                "temperature": 0.7,
            })),
        };

        let url = format!("{}/api/chat", self.base_url);
        let response = self.client
            .post(&url)
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown API error".to_string());
            return Err(OllamaApiError::ApiError(format!("APIリクエストが失敗しました: ステータス {} - {}", status, error_text)));
        }

        let body_stream = response.bytes_stream();

        let stream = body_stream
            .map_err(OllamaApiError::Reqwest)
            .and_then(|bytes: Bytes| async move {
                let s = String::from_utf8(bytes.to_vec())
                    .map_err(|e| OllamaApiError::StreamError(format!("Invalid UTF-8 sequence: {}", e)))?;

                let json_str = s.strip_prefix("data: ").unwrap_or(&s).trim();

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
            .try_filter_map(|opt_content| async move {
                Ok(opt_content)
            })
            .boxed();

        Ok(stream)
    }
}