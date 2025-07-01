// src/modules/agent/api/ollama.rs
use bytes::Bytes;
use futures_util::StreamExt;
use futures_util::TryStreamExt;
use futures_util::stream::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::boxed::Box;
use std::pin::Pin;

use async_trait::async_trait;

use crate::modules::agent::api::{AIApiTrait, ApiError, ChatMessage};

#[derive(Serialize, Default)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
    pub options: Option<serde_json::Value>,
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
}

#[async_trait]
impl AIApiTrait for OllamaApi {
    fn set_model(&mut self, model_name: String) {
        self.default_model = model_name;
    }

    fn get_model(&self) -> String {
        self.default_model.clone()
    }

    async fn list_models(&self) -> Result<serde_json::Value, ApiError> {
        let url = format!("{}/api/tags", self.base_url);
        let response = self.client.get(&url).send().await?.json().await?;
        Ok(response)
    }

    async fn get_chat_completion_stream(
        &self,
        messages: Vec<ChatMessage>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, ApiError>> + Send>>, ApiError> {
        let request_body = ChatCompletionRequest {
            model: self.default_model.clone(),
            messages,
            stream: true,
            options: Some(serde_json::json!({ "temperature": 0.7, })),
        };

        let url = format!("{}/api/chat", self.base_url);
        let response = self.client.post(&url).json(&request_body).send().await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown API error".to_string());
            return Err(ApiError::ApiError(format!(
                "APIリクエストが失敗しました: ステータス {} - {}",
                status, error_text
            )));
        }

        let body_stream = response.bytes_stream();

        let stream = body_stream
            .map_err(ApiError::Reqwest)
            .and_then(|bytes: Bytes| async move {
                let s = String::from_utf8(bytes.to_vec())
                    .map_err(|e| ApiError::StreamError(format!("Invalid UTF-8 sequence: {}", e)))?;

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

    fn clone_box(&self) -> Box<dyn AIApiTrait> {
        Box::new(self.clone())
    }
}
