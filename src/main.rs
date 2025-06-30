use crate::modules::cli::App as CliApp; // CLIアプリケーションのApp構造体をインポート
use crate::modules::tui::TuiApp; // TUIアプリケーションのTuiApp構造体をインポート
use anyhow::{Result, anyhow}; // anyhowクレートからのResult型とanyhow!マクロを使用
use crossterm::execute;
use crossterm::terminal::{LeaveAlternateScreen, disable_raw_mode}; // ターミナルを復元するためにインポート
use std::io; // 標準入出力操作のためのトレイトと関数をインポート
use std::panic; // パニックフックを設定するためにインポート
use reqwest; // HTTPクライアントのためにインポート
use serde_json::Value; // JSONパースのためにインポート

mod modules; // modulesディレクトリをモジュールとして宣言

/// 利用可能なOllamaモデルを取得し、バランスの取れたモデルを選択します。
async fn select_balanced_ollama_model(base_url: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/tags", base_url); // Ollamaのモデルリストエンドポイント

    let response = client.get(&url).send().await?.json::<Value>().await?;

    let models_data = response["models"]
        .as_array()
        .ok_or_else(|| anyhow!("モデルリストが予期しない形式です。"))?;

    let available_models: Vec<String> = models_data
        .iter()
        .filter_map(|model| model["name"].as_str().map(|s| s.to_string()))
        .collect();

    // 優先順位に基づいてモデルを選択
    // 1. llama3.1:8b
    // 2. llama3.1:latest
    // 3. gemma3:latest (もしgemma3:1bが唯一のgemma3モデルなら、それにフォールバック)
    // 4. その他のgemma3モデル
    // 5. 見つかった最初のモデル

    // 明示的な8bモデルの優先
    if available_models.contains(&"llama3.1:8b".to_string()) {
        return Ok("llama3.1:8b".to_string());
    }
    
    // 最新のllama3.1モデルを優先
    if available_models.contains(&"llama3.1:latest".to_string()) {
        return Ok("llama3.1:latest".to_string());
    }

    // gemma3:latest を優先 (もしgemma3:1bが存在し、これが最新ならそれも考慮)
    if available_models.contains(&"gemma3:latest".to_string()) {
        return Ok("gemma3:latest".to_string());
    }

    // その他の gemma3 モデルを検索
    if let Some(gemma_model) = available_models.iter().find(|id| id.starts_with("gemma3:")) {
        return Ok(gemma_model.clone());
    }

    // デフォルトで最初の利用可能なモデルにフォールバック
    if let Some(first_model) = available_models.into_iter().next() {
        return Ok(first_model);
    }

    Err(anyhow!("利用可能なOllamaモデルが見つかりませんでした。"))
}


#[tokio::main] // 非同期メイン関数をtokioランタイムで実行するためのマクロ
async fn main() -> Result<()> {
    // パニックフックを設定
    // アプリケーションがパニックしたときに、ターミナルを正常な状態に復元するようにします。
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // 生モードを無効にし、代替スクリーンを終了してターミナルを復元
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);

        // 元のパニックハンドラを呼び出し、パニック情報を表示
        original_hook(panic_info);
    }));

    let ollama_base_url =
        std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434".to_string());

    // 利用可能なOllamaモデルからバランスの取れたモデルを選択
    let default_ollama_model = match select_balanced_ollama_model(&ollama_base_url).await {
        Ok(model) => {
            println!("選択されたデフォルトモデル: {}", model);
            model
        },
        Err(e) => {
            eprintln!("モデルの選択中にエラーが発生しました: {}. デフォルトで 'gemma3:latest' を使用します。", e);
            "gemma3:latest".to_string() // エラー発生時のフォールバック
        }
    };

    // コマンドライン引数をチェック
    let args: Vec<String> = std::env::args().collect();
    let use_cli = args.contains(&"--cli".to_string());

    if use_cli {
        // --cliオプションが指定された場合はCLIアプリケーションを起動
        println!("CLIアプリケーションを起動中...");
        let mut app = CliApp::new(ollama_base_url, default_ollama_model);
        app.run().await?;
    } else {
        // デフォルトでTUIアプリケーションを起動
        println!("TUIアプリケーションを起動中...");
        let mut app = TuiApp::new(ollama_base_url, default_ollama_model);
        app.run().await?;
    }

    Ok(())
}
