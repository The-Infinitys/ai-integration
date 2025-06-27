// src/modules/chat/api.rs

use async_trait::async_trait;
use futures_util::TryStreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::io::{self, Write};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::io::StreamReader;
use chrono::{Local, DateTime};
use tokio::fs;
use std::path::Path;
// html2md と urlencoding は search.rs に移動したため、ここでは不要（もし他で使っていなければ）
// use html2md::parse_html;
// use urlencoding;
use regex::Regex;
use crate::dprintln; // src/lib.rs または src/main.rs で定義されたマクロをインポート

// 新しいsearchサブモジュールを宣言
pub mod search;

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
    #[serde(default)]
    choices: Vec<StreamChoice>,
    done: Option<bool>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Deserialize, Default)]
struct StreamDelta {
    #[serde(default)]
    content: String,
}

/// `OllamaAIAgentApi`は`AIAgentApi`トレイトのOllama実装です。
/// OllamaサーバーとHTTPで通信し、AIの応答を取得します。
pub struct OllamaAIAgentApi {
    client: Client,
    ollama_url: String,
    model_name: String,
    chat_history: Vec<Message>, // 会話履歴を保持
    tool_call_regex: Regex,     // ツール呼び出しを検出するための正規表現
    debug_mode: bool,           // デバッグモードを管理するフラグ
}

impl OllamaAIAgentApi {
    /// 新しい`OllamaAIAgentApi`のインスタンスを作成します。
    ///
    /// # 引数
    /// * `ollama_url` - OllamaサーバーのURL (例: "http://localhost:11434")。
    /// * `model_name` - 使用するOllamaモデルの名前 (例: "llama2")。
    /// * `debug_mode` - デバッグ出力を有効にするかどうか。
    pub fn new(ollama_url: String, model_name: String, debug_mode: bool) -> Self {
        let tool_call_regex = Regex::new(r"<tool_code>(.*?)</tool_code>").unwrap();

        OllamaAIAgentApi {
            client: Client::new(),
            ollama_url,
            model_name,
            chat_history: vec![Message {
                role: "system".to_string(),
                content: r#"あなたは便利なAIアシスタントです。
現在の場所は丸亀市、香川県、日本です。
Web検索やURLへのアクセスが必要な場合は、以下の形式でツールを呼び出してください:
- **Web検索**: `<tool_code>web_search(query='検索クエリ', engine='google')</tool_code>`
  `query`は必須、`engine`はオプションで`google`, `bing`, `duckduckgo`から選択可能です。
- **URLアクセス**: `<tool_code>web_search(url='https://example.com')</tool_code>`
  `url`は直接アクセスするURLです。`url`が指定された場合、`query`は無視されます。
ツールを呼び出した後は、追加の思考をせず、単にツール呼び出しコードを出力してください。
ツールの結果を受け取った後で、その結果に基づいてユーザーに応答してください。"#
                    .to_string(),
            }],
            tool_call_regex,
            debug_mode,
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
                Ok(ft) if ft.is_file() => "ファイル",
                Ok(ft) if ft.is_dir() => "ディレクトリ",
                _ => "その他",
            };
            file_info.push_str(&format!(
                "- {}: {}\n",
                type_str,
                file_name.to_string_lossy()
            ));
        }
        file_info
    }
    
    // ヘルパー関数: ストリーム応答からテキストコンテンツを抽出して表示し、ツール呼び出しを検出
    async fn process_stream_and_get_content(
        &self,
        response: reqwest::Response,
    ) -> Result<(String, Option<String>), Box<dyn Error>> {
        fn reqwest_error_to_io_error(e: reqwest::Error) -> std::io::Error {
            io::Error::other(e)
        }

        let byte_stream = response.bytes_stream().map_err(reqwest_error_to_io_error);
        let mut reader = BufReader::new(StreamReader::new(byte_stream));

        let mut full_response_content = String::new();
        let mut tool_code_detected: Option<String> = None;
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
                        if !choice.delta.content.is_empty() {
                            print!("{}", choice.delta.content);
                            io::stdout().flush()?;
                            full_response_content.push_str(&choice.delta.content);
                        }
                    }
                    if stream_response.done == Some(true) {
                        break;
                    }
                }
                Err(e) => {
                    dprintln!(
                        self.debug_mode,
                        "OllamaストリームからのJSON行のパースに失敗しました: {:?}, 行: '{}'",
                        e,
                        line_content
                    );
                    continue;
                }
            }
        }

        // 全コンテンツが揃った後で、ツール呼び出しの正規表現を適用
        if let Some(captures) = self.tool_call_regex.captures(&full_response_content) {
            if let Some(tool_code) = captures.get(1) {
                tool_code_detected = Some(tool_code.as_str().to_string());
            }
        }

        Ok((full_response_content, tool_code_detected))
    }
}

impl Default for OllamaAIAgentApi {
    fn default() -> Self {
        // `AI_AGENT_DEBUG` 環境変数をチェックしてデバッグモードを設定
        let debug_mode = std::env::var("AI_AGENT_DEBUG").unwrap_or_default() == "true";

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
        Self::new("http://localhost:11434".to_string(), model_name, debug_mode)
    }
}

#[async_trait]
impl AIAgentApi for OllamaAIAgentApi {
    async fn get_ai_response(&mut self, user_input: &str) -> Result<String, Box<dyn Error>> {
        // 現在のコンテキスト情報を取得
        let current_datetime = OllamaAIAgentApi::get_current_datetime();
        let current_dir_path = std::env::current_dir().unwrap_or_default();
        let file_status = Self::get_file_status(&current_dir_path.to_string_lossy()).await;

        // システムメッセージに現在のコンテキストを追加
        // chat_historyの最後のメッセージがシステムメッセージであると仮定して、そのcontentを更新
        if let Some(system_msg) = self.chat_history.iter_mut().find(|m| m.role == "system") {
            system_msg.content = format!(
                r#"あなたは便利なAIアシスタントです。
現在の場所は丸亀市、香川県、日本です。
Web検索やURLへのアクセスが必要な場合は、以下の形式でツールを呼び出してください:
- **Web検索**: `<tool_code>web_search(query='検索クエリ', engine='google')</tool_code>`
  `query`は必須、`engine`はオプションで`google`, `bing`, `duckduckgo`から選択可能です。
- **URLアクセス**: `<tool_code>web_search(url='https://example.com')</tool_code>`
  `url`は直接アクセスするURLです。`url`が指定された場合、`query`は無視されます。
ツールを呼び出した後は、追加の思考をせず、単にツール呼び出しコードを出力してください。
ツールの結果を受け取った後で、その結果に基づいてユーザーに応答してください。
---
現在の状況:
日時: {}
現在のディレクトリの内容:
{}
"#,
                current_datetime, file_status
            );
        } else {
            self.chat_history.push(Message {
                role: "system".to_string(),
                content: format!(
                    "現在の状況: 日時: {}。現在のディレクトリの内容:\n{}",
                    current_datetime, file_status
                ),
            });
        }

        // ユーザーメッセージを履歴に追加
        self.chat_history.push(Message {
            role: "user".to_string(),
            content: user_input.to_string(),
        });

        // ===== 1回目のOllama API呼び出し: AIがツール呼び出しを生成するかどうか =====
        dprintln!(
            self.debug_mode,
            "\n[AI (システム): AIが応答を生成中... (ツール呼び出しの可能性あり)]"
        );
        io::stdout().flush()?;

        let request_body_first = OllamaChatRequest {
            model: self.model_name.clone(),
            messages: self.chat_history.clone(),
            stream: true,
        };

        dprintln!(
            self.debug_mode,
            "DEBUG: Request Body (1st call):\n{}",
            serde_json::to_string_pretty(&request_body_first)?
        );
        io::stdout().flush()?;

        let request_url = format!("{}/v1/chat/completions", self.ollama_url);

        let response_first = self
            .client
            .post(&request_url)
            .json(&request_body_first)
            .send()
            .await?;

        if !response_first.status().is_success() {
            let status = response_first.status();
            let text = response_first
                .text()
                .await
                .unwrap_or_else(|_| "Failed to get response body".to_string());
            dprintln!(
                self.debug_mode,
                "DEBUG: Failed Request Body (1st call):\n{}",
                serde_json::to_string_pretty(&request_body_first).unwrap_or_default()
            );
            return Err(format!("Ollama APIリクエスト (1回目) が失敗しました。ステータス: {}, ボディ: {}. Ollamaサーバーが {} で実行されており、モデル '{}' が利用可能であることを確認してください。", status, text, self.ollama_url, self.model_name).into());
        }

        let (mut assistant_response_content, tool_code_detected) =
            self.process_stream_and_get_content(response_first).await?;

        // ツール呼び出しがあった場合
        if let Some(tool_code) = tool_code_detected {
            dprintln!(
                self.debug_mode,
                "\n[AI (システム): ツール呼び出しを検出しました: {}]",
                tool_code
            );
            io::stdout().flush()?;

            // AIが生成したツール呼び出しコードをアシスタントメッセージとして履歴に追加
            // AIからの最終的な応答は、このツール呼び出しコードで終わるはず
            self.chat_history.push(Message {
                role: "assistant".to_string(),
                content: assistant_response_content.clone(), // ツールコードを含む応答全体
            });

            let tool_result: String;
            let parts: Vec<&str> = tool_code.splitn(2, '(').collect();
            if parts.len() == 2 && parts[1].ends_with(')') {
                let func_name = parts[0];
                let args_str = &parts[1][..parts[1].len() - 1]; // Remove closing parenthesis

                match func_name {
                    "web_search" => {
                        let mut query: Option<&str> = None;
                        let mut url: Option<&str> = None;
                        let mut engine: Option<&str> = None;

                        // 引数文字列をパース
                        for arg_pair in args_str.split(',') {
                            let key_value: Vec<&str> = arg_pair.trim().splitn(2, '=').collect();
                            if key_value.len() == 2 {
                                let key = key_value[0].trim();
                                let value = key_value[1].trim().trim_matches('\''); // Remove single quotes
                                match key {
                                    "query" => query = Some(value),
                                    "url" => url = Some(value),
                                    "engine" => engine = Some(value),
                                    _ => {
                                        dprintln!(
                                            self.debug_mode,
                                            "警告: 不明な引数 '{}' を検出しました。",
                                            key
                                        );
                                    }
                                }
                            }
                        }

                        if query.is_some() || url.is_some() {
                            // searchモジュールのexecute_web_searchを呼び出す
                            tool_result = search::execute_web_search(&self.client, self.debug_mode, query, url, engine).await?;
                        } else {
                            tool_result = "エラー: web_searchツールには 'query' または 'url' のいずれかが必要です。".to_string();
                        }
                    }
                    _ => {
                        tool_result = format!("不明なツール: {}", func_name);
                    }
                }
            } else {
                tool_result = format!("エラー: 不正なツール呼び出し形式: {}", tool_code);
            }

            dprintln!(
                self.debug_mode,
                "[AI (システム): ツール実行結果: {}]",
                tool_result
            );
            io::stdout().flush()?;

            // ツールの実行結果をユーザーメッセージとして履歴に追加し、AIにフィードバック
            self.chat_history.push(Message {
                role: "user".to_string(), // AIにツール結果を「ユーザーからの情報」として提供
                content: format!(
                    "ユーザー: ツール実行結果を以下に示します。\n{}",
                    tool_result
                ),
            });

            // ===== 2回目のOllama API呼び出し: ツール実行結果に基づいて最終応答を生成 =====
            dprintln!(
                self.debug_mode,
                "[AI (システム): ツール結果に基づいて最終応答を生成中...]"
            );
            io::stdout().flush()?;

            let request_body_second = OllamaChatRequest {
                model: self.model_name.clone(),
                messages: self.chat_history.clone(), // ツールの結果も含む
                stream: true,
            };

            dprintln!(
                self.debug_mode,
                "DEBUG: Request Body (2nd call):\n{}",
                serde_json::to_string_pretty(&request_body_second)?
            );
            io::stdout().flush()?;

            let response_second = self
                .client
                .post(&request_url)
                .json(&request_body_second)
                .send()
                .await?;

            if !response_second.status().is_success() {
                let status = response_second.status();
                let text = response_second
                    .text()
                    .await
                    .unwrap_or_else(|_| "Failed to get response body".to_string());
                dprintln!(
                    self.debug_mode,
                    "DEBUG: Failed Request Body (2nd call):\n{}",
                    serde_json::to_string_pretty(&request_body_second).unwrap_or_default()
                );
                return Err(format!("Ollama APIリクエスト (2回目) が失敗しました。ステータス: {}, ボディ: {}. Ollamaサーバーが {} で実行されており、モデル '{}' が利用可能であることを確認してください。", status, text, self.ollama_url, self.model_name).into());
            }

            let (final_assistant_response, _) =
                self.process_stream_and_get_content(response_second).await?;
            assistant_response_content = final_assistant_response;
        } else {
            // ツール呼び出しがなかった場合、1回目の応答が最終応答となる
        }

        println!(); // 最終応答の後に改行

        // アシスタントの最終応答をチャット履歴に追加
        self.chat_history.push(Message {
            role: "assistant".to_string(),
            content: assistant_response_content.clone(),
        });

        Ok(assistant_response_content)
    }
}
