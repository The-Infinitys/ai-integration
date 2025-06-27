// src/ai.rs
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::{error::Error};
// use std::env; // 環境変数を読み込む必要がなくなったためコメントアウト
use crate::config::Config; // Configモジュールをインポート

/// AIプロバイダーの種類を定義する列挙型
#[derive(Clone, Copy, PartialEq)]
pub enum AIProvider {
    OpenAI,
    Ollama,
    Gemini,
}
impl fmt::Display for AIProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpenAI => write!(f, "OpenAI"),
            Self::Ollama => write!(f, "Ollama"),
            Self::Gemini => write!(f, "Gemini"),
        }
    }
}
/// AIモデルとの対話のための共通インターフェース
#[async_trait]
pub trait AIGenerator {
    /// 指定されたプロンプトに基づいてコンテンツを生成する非同期メソッド
    async fn generate_content(&self, prompt: &str) -> Result<String, Box<dyn Error>>;
}

/// OpenAI APIクライアント
pub struct OpenAIChat {
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::Client,
}

impl OpenAIChat {
    /// 新しいOpenAIChatインスタンスを作成
    /// ConfigからAPIキーとモデルを読み込みます。
    pub fn new(config: &Config) -> Result<Self, Box<dyn Error>> {
        // APIキーが設定されているかチェック
        if config.openai_api_key == "YOUR_OPENAI_API_KEY" || config.openai_api_key.is_empty() {
            return Err(
                "ConfigにOpenAI APIキーが設定されていません。config.rsを更新してください。".into(),
            );
        }
        let base_url = "https://api.openai.com/v1/chat/completions".to_string();
        let client = reqwest::Client::new();
        Ok(Self {
            api_key: config.openai_api_key.clone(),
            model: config.openai_model.clone(),
            base_url,
            client,
        })
    }
}

#[async_trait]
impl AIGenerator for OpenAIChat {
    async fn generate_content(&self, prompt: &str) -> Result<String, Box<dyn Error>> {
        #[derive(Serialize, Deserialize)]
        struct Message {
            role: String,
            content: String,
        }

        #[derive(Serialize, Deserialize)]
        struct RequestBody {
            model: String,
            messages: Vec<Message>,
        }

        #[derive(Serialize, Deserialize)]
        struct Choice {
            message: Message,
        }

        #[derive(Serialize, Deserialize)]
        struct APIResponse {
            choices: Vec<Choice>,
        }

        let body = RequestBody {
            model: self.model.clone(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
        };

        let res = self
            .client
            .post(&self.base_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?
            .json::<APIResponse>()
            .await?;

        if let Some(choice) = res.choices.into_iter().next() {
            Ok(choice.message.content)
        } else {
            Err("OpenAIから応答がありませんでした".into())
        }
    }
}

/// Ollama APIクライアント
pub struct OllamaChat {
    model: String,
    base_url: String, // 通常は http://localhost:11434/api/generate
    client: reqwest::Client,
}

impl OllamaChat {
    /// 新しいOllamaChatインスタンスを作成
    /// ConfigからモデルとベースURLを読み込みます。
    pub fn new(config: &Config) -> Self {
        let client = reqwest::Client::new();
        Self {
            model: config.ollama_model.clone(),
            base_url: config.ollama_base_url.clone(),
            client,
        }
    }
}

#[async_trait]
impl AIGenerator for OllamaChat {
    async fn generate_content(&self, prompt: &str) -> Result<String, Box<dyn Error>> {
        #[derive(Serialize)]
        struct RequestBody {
            model: String,
            prompt: String,
            stream: bool, // ストリーミングではない単一の応答を要求
        }

        #[derive(Deserialize)]
        struct APIResponse {
            response: String,
        }

        let body = RequestBody {
            model: self.model.clone(),
            prompt: prompt.to_string(),
            stream: false,
        };

        let res = self
            .client
            .post(&self.base_url)
            .json(&body)
            .send()
            .await?
            .json::<APIResponse>()
            .await?;

        Ok(res.response)
    }
}

/// Gemini APIクライアント
pub struct GeminiChat {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl GeminiChat {
    /// 新しいGeminiChatインスタンスを作成
    /// ConfigからAPIキーとモデルを読み込みます。
    pub fn new(config: &Config) -> Result<Self, Box<dyn Error>> {
        // APIキーが設定されているかチェック
        if config.gemini_api_key == "YOUR_GEMINI_API_KEY" || config.gemini_api_key.is_empty() {
            return Err(
                "ConfigにGemini APIキーが設定されていません。config.rsを更新してください。".into(),
            );
        }
        let base_url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            config.gemini_model
        );
        let client = reqwest::Client::new();
        Ok(Self {
            api_key: config.gemini_api_key.clone(),
            base_url,
            client,
        })
    }
}

#[async_trait]
impl AIGenerator for GeminiChat {
    async fn generate_content(&self, prompt: &str) -> Result<String, Box<dyn Error>> {
        #[derive(Serialize)]
        struct Part {
            text: String,
        }

        #[derive(Serialize)]
        struct Content {
            role: String,
            parts: Vec<Part>,
        }

        #[derive(Serialize)]
        struct RequestBody {
            contents: Vec<Content>,
        }

        #[derive(Deserialize)]
        struct CandidatePart {
            text: String,
        }

        #[derive(Deserialize)]
        struct CandidateContent {
            parts: Vec<CandidatePart>,
        }

        #[derive(Deserialize)]
        struct Candidate {
            content: CandidateContent,
        }

        #[derive(Deserialize)]
        struct APIResponse {
            candidates: Vec<Candidate>,
        }

        let body = RequestBody {
            contents: vec![Content {
                role: "user".to_string(),
                parts: vec![Part {
                    text: prompt.to_string(),
                }],
            }],
        };

        let res = self
            .client
            .post(&self.base_url)
            .query(&[("key", &self.api_key)]) // APIキーをクエリパラメータとして追加
            .json(&body)
            .send()
            .await?
            .json::<APIResponse>()
            .await?;

        if let Some(candidate) = res.candidates.into_iter().next() {
            if let Some(part) = candidate.content.parts.into_iter().next() {
                Ok(part.text)
            } else {
                Err("Geminiからコンテンツ部分がありませんでした".into())
            }
        } else {
            Err("Geminiから候補がありませんでした".into())
        }
    }
}
