// src/modules/aurascript.rs

/// AIが使用するコマンドラインを設定する
///
/// ## 例
///
/// ```bash
/// !ls                     # ターミナルのコマンドを実行する
/// /web_search Rust Google # 予め使用できるコマンドを設定しておき、AIが利用できるようにする
/// ```
///
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::Command;
// Removed unused async_trait import
use std::pin::Pin; // Import Pin for boxed futures

/// Type alias for a boxed, Send + Sync + 'static async closure for internal commands.
/// 内部コマンド用のBox化された、Send + Sync + 'static な非同期クロージャの型エイリアス。
/// Now returns `Pin<Box<dyn Future + Send>>` directly, which is awaitable.
/// これで直接 `Pin<Box<dyn Future + Send>>` を返し、これは await 可能である。
pub type InternalCommandFn = Arc<dyn Fn(String) -> Pin<Box<dyn futures::Future<Output = Result<String, String>> + Send>> + Send + Sync + 'static>;


/// Manages and executes AuraScript commands.
/// AuraScriptコマンドを管理し、実行します。
pub struct AuraScriptRunner {
    /// Registered internal commands, mapped by command name (e.g., "web_search", "echo").
    internal_commands: HashMap<String, InternalCommandFn>,
}

impl AuraScriptRunner {
    /// Creates a new `AuraScriptRunner` instance with default internal commands.
    /// デフォルトの内部コマンドを持つ新しい `AuraScriptRunner` インスタンスを作成します。
    pub fn new() -> Self {
        let mut runner = Self {
            internal_commands: HashMap::new(),
        };

        // Register default internal commands
        runner.register_command(
            "echo",
            Arc::new(|args| Box::pin(async move { Ok(args) })), // Box::pin is used here
        );

        runner.register_command(
            "web_search",
            Arc::new(|args| Box::pin(async move {
                // This is a dummy implementation. In a real app, this would call a web search API.
                println!("[AuraScript] Simulating web search for: '{}'", args);
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await; // Simulate delay
                Ok(format!("Web search results for '{}': Found 3 articles. First one is about Rust programming.", args))
            })),
        );

        runner
    }

    /// Registers a new internal command.
    /// 新しい内部コマンドを登録します。
    pub fn register_command(&mut self, name: &str, func: InternalCommandFn) {
        self.internal_commands.insert(name.to_string(), func);
    }

    /// Runs an AuraScript command.
    /// AuraScriptコマンドを実行します。
    ///
    /// # Arguments
    /// * `script` - The command string (e.g., "!ls -l" or "/web_search Rust").
    ///
    /// # Returns
    /// `Ok(String)` containing the command's output, or `Err(String)` if an error occurs.
    pub async fn run_script(&self, script: &str) -> Result<String, String> {
        if script.starts_with('!') {
            // Shell command
            let cmd_parts: Vec<&str> = script[1..].splitn(2, ' ').collect();
            let command = cmd_parts[0];
            let args = cmd_parts.get(1).unwrap_or(&"");

            println!("[AuraScript] Executing shell command: '{} {}'", command, args);

            let output = Command::new(command)
                .args(shlex::split(args).unwrap_or_default()) // Safely split arguments
                .output()
                .await
                .map_err(|e| format!("Failed to execute shell command '{}': {}", command, e))?;

            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            } else {
                Err(format!(
                    "Shell command '{}' failed with exit code {}. Stderr: {}",
                    command,
                    output.status.code().unwrap_or(-1),
                    String::from_utf8_lossy(&output.stderr)
                ))
            }
        } else if script.starts_with('/') {
            // Internal command
            let cmd_parts: Vec<&str> = script[1..].splitn(2, ' ').collect();
            let command_name = cmd_parts[0];
            let args = cmd_parts.get(1).unwrap_or(&"").to_string(); // Arguments for the internal command

            println!("[AuraScript] Executing internal command: '/{} {}'", command_name, args);

            if let Some(func) = self.internal_commands.get(command_name) {
                // func(args) now returns Pin<Box<dyn Future>> directly, so it can be awaited.
                // func(args) は直接 Pin<Box<dyn Future>> を返すので、await できる。
                func(args).await
            } else {
                Err(format!("Unknown internal command: '{}'", command_name))
            }
        } else {
            Err("Not an AuraScript command. Must start with '!' or '/'.".to_string())
        }
    }
}

// Default implementation (empty runner, no commands registered by default)
impl Default for AuraScriptRunner {
    fn default() -> Self {
        Self::new()
    }
}
