// src/modules/agent/api.rs
pub mod ollama;
pub mod gemini;
pub use ollama::{ChatMessage, ChatRole, OllamaApiError};

use ollama::OllamaApi;

use futures_util::stream::Stream;
use std::pin::Pin;
use std::boxed::Box;

#[derive(Debug, Clone)]
pub struct AIApi {
    ollama_api: OllamaApi,
}

impl AIApi {
    pub fn new(base_url: String, default_model: String) -> Self {
        let ollama_api = OllamaApi::new(base_url, default_model);
        Self { ollama_api }
    }

    pub fn set_model(&mut self, model_name: String) {
        self.ollama_api.set_model(model_name);
    }

    pub async fn list_models(&self) -> Result<serde_json::Value, OllamaApiError> {
        self.ollama_api.list_models().await
    }

    pub async fn get_chat_completion_stream(&self, messages: Vec<ChatMessage>) -> Result<Pin<Box<dyn Stream<Item = Result<String, OllamaApiError>> + Send>>, OllamaApiError> {
        self.ollama_api.get_chat_completion_stream(messages).await
    }
}