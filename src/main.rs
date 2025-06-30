use crate::modules::cli::App as CliApp; // CLIアプリケーションのApp構造体をインポート
use crate::modules::tui::TuiApp; // TUIアプリケーションのTuiApp構造体をインポート
use anyhow::Result; // anyhowクレートからのResult型を使用
use crossterm::execute;
use crossterm::terminal::{LeaveAlternateScreen, disable_raw_mode}; // ターミナルを復元するためにインポート
use std::io; // 標準入出力操作のためのトレイトと関数をインポート
use std::panic; // パニックフックを設定するためにインポート // ターミナルコマンドを実行するためにインポート

mod modules; // modulesディレクトリをモジュールとして宣言

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
    let default_ollama_model =
        std::env::var("DEFAULT_OLLAMA_MODEL").unwrap_or_else(|_| "gemma3:latest".to_string()); // 必要に応じてモデル名を調整

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
