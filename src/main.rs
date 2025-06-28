// src/main.rs

use ai_integration::modules::prompt::Prompt; // Import the Prompt module
use std::io::{self, Write}; // Import the standard I/O library for input

fn main() -> Result<(), std::io::Error> {
    println!("Hello from AI Agent project!");
    println!("Type your message and press Enter. Type 'exit' to quit.");

    // Create a new prompt instance with a default system message
    // デフォルトのシステムメッセージを持つ新しいプロンプトインスタンスを作成
    let mut my_prompt = Prompt::new(
        "You are a helpful AI assistant. Please respond concisely.", // System message
        "", // User message will be updated by user input
    );

    loop {
        print!("\nUser Input:");
        std::io::stdout().flush()?;
        let mut user_input = String::new();

        // Read a line from standard input
        // 標準入力から1行読み込み
        io::stdin()
            .read_line(&mut user_input)
            .expect("Failed to read line"); // エラーハンドリング

        // Trim whitespace and check for exit command
        // 前後の空白を削除し、終了コマンドをチェック
        let trimmed_input = user_input.trim();
        if trimmed_input.eq_ignore_ascii_case("exit") {
            println!("Exiting AI Agent. Goodbye!");
            break; // Exit the loop
        }

        // Update the user message of the prompt
        // プロンプトのユーザーメッセージを更新
        my_prompt.set_user_message(trimmed_input);

        // For now, we just print the generated full prompt.
        // In a real application, this prompt would be sent to an AI API.
        // 現時点では、生成された完全なプロンプトを出力するだけです。
        // 実際のアプリケーションでは、このプロンプトはAI APIに送信されます。
        println!("\nAI Agent's Internal Prompt (simulated input to AI):");
        println!("{}", my_prompt.generate_full_prompt());

        // --- Future: Call to AI API would go here ---
        // For example:
        // let ai_response = my_agent.send_prompt_to_ai(&my_prompt);
        // println!("AI Response: {}", ai_response);
    }
    Ok(())
}
