// src/main.rs
use regex::Regex;
use std::collections::HashMap;
use std::fmt::{self, Display};
use std::env; // コマンドライン引数を読み込むために必要
use std::error::Error;
use std::fs;
use std::pin::Pin;
use tokio;

// 新しいモジュールをインポート
mod ai;
mod config;
mod help;
mod prompt;

use ai::{AIGenerator, GeminiChat, OllamaChat, OpenAIChat};
use config::Config;

// --- Custom Error Type for String Messages ---
#[derive(Debug)]
struct CustomError(String);

impl Display for CustomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for CustomError {}

// --- End Custom Error Type ---

// --- 1. データ構造の定義 ---

// スクリプトが最終的に持つ値
#[derive(Debug, Clone, PartialEq)]
enum Value {
    String(String),
    Null,
}

// Displayトレイトを実装して、Printで簡単に出力できるようにする
impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::String(s) => write!(f, "{}", s),
            Value::Null => write!(f, "null"),
        }
    }
}

// 式 (評価するとValueになるもの)
#[derive(Debug, Clone, PartialEq)]
enum Expression {
    LiteralString(String),     // "文字列" のようなリテラル
    Variable(String),          // 変数名
    ReadFile(Box<Expression>), // ファイルパスも式で評価できる (例: Read file "path/to/file.txt")
    // 新しいAI関連の式を追加
    GenerateContent(Box<Expression>, Box<Expression>), // Generate content from <provider> with prompt <expression>
}

// 文 (実行される各行の命令)
#[derive(Debug, Clone, PartialEq)]
enum Statement {
    Let(String, Expression), // let a = ...
    Print(Expression),       // Print ...
}

// --- 2. パーサーの実装 ---

// "..." または 変数名 をパースする
fn parse_expression(input: &str) -> Result<Expression, Box<dyn Error>> {
    let input = input.trim();

    // Read file "..." の構文をチェック
    let read_file_re = Regex::new(r#"^Read file (.+)$"#)?;
    if let Some(caps) = read_file_re.captures(input) {
        // Read file の後の式を再帰的にパース
        let path_expr_str = caps.get(1).unwrap().as_str();
        let path_expr = parse_expression(path_expr_str)?;
        return Ok(Expression::ReadFile(Box::new(path_expr)));
    }

    // Generate content from <provider> with prompt <expression> の構文をチェック
    let generate_content_re =
        Regex::new(r#"^Generate content from\s+(.+?)\s+with prompt\s+(.+)$"#)?;
    if let Some(caps) = generate_content_re.captures(input) {
        let provider_expr_str = caps.get(1).unwrap().as_str();
        let prompt_expr_str = caps.get(2).unwrap().as_str();
        let provider_expr = parse_expression(provider_expr_str)?;
        let prompt_expr = parse_expression(prompt_expr_str)?;
        return Ok(Expression::GenerateContent(
            Box::new(provider_expr),
            Box::new(prompt_expr),
        ));
    }

    // "..." (リテラル文字列) の構文をチェック
    let literal_re = Regex::new(r#"^"([^"]*)"$"#)?;
    if let Some(caps) = literal_re.captures(input) {
        let value = caps.get(1).unwrap().as_str().to_string();
        return Ok(Expression::LiteralString(value));
    }

    // どれにも当てはまらなければ変数名とみなす
    Ok(Expression::Variable(input.to_string()))
}

// スクリプト全体をパースして、文のリストを返す
fn parse(script: &str) -> Result<Vec<Statement>, Box<dyn Error>> {
    let mut statements = Vec::new();

    // 正規表現の準備
    let let_re = Regex::new(r"^\s*let\s+([a-zA-Z0-9_]+)\s*=\s*(.+)$")?;
    let print_re = Regex::new(r"^\s*Print\s+(.+)$")?;

    for line in script.lines() {
        if line.trim().is_empty() || line.trim().starts_with("//") {
            continue; // 空行とコメントはスキップ
        }

        if let Some(caps) = let_re.captures(line) {
            let var_name = caps.get(1).unwrap().as_str().to_string();
            let expr_str = caps.get(2).unwrap().as_str();
            let expression = parse_expression(expr_str)?;
            statements.push(Statement::Let(var_name, expression));
        } else if let Some(caps) = print_re.captures(line) {
            let expr_str = caps.get(1).unwrap().as_str();
            let expression = parse_expression(expr_str)?;
            statements.push(Statement::Print(expression));
        } else {
            return Err(Box::new(CustomError(format!("無効な構文: {}", line))) as Box<dyn Error>);
        }
    }

    Ok(statements)
}

// --- 3. 実行エンジンの実装 ---
struct Executor {
    variables: HashMap<String, Value>,
    // AIジェネレータを動的に保持するためにBox<dyn AIGenerator>を使用
    // ConfigをExecutorに渡し、必要に応じてジェネレータを初期化する
    config: Config, // Configインスタンスを保持
    openai_generator: Option<Box<dyn AIGenerator>>,
    ollama_generator: Option<Box<dyn AIGenerator>>,
    gemini_generator: Option<Box<dyn AIGenerator>>,
}

impl Executor {
    fn new(config: Config) -> Self {
        Executor {
            variables: HashMap::new(),
            config, // Configを初期化
            openai_generator: None,
            ollama_generator: None,
            gemini_generator: None,
        }
    }

    // 式を評価してValueに変換する
    async fn evaluate(&mut self, expr: &Expression) -> Result<Value, Box<dyn Error>> {
        match expr {
            Expression::LiteralString(s) => Ok(Value::String(s.clone())),
            Expression::Variable(name) => self
                .variables
                .get(name)
                .cloned()
                .ok_or_else(|| Box::new(CustomError(format!("変数 '{}' が見つかりません", name))) as Box<dyn Error>),
            Expression::ReadFile(path_expr) => {
                // ファイルパスを表す式をまず評価する
                let path_val = Box::pin(self.evaluate(path_expr)).await?;
                if let Value::String(path) = path_val {
                    // std::io::Error は Error トレイトを実装しているので .into() で Box<dyn Error> に変換できる
                    // e.into() は型推論があいまいになる可能性があるため、明示的にBox化します
                    let content = fs::read_to_string(&path).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                    Ok(Value::String(content))
                } else {
                    Err(Box::new(CustomError("ファイルパスは文字列である必要があります".to_string())) as Box<dyn Error>)
                }
            }
            Expression::GenerateContent(provider_expr, prompt_expr) => {
                let provider_name = match Box::pin(self.evaluate(provider_expr)).await? {
                    Value::String(s) => s.to_lowercase(),
                    _ => return Err(Box::new(CustomError("AIプロバイダー名は文字列である必要があります".to_string())) as Box<dyn Error>),
                };
                let prompt_text = match Box::pin(self.evaluate(prompt_expr)).await? {
                    Value::String(s) => s,
                    _ => return Err(Box::new(CustomError("プロンプトは文字列である必要があります".to_string())) as Box<dyn Error>),
                };

                // プロバイダー名に基づいて適切なジェネレータを選択
                let generator: &dyn AIGenerator = match provider_name.as_str() {
                    "openai" => {
                        if self.openai_generator.is_none() {
                            // OpenAIクライアントを必要に応じて初期化 (configを渡す)
                            // OpenAIChat::new は Box<dyn Error> を返すので、そのまま ? で伝播
                            self.openai_generator = Some(Box::new(OpenAIChat::new(&self.config)?));
                        }
                        self.openai_generator.as_ref().unwrap().as_ref()
                    }
                    "ollama" => {
                        if self.ollama_generator.is_none() {
                            // Ollamaクライアントを必要に応じて初期化 (configを渡す)
                            self.ollama_generator = Some(Box::new(OllamaChat::new(&self.config)));
                        }
                        self.ollama_generator.as_ref().unwrap().as_ref()
                    }
                    "gemini" => {
                        if self.gemini_generator.is_none() {
                            // Geminiクライアントを必要に応じて初期化 (configを渡す)
                            // GeminiChat::new は Box<dyn Error> を返すので、そのまま ? で伝播
                            self.gemini_generator = Some(Box::new(GeminiChat::new(&self.config)?));
                        }
                        self.gemini_generator.as_ref().unwrap().as_ref()
                    }
                    _ => return Err(Box::new(CustomError(format!("不明なAIプロバイダー: {}", provider_name))) as Box<dyn Error>),
                };

                let generated_text = generator.generate_content(&prompt_text).await?;
                Ok(Value::String(generated_text))
            }
        }
    }
    // 文を実行する
    async fn run(&mut self, statements: &[Statement]) -> Result<(), Box<dyn Error>> {
        for stmt in statements {
            match stmt {
                Statement::Let(name, expr) => { // evaluate のエラー型が Box<dyn Error> になったので map_err は不要
                    let value = self.evaluate(expr).await?;
                    self.variables.insert(name.clone(), value);
                }
                Statement::Print(expr) => { // evaluate のエラー型が Box<dyn Error> になったので map_err は不要
                    let value = self.evaluate(expr).await?;
                    println!("{}", value);
                }
            }
        }
        Ok(())
    }
}

// --- 4. メイン関数の実装 ---
#[tokio::main] // main関数を非同期にする
async fn main() {
    let args: Vec<String> = env::args().collect();

    // 設定を初期化 (APIキーがハードコードされている警告が表示されます)
    let config = Config::new();

    match args.get(1).map(|s| s.as_str()) {
        Some("help") => {
            help::display_help();
        }
        Some("prompt") => {
            if let Some(provider_name) = args.get(2) {
                let generator: Pin<Box<dyn AIGenerator>> = match provider_name.as_str() {
                    "openai" => Box::pin(OpenAIChat::new(&config).expect("OpenAIクライアントの初期化に失敗しました")),
                    "ollama" => Box::pin(OllamaChat::new(&config)),
                    "gemini" => Box::pin(GeminiChat::new(&config).expect("Geminiクライアントの初期化に失敗しました")),
                    _ => {
                        // エラーメッセージは Box<dyn Error> を返すので、それを Box::pin で Pin<Box<dyn Error>> に変換
                        // ただし、ここでは eprintln! で直接出力して return しているため、Result に変換する必要はない
                        // そのため、このブロックは変更なし
                        // (もし Result を返すなら Box::pin(Box::<dyn Error>::from(...)) となる)
                        eprintln!(
                            "エラー: 不明なAIプロバイダー '{}' です。'openai', 'ollama', 'gemini' のいずれかを指定してください。",
                            provider_name
                        );
                        return;
                    }
                };
                if let Err(e) = prompt::start_chat_loop(generator).await { // generator は既に Pin<Box<dyn AIGenerator>> なので .into() は不要
                    eprintln!("チャットエラー: {}", e);
                }
            } else {
                eprintln!(
                    "エラー: 'prompt' コマンドにはAIプロバイダーを指定する必要があります (例: cargo run -- prompt openai)"
                );
                help::display_help();
            }
        }
        _ => {
            // デフォルトのスクリプト実行
            let script = r#"
                // AuraScript Engine Prototype
                
                let greeting = "こんにちは、AuraScript！"
                Print greeting
                
                let filename = "hello.txt"
                let file_content = Read file filename
                
                Print "--- ファイルの内容 ---"
                Print file_content

                // AIによるコンテンツ生成の例
                // AIサービスが利用可能であることを確認してください (例: Ollamaがローカルで実行中)
                
                let openai_provider = "openai"
                let openai_prompt = "Rust言語について簡潔に説明してください。"
                let openai_response = Generate content from openai_provider with prompt openai_prompt
                Print "--- OpenAIからの応答 ---"
                Print openai_response

                let ollama_provider = "ollama"
                let ollama_prompt = "Ollamaとは何ですか？"
                let ollama_response = Generate content from ollama_provider with prompt ollama_prompt
                Print "--- Ollamaからの応答 ---"
                Print ollama_response

                let gemini_provider = "gemini"
                let gemini_prompt = "Google Geminiについて説明してください。"
                let gemini_response = Generate content from gemini_provider with prompt gemini_prompt
                Print "--- Geminiからの応答 ---"
                Print gemini_response
            "#;

            println!("スクリプトを解析中...");
            let statements = match parse(script) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("解析エラー: {}", e);
                    return;
                }
            };

            println!("\nスクリプトを実行中...\n");
            let mut executor = Executor::new(config); // ExecutorにConfigを渡す
            if let Err(e) = executor.run(&statements).await {
                eprintln!("実行時エラー: {}", e);
            }

            println!("\nスクリプトの実行が完了しました。");
        }
    }
}
