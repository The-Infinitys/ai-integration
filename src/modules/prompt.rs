// src/modules/prompt.rs

use std::io::{self, Write};
use futures::stream::BoxStream;
use futures::StreamExt; // For StreamExt::next().await

/// Reads a line of input from the user.
/// ユーザーから1行の入力を読み取ります。
pub fn read_user_input() -> Result<String, String> {
    print!("\nUser Input: ");
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let mut user_input = String::new();
    io::stdin().read_line(&mut user_input).map_err(|e| e.to_string())?;
    Ok(user_input.trim().to_string())
}

/// Prints a streaming response from the AI to the console.
/// AIからのストリーミング応答をコンソールに出力します。
pub async fn print_ai_streaming_response(mut stream: BoxStream<'static, Result<String, String>>) -> Result<String, String> {
    let mut accumulated_response = String::new();
    print!("[AI Response] (streaming): "); // Indicates AI's response start
    std::io::stdout().flush().map_err(|e| e.to_string())?;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                print!("{}", chunk);
                std::io::stdout().flush().map_err(|e| e.to_string())?;
                accumulated_response.push_str(&chunk);
            }
            Err(e) => {
                eprintln!("\nError during AI streaming: {}", e);
                accumulated_response.push_str(&format!("\nError: {}", e));
                return Err(format!("AI streaming error: {}", e));
            }
        }
    }
    println!(); // Newline after streaming
    Ok(accumulated_response.trim().to_string())
}

/// Prints a general informational message.
/// 一般的な情報メッセージを出力します。
pub fn print_info(message: &str) {
    println!("{}", message);
}

/// Prints an AuraScript command output.
/// AuraScriptコマンドの出力を出力します。
pub fn print_aurascript_output(output: &str) {
    println!("[AuraScript Output]:\n{}", output);
}

/// Prints a tool output.
/// ツールの出力を出力します。
pub fn print_tool_output(output: &str) {
    println!("[Tool Output]:\n{}", output);
}

/// Prints an error message.
/// エラーメッセージを出力します。
pub fn print_error(message: &str) {
    eprintln!("[ERROR] {}", message);
}

/// Prints a configuration message.
/// 設定メッセージを出力します。
pub fn print_config(message: &str) {
    println!("[CONFIG] {}", message);
}
