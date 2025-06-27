// src/modules/chat/api.rs

use async_trait::async_trait;
use futures_util::TryStreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value; // For handling JSON objects for tool arguments and results
use std::error::Error;
use std::io::{self, Write};
use tokio::io::{AsyncBufReadExt, BufReader}; // For streaming
use tokio_util::io::StreamReader; // To convert reqwest stream to tokio's AsyncRead
use chrono::{Local, DateTime}; // For current date and time
use tokio::fs; // For async file system operations
use std::path::Path; // For path manipulation

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
    #[serde(skip_serializing_if = "Option::is_none")] // toolsがない場合はシリアライズしない
    tools: Option<Vec<Tool>>,
    stream: bool,
}

// チャットメッセージの構造体（ロールとコンテンツを含む）
#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)] // JSONの構造に応じてデシリアライズする
enum Message {
    UserAssistantSystem {
        role: String,
        content: String,
    },
    // AIがツールを呼び出すときに使用するメッセージタイプ
    ToolCall {
        role: String,
        #[serde(default)]
        tool_calls: Vec<ToolCall>,
    },
    // ツールの実行結果をAIにフィードバックするときに使用するメッセージタイプ
    ToolResult {
        role: String,
        tool_call_id: String, // どのツール呼び出しに対応するか
        content: String,      // ツールの実行結果
    },
}

// AIが利用できるツールの定義
#[derive(Serialize, Deserialize, Clone)]
struct Tool {
    #[serde(rename = "type")]
    tool_type: String, // "function"
    function: Function,
}

// ツールの関数の定義（名前とパラメータスキーマ）
#[derive(Serialize, Deserialize, Clone)]
struct Function {
    name: String,
    description: String,
    parameters: Value, // JSON Schema object
}

// AIがツールを呼び出すことを決定したときに返される構造体
#[derive(Serialize, Deserialize, Clone, Debug)]
struct ToolCall {
    id: String, // ツール呼び出しの一意のID
    #[serde(rename = "type")]
    tool_type: String, // "function"
    function: ToolFunctionCall,
}

// ツール呼び出し内で関数とその引数を定義する構造体
#[derive(Serialize, Deserialize, Clone, Debug)]
struct ToolFunctionCall {
    name: String,
    arguments: Value, // JSON object of arguments
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
    delta: StreamDelta, // 差分コンテンツとツール呼び出しを含む
    #[allow(dead_code)]
    index: Option<u32>,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

// ストリーミングレスポンスの`delta`部分。コンテンツとツール呼び出しのどちらか、または両方を含む。
#[derive(Deserialize, Default)]
struct StreamDelta {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Vec<ToolCall>,
    #[serde(default)]
    #[allow(dead_code)]
    role: String, // "assistant"など、deltaのロール
}


/// `OllamaAIAgentApi`は`AIAgentApi`トレイトのOllama実装です。
/// OllamaサーバーとHTTPで通信し、AIの応答を取得します。
pub struct OllamaAIAgentApi {
    client: Client,
    ollama_url: String,
    model_name: String,
    chat_history: Vec<Message>, // 会話履歴を保持
    available_tools: Vec<Tool>, // AIに提供する利用可能なツール
}

impl OllamaAIAgentApi {
    /// 新しい`OllamaAIAgentApi`のインスタンスを作成します。
    ///
    /// # 引数
    /// * `ollama_url` - OllamaサーバーのURL (例: "http://localhost:11434")。
    /// * `model_name` - 使用するOllamaモデルの名前 (例: "llama2")。
    pub fn new(ollama_url: String, model_name: String) -> Self {
        let web_search_tool = Tool {
            tool_type: "function".to_string(),
            function: Function {
                name: "web_search".to_string(),
                description: "指定されたクエリでWebを検索し、結果を返します。".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "検索するキーワードまたはフレーズ。"
                        },
                        "engine": {
                            "type": "string",
                            "description": "使用する検索エンジン（例: google, bing, duckduckgo）。デフォルトはgoogle。",
                            "enum": ["google", "bing", "duckduckgo"]
                        }
                    },
                    "required": ["query"]
                }),
            },
        };

        OllamaAIAgentApi {
            client: Client::new(),
            ollama_url,
            model_name,
            chat_history: vec![Message::UserAssistantSystem {
                role: "system".to_string(),
                content: "あなたは便利なAIアシスタントです。現在の場所は丸亀市、香川県、日本です。Web検索が必要な場合は、web_searchツールを使用してください。".to_string(),
            }],
            available_tools: vec![web_search_tool],
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
            file_info.push_str(&format!("- {}: {}\n", type_str, file_name.to_string_lossy()));
        }
        file_info
    }

    // 新しいヘルパー関数: ダミーのWeb検索を実行
    async fn execute_web_search(&self, query: &str, engine: Option<&str>) -> String {
        let used_engine = engine.unwrap_or("google");
        println!("\n[AI (ツール): Web検索を実行中... クエリ: '{}', エンジン: '{}']", query, used_engine);
        io::stdout().flush().unwrap_or_default();

        // ここに実際のWeb検索ロジック（例: 外部API呼び出し）が入ります。
        // 今はダミーの結果を返します。
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await; // 検索の遅延をシミュレート

        format!(
            "検索結果 (エンジン: {}): 「{}」\n\
            1. Rustプログラミング入門 - URL: https://example.com/rust-intro\n\
            2. 最新AI技術の動向 - URL: https://example.com/latest-ai\n\
            3. 香川県丸亀市の観光情報 - URL: https://example.com/marugame-tourism\n\
            上記のWeb検索結果はシミュレーションです。",
            used_engine, query
        )
    }

    // ヘルパー関数: ストリーム応答からテキストコンテンツを抽出して表示
    async fn process_stream_and_get_content(
        &self,
        response: reqwest::Response,
    ) -> Result<(String, Option<ToolCall>), Box<dyn Error>> {
        fn reqwest_error_to_io_error(e: reqwest::Error) -> std::io::Error {
            io::Error::other(e)
        }

        let byte_stream = response.bytes_stream().map_err(reqwest_error_to_io_error);
        let mut reader = BufReader::new(StreamReader::new(byte_stream));

        let mut full_response_content = String::new();
        let mut assistant_tool_call: Option<ToolCall> = None;
        let mut buffer = String::new();

        loop {
            buffer.clear();
            let bytes_read = reader.read_line(&mut buffer).await?;
            if bytes_read == 0 {
                break; // End of stream
            }

            let line_content = buffer.trim();
            if line_content.is_empty() {
                continue; // Skip empty lines
            }

            let json_str = if line_content.starts_with("data: ") {
                &line_content[6..] // Skip "data: "
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
                            // AIからのテキストコンテンツを表示
                            print!("{}", choice.delta.content);
                            io::stdout().flush()?;
                            full_response_content.push_str(&choice.delta.content);
                        }
                        
                        // ツール呼び出しがある場合、これをキャプチャ
                        if !choice.delta.tool_calls.is_empty() {
                            // 現時点では最初のツール呼び出しのみを処理（Ollamaのdeltaでtool_callsが複数来ることは稀）
                            if let Some(tc) = choice.delta.tool_calls.into_iter().next() {
                                assistant_tool_call = Some(tc);
                            }
                        }
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
                    // 不完全なJSON行をスキップするが、エラーを報告
                    // ストリームが正しく終了しない場合の最後の行などで発生しやすい
                    continue;
                }
            }
        }
        Ok((full_response_content, assistant_tool_call))
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
                .nth(1) // Get the second line (after the header)
                .and_then(|line| line.split_whitespace().next()) // Get the first word (model name)
                .unwrap_or("llama2"); // Default to llama2 if unable to get
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
        let current_dir = ".".to_string();
        let file_status = Self::get_file_status(&current_dir).await;

        // システムメッセージに現在のコンテキストを追加
        // このメッセージは、各リクエストのたびに最新の情報を提供します。
        self.chat_history.push(Message::UserAssistantSystem {
            role: "system".to_string(),
            content: format!(
                "現在の状況: 日時: {}。現在のディレクトリの内容:\n{}",
                current_datetime, file_status
            ),
        });

        // ユーザーメッセージを履歴に追加
        self.chat_history.push(Message::UserAssistantSystem {
            role: "user".to_string(),
            content: user_input.to_string(),
        });

        // ===== 1回目のOllama API呼び出し: AIがツール呼び出しを決定するかどうか =====
        println!("[AI (システム): AIが応答を生成中... (ツール呼び出しの可能性あり)]");
        io::stdout().flush()?;

        let request_body_first = OllamaChatRequest {
            model: self.model_name.clone(),
            messages: self.chat_history.clone(),
            tools: Some(self.available_tools.clone()), // 利用可能なツールをAIに教える
            stream: true,
        };

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
            return Err(format!("Ollama APIリクエスト (1回目) が失敗しました。ステータス: {}, ボディ: {}. Ollamaサーバーが {} で実行されており、モデル '{}' が利用可能であることを確認してください。", status, text, self.ollama_url, self.model_name).into());
        }

        let (mut assistant_response_content, assistant_tool_call) =
            self.process_stream_and_get_content(response_first).await?;

        // ツール呼び出しがあった場合
        if let Some(tool_call) = assistant_tool_call {
            println!("\n[AI (システム): ツール呼び出しを検出しました: {:?}]", tool_call);
            io::stdout().flush()?;

            // アシスタントのツール呼び出しメッセージを履歴に追加
            self.chat_history.push(Message::ToolCall {
                role: "assistant".to_string(),
                tool_calls: vec![tool_call.clone()], // クローンして履歴に残す
            });

            let tool_result: String;
            match tool_call.function.name.as_str() {
                "web_search" => {
                    // `query` 引数を抽出
                    let query = tool_call.function.arguments["query"]
                        .as_str()
                        .unwrap_or_default();
                    let engine = tool_call.function.arguments["engine"]
                        .as_str(); // `engine`はオプション
                    tool_result = self.execute_web_search(query, engine).await;
                }
                _ => {
                    tool_result = format!("不明なツール: {}", tool_call.function.name);
                }
            }

            println!("[AI (システム): ツール実行結果: {}]", tool_result);
            io::stdout().flush()?;

            // ツールの実行結果を履歴に追加
            self.chat_history.push(Message::ToolResult {
                role: "tool".to_string(),
                tool_call_id: tool_call.id.clone(), // どのツール呼び出しに対応するか
                content: tool_result,
            });

            // ===== 2回目のOllama API呼び出し: ツール実行結果に基づいて最終応答を生成 =====
            println!("[AI (システム): ツール結果に基づいて最終応答を生成中...]");
            io::stdout().flush()?;

            let request_body_second = OllamaChatRequest {
                model: self.model_name.clone(),
                messages: self.chat_history.clone(), // ツールの結果も含む
                tools: Some(self.available_tools.clone()), // 再度ツール定義を含める
                stream: true,
            };

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
                return Err(format!("Ollama APIリクエスト (2回目) が失敗しました。ステータス: {}, ボディ: {}. Ollamaサーバーが {} で実行されており、モデル '{}' が利用可能であることを確認してください。", status, text, self.ollama_url, self.model_name).into());
            }

            let (final_assistant_response, _) =
                self.process_stream_and_get_content(response_second).await?;
            assistant_response_content = final_assistant_response; // 最終応答を更新

        } else {
            // ツール呼び出しがなかった場合、1回目の応答が最終応答となる
            // 何もしない（assistant_response_contentは既に設定されている）
        }

        println!(); // 最終応答の後に改行

        // アシスタントの最終応答をチャット履歴に追加
        self.chat_history.push(Message::UserAssistantSystem {
            role: "assistant".to_string(),
            content: assistant_response_content.clone(),
        });

        Ok(assistant_response_content)
    }
}
