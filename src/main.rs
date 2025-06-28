use ai_integration::modules::prompt::Prompt; // Import the Prompt module

fn main() {
    println!("Hello from AI Agent project!");

    // Create a new prompt instance
    // 新しいプロンプトインスタンスを作成
    let mut my_prompt = Prompt::new(
        "You are a helpful AI assistant.", // System message
        "What is the capital of France?",  // User message
    );

    // Generate and print the full prompt
    // フルプロンプトを生成して表示
    println!("\nGenerated Prompt:");
    println!("{}", my_prompt.generate_full_prompt());

    // Update the user message and generate again
    // ユーザーメッセージを更新して再度生成
    my_prompt.set_user_message("Tell me a short story about a brave knight.");
    println!("\nUpdated Prompt:");
    println!("{}", my_prompt.generate_full_prompt());

    // You can continue to integrate this Prompt module into the AIAgent
    // and use it when interacting with an actual AI API.
    // このPromptモジュールをAIAgentに統合し、実際のAI APIとやり取りする際に
    // 使用することができます。
}
