// src/modules/agent/api.rs
pub mod gemini;
pub mod ollama;

use async_trait::async_trait;
use colored::*;
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use std::boxed::Box;
use std::fmt;
use std::pin::Pin;

// Common ChatMessage and ChatRole definitions
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    User,
    System,
    #[default]
    Assistant,
    Tool,
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
            Self::Tool => write!(f, "{}", "Tool".blue()),
        }
    }
}

// Common Error type for API operations
#[derive(Debug)]
pub enum ApiError {
    Reqwest(reqwest::Error),
    SerdeJson(serde_json::Error),
    Message(String),
    StreamError(String),
    IoError(std::io::Error),
    UnsupportedOperation(String),
}

impl From<reqwest::Error> for ApiError {
    fn from(err: reqwest::Error) -> Self {
        ApiError::Reqwest(err)
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::SerdeJson(err)
    }
}

impl From<std::io::Error> for ApiError {
    fn from(err: std::io::Error) -> Self {
        ApiError::IoError(err)
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Reqwest(e) => write!(f, "Reqwest error: {}", e),
            ApiError::SerdeJson(e) => write!(f, "JSON parsing error: {}", e),
            ApiError::Message(msg) => write!(f, "API error: {}", msg),
            ApiError::IoError(e) => write!(f, "IO error: {}", e),
            ApiError::StreamError(msg) => write!(f, "Stream error: {}", msg),

            ApiError::UnsupportedOperation(msg) => write!(f, "Unsupported operation: {}", msg),
        }
    }
}

impl std::error::Error for ApiError {}

/// Trait for AI API implementations (Ollama, Gemini, etc.)
#[async_trait]
pub trait AIApiTrait: Send + Sync {
    fn set_model(&mut self, model_name: String);
    fn get_model(&self) -> String;
    async fn list_models(&self) -> Result<serde_json::Value, ApiError>;
    async fn get_chat_completion_stream(
        &self,
        messages: Vec<ChatMessage>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, ApiError>> + Send>>, ApiError>;
    fn clone_box(&self) -> Box<dyn AIApiTrait>;
}

/// Enum to select the concrete AI API implementation
pub enum AIProvider {
    Ollama,
    Gemini,
}

/// Main API struct that holds a boxed trait object
pub struct AIApi {
    inner: Box<dyn AIApiTrait>,
}

impl Clone for AIApi {
    fn clone(&self) -> Self {
        AIApi {
            inner: self.inner.clone_box(),
        }
    }
}

impl AIApi {
    pub fn new(provider: AIProvider, base_url: String, default_model: String) -> Self {
        match provider {
            AIProvider::Ollama => {
                let ollama_api = ollama::OllamaApi::new(base_url, default_model);
                AIApi {
                    inner: Box::new(ollama_api),
                }
            }
            AIProvider::Gemini => {
                let gemini_api = gemini::GeminiApi::new(base_url, default_model);
                AIApi {
                    inner: Box::new(gemini_api),
                }
            }
        }
    }

    pub fn set_model(&mut self, model_name: String) {
        self.inner.set_model(model_name);
    }

    pub fn get_model(&self) -> String {
        self.inner.get_model()
    }

    pub async fn list_models(&self) -> Result<serde_json::Value, ApiError> {
        self.inner.list_models().await
    }

    pub async fn get_chat_completion_stream(
        &self,
        messages: Vec<ChatMessage>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, ApiError>> + Send>>, ApiError> {
        self.inner.get_chat_completion_stream(messages).await
    }
}
