// src/modules/agent/api.rs
use std::collections::HashMap;
pub mod ollama;
pub struct AIApi{
    // Stores URLs and authentication information for using the API.
    // APIを使用するためのURLや認証情報を入れておく
    pub info:HashMap<String,String>,
}

impl AIApi{
    /// Creates a new `AIApi` instance.
    /// 新しい `AIApi` インスタンスを作成します。
    pub fn new() -> Self {
        Self {
            info: HashMap::new(), // Initialize with an empty HashMap
        }
    }

    /// Adds API information (e.g., URL, API key) to the `AIApi` instance.
    /// API情報（例：URL、APIキー）を`AIApi`インスタンスに追加します。
    pub fn add_info(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.info.insert(key.into(), value.into());
    }
}

pub enum ApiType{
    Ollama
}

// TODO OpenAIApi, GeminiAIApiなど、様々なサービスを使用して
