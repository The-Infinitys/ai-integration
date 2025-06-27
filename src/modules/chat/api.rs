// src/modules/chat/api.rs

use async_trait::async_trait;
use futures_util::TryStreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::io::{self, Write};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::io::StreamReader;
use chrono::{Local, DateTime}; // For current date and time
use tokio::fs; // For async file system operations
use std::path::Path; // For path manipulation

/// AIAgentApiトレイトは、AIエージェントとやり取りするための基本的なインターフェースを定義します。
#[async_trait]
pub trait AIAgentApi {
    /// ユーザーの入力に基づいてAIからの応答を非同期に取得します。
    ///
    /// # 引数
    /// * `user_input` - ユーザーからのテキスト入力。
    ///
    /// # 戻り値
    /// `Result<String, Box<dyn Error>>` - AIからの応答文字列、またはエラー。
    async fn get_ai_response(&mut self, user_input: &str) -> Result<String, Box<dyn Error>>;
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

// Ollama streaming response struct
#[derive(Deserialize)]
struct OllamaStreamResponse {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    done: Option<bool>,
}

// ストリーミングレスポンス内の選択肢の内部構造体
#[derive(Deserialize)]
struct StreamChoice {
    delta: Message,
}

/// OllamaAIAgentApiはAIAgentApiトレイトのOllama実装です。
/// OllamaサーバーとHTTPで通信し、AIの応答を取得します。
pub struct OllamaAIAgentApi {
    client: Client,
    ollama_url: String,
    model_name: String,
    chat_history: Vec<Message>, // 会話履歴を保持
}

impl OllamaAIAgentApi {
    /// 新しい`OllamaAIAgentApi`のインスタンスを作成します。
    ///
    /// # 引数
    /// * `ollama_url` - OllamaサーバーのURL (例: "http://localhost:11434")。
    /// * `model_name` - 使用するOllamaモデルの名前 (例: "llama2")。
    pub fn new(ollama_url: String, model_name: String) -> Self {
        OllamaAIAgentApi {
            client: Client::new(),
            ollama_url,
            model_name,
            chat_history: vec![Message {
                role: "system".to_string(),
                content: "You are a helpful assistant. You are an AI assistant in Marugame, Kagawa, Japan.".to_string(),
            }],
        }
    }

    // 現在の日時を取得してフォーマットするヘルパー関数
    fn get_current_datetime() -> String {
        let now: DateTime<Local> = Local::now();
        now.format("%Y-%m-%d %H:%M:%S").to_string()
    }

    // 指定されたパスのファイル情報を取得するヘルパー関数
    async fn get_file_status(path: &str) -> String {
        let path_obj = Path::new(path);
        if !path_obj.exists() {
            return format!("パス '{}' は存在しません。", path);
        }

        let mut entries = match fs::read_dir(path_obj).await {
            Ok(dir) => dir,
            Err(e) => return format!("ディレクトリ '{}' の読み取りに失敗しました: {}", path, e),
        };

        let mut file_info = String::new();
        file_info.push_str(&format!("ディレクトリ '{}' の内容:\n", path));

        while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
            let file_name = entry.file_name();
            let file_type = entry.file_type().await;

            let type_str = match file_type {
                Ok(ft) if ft.is_file() => "File",
                Ok(ft) if ft.is_dir() => "Dir",
                _ => "その他",
            };
            file_info.push_str(&format!("- {}: {}\n", type_str, file_name.to_string_lossy()));
        }
        file_info
    }
}

impl Default for OllamaAIAgentApi {
    fn default() -> Self {
        let model_name = {
            let output = std::process::Command::new("ollama")
                .arg("list")
                .output()
                .expect("Failed to execute ollama list command");

            let stdout = String::from_utf8_lossy(&output.stdout);
            let model_line = stdout
                .lines()
                .nth(1)
                .and_then(|line| line.split_whitespace().next())
                .unwrap_or("llama2");
            model_line.to_string()
        }
        .to_string();
        Self::new("http://localhost:11434".to_string(), model_name)
    }
}

#[async_trait]
impl AIAgentApi for OllamaAIAgentApi {
    async fn get_ai_response(&mut self, user_input: &str) -> Result<String, Box<dyn Error>> {
        // 現在のコンテキスト情報を取得
        let current_datetime = OllamaAIAgentApi::get_current_datetime();
        // 現在の作業ディレクトリを渡す
        let current_dir = ".".to_string(); // または env::current_dir().unwrap().to_string_lossy().into_owned() で実際のパスを取得
        let file_status = Self::get_file_status(&current_dir).await;

        // システムメッセージに現在のコンテキストを追加
        let context_message_content = format!(
            "現在の状況: 日時: {}。 場所: 丸亀市、香川県、日本。現在のディレクトリの内容:\n{}",
            current_datetime, file_status
        );
        // 既存のsystemメッセージを更新するか、新しいコンテキストメッセージを追加
        // ここでは、新しいuserメッセージの前にsystemメッセージとして追加します。
        // これにより、各リクエストで最新のコンテキストが提供されます。
        self.chat_history.push(Message {
            role: "system".to_string(),
            content: context_message_content,
        });


        // ユーザーメッセージを履歴に追加
        self.chat_history.push(Message {
            role: "user".to_string(),
            content: user_input.to_string(),
        });

        let request_body = OllamaChatRequest {
            model: self.model_name.clone(),
            messages: self.chat_history.clone(),
            stream: true,
        };

        let request_url = format!("{}/v1/chat/completions", self.ollama_url);

        let response = self
            .client
            .post(&request_url)
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to get response body".to_string());
            return Err(format!("Ollama APIリクエストが失敗しました。ステータス: {}, ボディ: {}. Ollamaサーバーが {} で実行されており、モデル '{}' が利用可能であることを確認してください。", status, text, self.ollama_url, self.model_name).into());
        }

        fn reqwest_error_to_io_error(e: reqwest::Error) -> std::io::Error {
            io::Error::other(e)
        }

        let byte_stream = response.bytes_stream().map_err(reqwest_error_to_io_error);
        let mut reader = BufReader::new(StreamReader::new(byte_stream));

        let mut full_response_content = String::new();
        let mut buffer = String::new();

        loop {
            buffer.clear();
            let bytes_read = reader.read_line(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }

            let line_content = buffer.trim();
            if line_content.is_empty() {
                continue;
            }

            let json_str = if line_content.starts_with("data: ") {
                &line_content[6..]
            } else {
                line_content
            };

            if json_str == "[DONE]" {
                break;
            }

            match serde_json::from_str::<OllamaStreamResponse>(json_str) {
                Ok(stream_response) => {
                    if let Some(choice) = stream_response.choices.into_iter().next() {
                        print!("{}", choice.delta.content);
                        io::stdout().flush()?;
                        full_response_content.push_str(&choice.delta.content);
                    }
                    if stream_response.done == Some(true) {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!(
                        "OllamaストリームからのJSON行のパースに失敗しました: {:?}, 行: '{}'",
                        e, line_content
                    );
                    continue;
                }
            }
        }

        println!();

        self.chat_history.push(Message {
            role: "assistant".to_string(),
            content: full_response_content.clone(),
        });

        Ok(full_response_content)
    }
}