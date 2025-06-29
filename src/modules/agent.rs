pub mod api;
use api::{ChatMessage, ChatRole, OllamaApiError};
use futures_util::stream::Stream;
use std::pin::Pin; // Pinをインポート
use std::boxed::Box; // Boxをインポート

pub struct AIAgent {
    api: api::AIApi,
    messages: Vec<ChatMessage>,
}

impl AIAgent {
    pub fn new(base_url: String, default_model: String) -> Self {
        let api = api::AIApi::new(base_url, default_model);
        let system_message = ChatMessage {
            role: ChatRole::System,
            content: "あなたは役立つAIアシスタントです。".to_string(),
        };
        AIAgent {
            api,
            messages: vec![system_message],
        }
    }

    pub async fn list_models(&self) -> Result<serde_json::Value, OllamaApiError> {
        self.api.list_models().await
    }

    // 戻り値の型を Pin<Box<dyn Stream>> に変更
    pub async fn chat_stream(&mut self, user_content: String) -> Result<Pin<Box<dyn Stream<Item = Result<String, OllamaApiError>> + Send>>, OllamaApiError> {
        self.messages.push(ChatMessage {
            role: ChatRole::User,
            content: user_content,
        });

        let stream = self.api.get_chat_completion_stream(self.messages.clone()).await?;

        Ok(stream)
    }

    pub fn add_ai_response(&mut self, ai_content: String) {
        self.messages.push(ChatMessage {
            role: ChatRole::Assistant,
            content: ai_content,
        });
    }

    pub fn revert_last_user_message(&mut self) {
        if self.messages.last().map_or(false, |m| m.role == ChatRole::User) {
            self.messages.pop();
        }
    }
}