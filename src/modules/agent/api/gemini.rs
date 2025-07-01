// src/modules/agent/api/gemini.rs
use async_trait::async_trait;
use futures_util::stream::Stream;
use serde_json::json;
use std::boxed::Box;
use std::pin::Pin;

use crate::modules::agent::api::{AIApiTrait, ApiError, ChatMessage};

#[derive(Debug, Clone)]
pub struct GeminiApi {
    #[allow(dead_code)]
    base_url: String,
    default_model: String,
}

impl GeminiApi {
    pub fn new(base_url: String, default_model: String) -> Self {
        GeminiApi {
            base_url,
            default_model,
        }
    }
}

#[async_trait]
impl AIApiTrait for GeminiApi {
    fn set_model(&mut self, model_name: String) {
        self.default_model = model_name;
    }

    async fn list_models(&self) -> Result<serde_json::Value, ApiError> {
        Ok(json!([
            { "name": "gemini-pro", "description": "Gemini Pro model" },
            { "name": "gemini-pro-vision", "description": "Gemini Pro Vision model" }
        ]))
    }

    async fn get_chat_completion_stream(
        &self,
        _messages: Vec<ChatMessage>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, ApiError>> + Send>>, ApiError> {
        Err(ApiError::UnsupportedOperation(
            "Gemini chat completion stream not yet implemented.".to_string(),
        ))
    }

    fn clone_box(&self) -> Box<dyn AIApiTrait> {
        Box::new(self.clone())
    }
}
