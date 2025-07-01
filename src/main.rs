use crate::modules::agent::api::AIProvider;
use crate::modules::chat::tui::TuiApp; // TUIアプリケーションのTuiApp構造体をインポート
use anyhow::{Result, anyhow}; // anyhowクレートからのResult型とanyhow!マクロを使用
use crossterm::execute;
use crossterm::terminal::{LeaveAlternateScreen, disable_raw_mode}; // ターミナルを復元するためにインポート
use std::io; // 標準入出力操作のためのトレイトと関数をインポート
use std::panic; // パニックフックを設定するためにインポート
// HTTPクライアントのためにインポート
use serde_json::Value; // JSONパースのためにインポート
use std::process::Command; // コマンド実行のためにインポート

mod modules; // modulesディレクトリをモジュールとして宣言

/// システムの利用可能なメモリをバイト単位で取得します。
fn get_available_memory_bytes() -> Result<u64> {
    let output = Command::new("free").arg("-b").output()?;
    let stdout = String::from_utf8(output.stdout)?;

    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() < 2 {
        return Err(anyhow!("free -b の出力が予期せぬ形式です。"));
    }

    let mem_line = lines[1]; // 2行目がメモリ情報
    let parts: Vec<&str> = mem_line.split_whitespace().collect();
    // free -b の出力は通常以下の形式:
    //               total        used        free      shared  buff/cache   available
    // Mem:    16777216000  10000000000   2000000000   500000000   4000000000   6000000000
    // available は7番目の要素 (インデックス6)
    if parts.len() < 7 {
        return Err(anyhow!(
            "free -b のメモリ行が予期せぬ形式です。'available' カラムが見つかりません。"
        ));
    }

    let available_mem_str = parts[6];
    let available_mem_bytes = available_mem_str.parse::<u64>()?;

    Ok(available_mem_bytes)
}

/// システムのCPUコア数を取得します。
fn get_cpu_cores() -> Result<u32> {
    let output = Command::new("nproc").output()?;
    let stdout = String::from_utf8(output.stdout)?;

    let cores = stdout.trim().parse::<u32>()?;

    Ok(cores)
}

/// 利用可能なOllamaモデルを取得し、バランスの取れたモデルを選択します。
async fn select_balanced_ollama_model(base_url: &str, total_memory_bytes: u64) -> Result<String> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/tags", base_url); // Ollamaのモデルリストエンドポイント

    let response = client.get(&url).send().await?.json::<Value>().await?;

    let models_data = response["models"]
        .as_array()
        .ok_or_else(|| anyhow!("モデルリストが予期しない形式です。"))?;

    let mut models_with_size: Vec<(String, u64)> = models_data
        .iter()
        .filter_map(|model| {
            let name = model["name"].as_str()?.to_string();
            let size_str = model["size"].as_u64()?;
            Some((name, size_str))
        })
        .collect();

    // モデルをサイズでソート（小さい順）
    models_with_size.sort_by_key(|k| k.1);

    // 利用可能なメモリの40%を閾値とする
    let memory_threshold = total_memory_bytes * 40 / 100;

    // 優先順位に基づいてモデルを選択
    // 1. llama3.1:8b (メモリ閾値内)
    // 2. llama3.1:latest (メモリ閾値内)
    // 3. gemma3:latest (メモリ閾値内)
    // 4. その他のgemma3モデル (メモリ閾値内)
    // 5. メモリ閾値内で最も小さいモデル
    // 6. デフォルトで最初の利用可能なモデルにフォールバック

    // 明示的な8bモデルの優先
    if let Some((name, _size)) = models_with_size
        .iter()
        .find(|(name, size)| name == "llama3.1:8b" && *size <= memory_threshold)
    {
        return Ok(name.clone());
    }

    // 最新のllama3.1モデルを優先
    if let Some((name, _size)) = models_with_size
        .iter()
        .find(|(name, size)| name == "llama3.1:latest" && *size <= memory_threshold)
    {
        return Ok(name.clone());
    }

    // gemma3:latest を優先
    if let Some((name, _size)) = models_with_size
        .iter()
        .find(|(name, size)| name == "gemma3:latest" && *size <= memory_threshold)
    {
        return Ok(name.clone());
    }

    // その他の gemma3 モデルを検索 (メモリ閾値内)
    if let Some((name, _)) = models_with_size
        .iter()
        .find(|(name, size)| name.starts_with("gemma3:") && *size <= memory_threshold)
    {
        return Ok(name.clone());
    }

    // メモリ閾値内で最も小さいモデルを選択
    if let Some((name, _)) = models_with_size
        .iter()
        .filter(|(_, size)| *size <= memory_threshold)
        .min_by_key(|k| k.1)
    {
        return Ok(name.clone());
    }

    // デフォルトで最初の利用可能なモデルにフォールバック (メモリ閾値に関わらず)
    if let Some((first_model, _)) = models_with_size.into_iter().next() {
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

    let args: Vec<String> = std::env::args().collect();
    let use_cli = !args.contains(&"--tui".to_string());

    let provider_arg = args.iter().find(|arg| arg.starts_with("--provider="));
    let provider = if let Some(arg) = provider_arg {
        match arg.split('=').nth(1).unwrap_or("ollama") {
            "gemini" => AIProvider::Gemini,
            _ => AIProvider::Ollama,
        }
    } else {
        AIProvider::Ollama
    };

    let (base_url, default_model) = match provider {
        AIProvider::Ollama => {
            let ollama_base_url = std::env::var("OLLAMA_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:11434".to_string());

            let available_memory_bytes = get_available_memory_bytes().unwrap_or_else(|e| {
                eprintln!("利用可能メモリの取得中にエラーが発生しました: {}. モデル選択に影響する可能性があります。", e);
                // エラー時は適当な大きな値を返すか、エラーハンドリングを強化
                16 * 1024 * 1024 * 1024 // 例: 16GB
            });
            let cpu_cores = get_cpu_cores().unwrap_or_else(|e| {
                eprintln!("CPUコア数の取得中にエラーが発生しました: {}. モデル選択に影響する可能性があります。", e);
                4 // 例: 4コア
            });
            println!("検出された利用可能メモリ: {} bytes", available_memory_bytes);
            println!("検出されたCPUコア数: {}", cpu_cores);

            let default_ollama_model = match select_balanced_ollama_model(
                &ollama_base_url,
                available_memory_bytes,
            )
            .await
            {
                Ok(model) => {
                    println!("選択されたデフォルトモデル: {}", model);
                    model
                }
                Err(e) => {
                    eprintln!(
                        "モデルの選択中にエラーが発生しました: {}. デフォルトで 'gemma3:latest' を使用します。",
                        e
                    );
                    "gemma3:latest".to_string() // エラー発生時のフォールバック
                }
            };
            (ollama_base_url, default_ollama_model)
        }
        AIProvider::Gemini => {
            let gemini_base_url = std::env::var("GEMINI_BASE_URL")
                .unwrap_or_else(|_| "https://generativelanguage.googleapis.com".to_string());
            let default_gemini_model =
                std::env::var("GEMINI_DEFAULT_MODEL").unwrap_or_else(|_| "gemini-pro".to_string());
            println!(
                "選択されたデフォルトモデル (Gemini): {}",
                default_gemini_model
            );
            (gemini_base_url, default_gemini_model)
        }
    };

    if use_cli {
        // --cliオプションが指定された場合はCLIアプリケーションを起動
        println!("CLIアプリケーションを起動中...");
        modules::chat::cli::run_cli(provider, base_url, default_model).await?;
    } else {
        // デフォルトでTUIアプリケーションを起動
        println!("TUIアプリケーションを起動中...");
        let mut app = TuiApp::new(provider, base_url, default_model);
        app.run().await?;
    }

    Ok(())
}
