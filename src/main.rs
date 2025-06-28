// src/main.rs

use ai_integration::modules::agent::{AIAgent, api::{AIApi, ApiClient, openai::OpenAIApi}, Character};
use ai_integration::modules::aurascript::AuraScriptRunner;
use ai_integration::modules::prompt;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    prompt::print_info("Hello from AI Agent project!");
    prompt::print_info("Type your message and press Enter. Type '/exit' to quit.");
    prompt::print_info("Type '/enable_aurascript_exec' to let AI execute commands, '/disable_aurascript_exec' to disable.");
    prompt::print_info("\n--- AI Agent Initialization ---");

    let mut agent: AIAgent;

    let aura_script_runner_instance = AuraScriptRunner::new();

    let ollama_api_candidate = OpenAIApi::new_from_ollama_list().await;

    // Define the system prompt string. This will be used for both new() and default().
    let system_prompt_for_ai = r#"あなたはAIアシスタントです。ユーザーの質問に簡潔に答えます。
あなたの目標は、ユーザーの要求を理解し、必要に応じてツールを利用して、最終的な回答を提供することです。
あなたは「思考 (Thought)」、「行動 (Action)」、「観察 (Observation)」のループで動作します。

**思考のステップ (Thought):**
ユーザーの要求を分析し、最も適切な次のステップを決定します。
利用可能なツール（AuraScriptコマンド）が、あなたの知識だけでは答えられない、または情報を確認する必要がある場合に役立つかどうか判断します。
ツールが必要ない場合、直接ユーザーに最終応答を生成します。

**行動のステップ (Action):**
AuraScriptコマンドを実行する場合は、コマンドをコードブロック（`<aurascript>` で始まり、`</aurascript>` で終わる）で囲んでください。
コードブロックの中には、`!` で始まるシェルコマンド（例: `!ls -l`）または `/` で始まるカスタムツール（例: `/web_search Rust programming`）を1行に1つずつ記述できます。
現在利用可能なカスタムツール: `echo [テキスト]`, `web_search [クエリ]`

**ユーザーに直接応答する場合:**
あなたの応答はユーザーに向けられます。**応答の前に `USER: ` と明確に書いてください。**

**観察のステップ (Observation):**
あなたがコマンドブロックを出力した後、システムはその中のコマンドを順番に実行し、それぞれの出力がチャット履歴の「Command Output:」というプレフィックスを持つシステムメッセージとしてあなたに提供されます。あなたはこれを受け取り、次の思考と行動を決定します。

**例（思考-行動-観察ループのログ）:**
ユーザー: Rustの現在の安定版のバージョンは何ですか？

AIの思考: Rustの現在の安定版バージョンを知るにはウェブ検索が必要だ。
AIの行動:
```aurascript
/web_search Rust current stable version
```

(システムがコマンドを実行し、チャット履歴に結果を追加)
Command Output: Web search results for 'Rust current stable version': Rust 1.79.0 (stable) released on 2024-06-13.

AIの思考: ウェブ検索結果からRustの現在の安定版バージョンが分かった。これをユーザーに伝えることができる。
AIの行動: USER: Rustの現在の安定版バージョンは 1.79.0 です。

不明な点がある場合、またはコマンドの実行結果が不十分な場合は、さらにツールを実行するか、明確な質問をして情報を集めてください。
"#;


    if ollama_api_candidate.model == "llama2" && ollama_api_candidate.base_url == "http://localhost:11434/v1/chat/completions" {
        prompt::print_info("[WARN] Ollama model detection failed or no specific model found. Falling back to default AIAgent configuration.");
        agent = AIAgent::default();
        agent.update_main_system_prompt(system_prompt_for_ai.to_string());
        agent.aurascript_runner = aura_script_runner_instance;
        prompt::print_info(&format!("  Using API Base URL: {}", agent.api.config.get("base_url").unwrap_or(&"unknown".to_string())));
        prompt::print_info(&format!("  Using Model: {}", agent.api.config.get("model").unwrap_or(&"unknown".to_string())));
    } else {
        prompt::print_info("[INFO] Initializing with detected Ollama configuration.");
        let mut api = AIApi::new(ApiClient::OpenAI(ollama_api_candidate));
        api.add_config("model".to_string(), match &api.client { ApiClient::OpenAI(o) => o.model.clone() });
        api.add_config("base_url".to_string(), match &api.client { ApiClient::OpenAI(o) => o.base_url.clone() });
        agent = AIAgent::new(
            api,
            system_prompt_for_ai.to_string(),
            aura_script_runner_instance,
            false, // Default: AI cannot execute commands autonomously
        );
        prompt::print_info(&format!("  Using API Base URL: {}", agent.api.config.get("base_url").unwrap_or(&"unknown".to_string())));
        prompt::print_info(&format!("  Using Model: {}", agent.api.config.get("model").unwrap_or(&"unknown".to_string())));
    }

    prompt::print_info("-------------------------------");
    prompt::print_info("Try commands like 'note test', 'get test note', 'history', '!ls -l', or '/echo Hello AuraScript!' for demonstration.");


    if let Some(main_prompt) = agent.get_main_system_prompt() {
        prompt::print_info(&format!("\nAgent's Initial System Prompt:\n{}", main_prompt));
    }


    loop {
        let user_input = match prompt::read_user_input() {
            Ok(input) => input,
            Err(e) => {
                prompt::print_error(&format!("Error reading input: {}", e));
                continue;
            }
        };

        if user_input.eq_ignore_ascii_case("/enable_aurascript_exec") {
            agent.set_can_execute_aurascript(true);
            prompt::print_config("AI is now allowed to execute AuraScript commands.");
            continue;
        } else if user_input.eq_ignore_ascii_case("/disable_aurascript_exec") {
            agent.set_can_execute_aurascript(false);
            prompt::print_config("AI is now NOT allowed to execute AuraScript commands.");
            continue;
        } else if user_input.eq_ignore_ascii_case("/exit") {
            prompt::print_info("Exiting AI Agent. Goodbye!");
            break;
        }

        // Check if the input is a direct AuraScript command from the user
        if user_input.starts_with('!') || user_input.starts_with('/') {
            prompt::print_info("\n[AuraScript] Detected direct user AuraScript command.");
            agent.add_message(Character::User, Character::Cmd, &user_input);

            match agent.aurascript_runner.run_script(&user_input).await {
                Ok(output) => {
                    prompt::print_aurascript_output(&output);
                    agent.add_message(Character::Cmd, Character::Agent, &output);
                }
                Err(e) => {
                    prompt::print_error(&format!("[AuraScript Error]: {}", e));
                    agent.add_message(Character::Cmd, Character::Agent, format!("Error: {}", e));
                }
            }
        } else {
            // Not a direct AuraScript command.
            // Delegate the main AI interaction loop to the agent's process_user_input_and_react method.
            match agent.process_user_input_and_react(&user_input).await {
                Ok(_final_ai_response) => {
                    // Final AI response has already been printed and logged by process_user_input_and_react
                }
                Err(e) => {
                    prompt::print_error(&format!("An error occurred during AI's interaction loop: {}", e));
                }
            }
        }

        // Example usage of notes and chat history (for demonstration purposes).
        if user_input.contains("note test") {
            agent.add_note(vec!["test".to_string(), "example".to_string()], "Test Note Title", "This is the content of a test note.");
            prompt::print_info("Note added: 'Test Note Title' with tags 'test', 'example'");
        } else if user_input.contains("get test note") {
            let notes = agent.get_notes_by_tags(&["test".to_string()]);
            if !notes.is_empty() {
                prompt::print_info("Found notes with tag 'test':");
                for note in notes {
                    prompt::print_info(&format!("  Title: '{}', Data: '{}'", note.title, note.data));
                }
            } else {
                prompt::print_info("No notes found with tag 'test'.");
            }
        } else if user_input.eq_ignore_ascii_case("history") {
            prompt::print_info("\n--- Chat History ---");
            if agent.get_chat_history().is_empty() {
                prompt::print_info("No chat messages yet.");
            } else {
                for msg in agent.get_chat_history() {
                    prompt::print_info(&format!("[{}] {:?} -> {:?}: {}", msg.date.format("%Y-%m-%d %H:%M:%S"), msg.from, msg.to, msg.text));
                }
            }
            prompt::print_info("--------------------");
        }
    }
    Ok(())
}
