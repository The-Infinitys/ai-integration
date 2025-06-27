// src/help.rs

/// AuraScriptのヘルプ情報をコンソールに表示します。
/// 言語の概要、利用可能なコマンド、AIプロバイダー、およびインタラクティブモードの使用方法について説明します。
pub fn display_help() {
    println!("\n--- AuraScript ヘルプ ---");
    println!("AuraScriptは、AIアシスタントの自動化タスクを記述するためのシンプルな言語です。");
    println!("\n利用可能なコマンド:");
    println!("  let <変数名> = <値/式>");
    println!("    値を変数に代入します。");
    println!("    例: let my_var = \"Hello\";");
    println!("    例: let file_content = Read file \"path/to/file.txt\";");
    println!("    例: let ai_response = Generate content from \"openai\" with prompt \"Rustとは？\";");
    println!("\n  Print <値/式>");
    println!("    コンソールに値を出力します。");
    println!("    例: Print my_var;");
    println!("    例: Print \"直接出力する文字列\";");
    println!("\n利用可能なAIプロバイダー (Generate content):");
    println!("  \"openai\"   (OpenAI APIを使用)");
    println!("  \"ollama\"   (OllamaローカルAIを使用)");
    println!("  \"gemini\"   (Google Gemini APIを使用)");
    println!("\nインタラクティブチャットモード:");
    println!("  `cargo run -- prompt <provider>` コマンドで、指定したAIプロバイダーと直接チャットできます。");
    println!("  例: `cargo run -- prompt openai`");
    println!("  例: `cargo run -- prompt ollama`");
    println!("  例: `cargo run -- prompt gemini`");
    println!("  チャット中に 'exit' または 'quit' と入力するとチャットが終了します。");
    println!("\nスクリプト実行:");
    println!("  引数なしで `cargo run` を実行すると、デフォルトのAuraScriptが実行されます。");
    println!("\nヘルプ表示:");
    println!("  `cargo run -- help` コマンドでこのヘルプを表示します。");
    println!("\n--- ヘルプの終了 ---");
}
