// src/prompt.rs

use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use crate::ai::AIGenerator;
use std::error::Error;
use std::pin::Pin; // Pinをインポート

/// 指定されたAIジェネレータを使用して、AIとのインタラクティブなチャットループを開始します。
/// ユーザーはコマンドラインからプロンプトを入力し、AIの応答を受け取ります。
/// 'exit' または 'quit' と入力するとチャットが終了します。
pub async fn start_chat_loop(generator: Pin<Box<dyn AIGenerator>>) -> Result<(), Box<dyn Error>> {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    println!("AIチャットを開始しました。'exit' または 'quit' と入力すると終了します。");

    loop {
        print!("あなた: ");
        io::stdout().flush().await?; // プロンプトを確実に表示

        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;

        // EOF (Ctrl+Dなど) で入力が終了した場合
        if bytes_read == 0 {
            break;
        }

        let input = line.trim();
        if input.is_empty() {
            continue; // 空の入力はスキップ
        }

        // 終了コマンドのチェック
        if input == "exit" || input == "quit" {
            println!("チャットを終了します。");
            break;
        }

        println!("AI: (思考中...)");
        // AIにコンテンツを生成させる
        match generator.generate_content(input).await {
            Ok(response) => {
                println!("AI: {}", response);
            },
            Err(e) => {
                eprintln!("AI応答エラー: {}", e);
            }
        }
    }
    Ok(())
}
