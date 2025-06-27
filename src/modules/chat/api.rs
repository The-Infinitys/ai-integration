// src/modules/chat/api.rs

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize}; // JSONのシリアライズ/デシリアライズ用
use std::error::Error; // HTTPリクエスト用

/// `AIAgentApi`トレイトは、AIエージェントとやり取りするための基本的なインターフェースを定義します。
#[async_trait]
pub trait AIAgentApi {
    /// ユーザーの入力に基づいてAIからの応答を非同期に取得します。
    ///
    /// # Arguments
    /// * `user_input` - ユーザーからのテキスト入力。
    ///
    /// # Returns
    /// `Result<String, Box<dyn Error>>` - AIからの応答文字列、またはエラー。
    async fn get_ai_response(&self, user_input: &str) -> Result<String, Box<dyn Error>>;
}

// Ollama APIのchat/completionsエンドポイント用のリクエストボディの構造体
#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<Message>,
    stream: bool,
}

// チャットメッセージの構造体（ロールとコンテンツを含む）
#[derive(Serialize, Deserialize, Clone)]
struct Message {
    role: String,
    content: String,
}

// Ollama APIのchat/completionsエンドポイントからのレスポンスボディの構造体
#[derive(Deserialize)]
struct OllamaChatResponse {
    choices: Vec<Choice>,
    // 他のフィールドも必要に応じて追加できます (e.g., created, modelなど)
}

// レスポンス内の選択肢の構造体
#[derive(Deserialize, Clone)]
struct Choice {
    message: Message,
    // 必要に応じて他のフィールドも追加できます (e.g., index, logprobs, finish_reasonなど)
}

/// `OllamaAIAgentApi`は`AIAgentApi`トレイトのOllama実装です。
/// OllamaサーバーとHTTPで通信し、AIの応答を取得します。
pub struct OllamaAIAgentApi {
    client: Client,
    ollama_url: String,
    model_name: String,
    // 会話履歴を保持するためのフィールドを追加
    chat_history: Vec<Message>,
}

impl OllamaAIAgentApi {
    /// 新しい`OllamaAIAgentApi`のインスタンスを作成します。
    ///
    /// # Arguments
    /// * `ollama_url` - OllamaサーバーのURL (例: "http://localhost:11434")。
    /// * `model_name` - 使用するOllamaモデルの名前 (例: "llama2")。
    pub fn new(ollama_url: String, model_name: String) -> Self {
        OllamaAIAgentApi {
            client: Client::new(),
            ollama_url,
            model_name,
            // 初期システムメッセージを追加してチャット履歴を初期化
            chat_history: vec![Message {
                role: "system".to_string(),
                content: "You are a helpful assistant.".to_string(),
            }],
        }
    }
}

#[async_trait]
impl AIAgentApi for OllamaAIAgentApi {
    async fn get_ai_response(&self, user_input: &str) -> Result<String, Box<dyn Error>> {
        // 現在のチャット履歴にユーザーメッセージを追加
        let mut messages_for_request = self.chat_history.clone();
        messages_for_request.push(Message {
            role: "user".to_string(),
            content: user_input.to_string(),
        });

        let request_body = OllamaChatRequest {
            model: self.model_name.clone(),
            messages: messages_for_request,
            stream: false, // ストリーミングではなく、完全な応答を待機します
        };

        let request_url = format!("{}/v1/chat/completions", self.ollama_url);

        // Ollama APIへのPOSTリクエストを送信
        let response = self
            .client
            .post(&request_url)
            .json(&request_body)
            .send()
            .await?
            .error_for_status()?; // HTTPステータスが2xx以外の場合はエラーを返します

        // レスポンスボディをJSONとしてパース
        let ollama_response: OllamaChatResponse = response.json().await?;

        // 最初の選択肢のメッセージコンテンツを返す
        if let Some(choice) = ollama_response.choices.into_iter().next() {
            // アシスタントの応答をチャット履歴に追加（現在の実装ではchat_historyがmutではないため、これは動作しませんが、
            // chat_historyを可変にするか、新しいOllamaAIAgentApiインスタンスを返す必要があります。）
            // 現時点では、簡略化のため返された応答のみを返します。
            Ok(choice.message.content)
        } else {
            Err("No response from AI".into()) // エラー処理
        }
    }
}
