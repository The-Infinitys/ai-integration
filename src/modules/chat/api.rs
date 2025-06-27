// src/modules/chat/api.rs

use async_trait::async_trait;

use futures_util::TryStreamExt;

use reqwest::Client;

use serde::{Deserialize, Serialize};

use std::error::Error;

use tokio::io::{AsyncBufReadExt, BufReader}; // ストリーミング用

use tokio_util::io::StreamReader; // reqwestストリームをtokioのAsyncReadに変換するため

/// `AIAgentApi`トレイトは、AIエージェントとやり取りするための基本的なインターフェースを定義します。

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
    // `serde(default)` を追加し、`choices` フィールドがレスポンスに存在しない場合でも、

    // 空のベクターとしてデフォルトで扱われるようにします。
    #[serde(default)]
    choices: Vec<StreamChoice>,

    done: Option<bool>,
    // `model`, `created_at`, `total_duration` などの他のフィールドは、

    // ここでは特に必要ないため無視します。
}

// ストリーミングレスポンス内の選択肢の内部構造体

#[derive(Deserialize)]

struct StreamChoice {
    message: Message, // 既存の `Message` 構造体を使用
}

/// `OllamaAIAgentApi`は`AIAgentApi`トレイトのOllama実装です。
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

                content: "You are a helpful assistant.".to_string(),
            }],
        }
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
                .nth(1) // 2行目を取得 (ヘッダー行の次)
                .and_then(|line| line.split_whitespace().next()) // 最初の単語 (モデル名) を取得
                .unwrap_or("llama2"); // 取得できない場合はデフォルトでllama2
            model_line.to_string()
        }
        .to_string();
        Self::new("http://localhost:11434".to_string(), model_name)
    }
}
#[async_trait]

impl AIAgentApi for OllamaAIAgentApi {
    // chat_history を更新するため、&mut self を使用します

    async fn get_ai_response(&mut self, user_input: &str) -> Result<String, Box<dyn Error>> {
        // ユーザーメッセージを履歴に追加

        self.chat_history.push(Message {
            role: "user".to_string(),

            content: user_input.to_string(),
        });

        let request_body = OllamaChatRequest {
            model: self.model_name.clone(),

            messages: self.chat_history.clone(), // 現在の履歴をリクエストに含める

            stream: true, // ストリーミングを有効化
        };

        let request_url = format!("{}/v1/chat/completions", self.ollama_url);

        // Ollama APIへのPOSTリクエストを送信

        let response = self
            .client
            .post(&request_url)
            .json(&request_body)
            .send()
            .await?;

        // HTTPステータスコードをチェックし、エラーの場合は詳細なメッセージを返す

        if !response.status().is_success() {
            let status = response.status();

            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to get response body".to_string());

            return Err(format!("Ollama APIリクエストが失敗しました。ステータス: {}, ボディ: {}. Ollamaサーバーが {} で実行されており、モデル '{}' が利用可能であることを確認してください。", status, text, self.ollama_url, self.model_name).into());
        }

        // `reqwest::Error` を `std::io::Error` に変換するヘルパー関数

        fn reqwest_error_to_io_error(e: reqwest::Error) -> std::io::Error {
            std::io::Error::other(e)
        }

        // レスポンスストリームを処理

        // `response.bytes_stream()` は `impl Stream<Item = Result<Bytes, reqwest::Error>>` を返す

        // これを `tokio_util::io::StreamReader` に渡すためには、エラー型を `std::io::Error` に変換する必要がある

        let byte_stream = response.bytes_stream().map_err(reqwest_error_to_io_error);

        // `StreamReader` を使用して `AsyncRead` を実装するリーダを作成

        let mut reader = BufReader::new(StreamReader::new(byte_stream));

        let mut full_response_content = String::new();

        let mut buffer = String::new();

        loop {
            buffer.clear();

            let bytes_read = reader.read_line(&mut buffer).await?;

            if bytes_read == 0 {
                break; // ストリーム終了
            }

            let line_content = buffer.trim();

            if line_content.is_empty() {
                continue; // 空行をスキップ
            }

            // ストリームから受信したJSON行をパース

            // Ollamaのストリーミングは通常、各行が完全なJSONオブジェクトだが、

            // 空の行や不完全なJSON行を扱うためにエラーハンドリングを含める

            match serde_json::from_str::<OllamaStreamResponse>(line_content) {
                Ok(stream_response) => {
                    // 最初の選択肢からメッセージコンテンツを取得

                    if let Some(choice) = stream_response.choices.into_iter().next() {
                        full_response_content.push_str(&choice.message.content);
                    }

                    // `done: true` が来たらストリームを終了

                    if stream_response.done == Some(true) {
                        break;
                    }
                }

                Err(e) => {
                    // JSONパースエラーは、ストリームの末尾の不完全なJSON行などで発生する可能性があります。

                    // ログに記録しますが、処理を継続します。

                    eprintln!(
                        "OllamaストリームからのJSON行のパースに失敗しました: {:?}, 行: '{}'",
                        e, line_content
                    );

                    continue;
                }
            }
        }

        // アシスタントの最終応答をチャット履歴に追加

        self.chat_history.push(Message {
            role: "assistant".to_string(),

            content: full_response_content.clone(),
        });

        Ok(full_response_content)
    }
}
