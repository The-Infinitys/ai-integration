// src/modules/prompt.rs

/// Represents a prompt for the AI agent.
/// AIエージェントへのプロンプトを表します。
pub struct Prompt {
    pub system_message: String,
    pub user_message: String,
}

impl Prompt {
    /// Creates a new `Prompt` instance.
    /// 新しい `Prompt` インスタンスを作成します。
    pub fn new(system: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            system_message: system.into(),
            user_message: user.into(),
        }
    }

    /// Generates the full prompt string by combining system and user messages.
    /// システムメッセージとユーザーメッセージを組み合わせて完全なプロンプト文字列を生成します。
    pub fn generate_full_prompt(&self) -> String {
        format!("System: {}\nUser: {}", self.system_message, self.user_message)
    }

    /// Updates the system message.
    /// システムメッセージを更新します。
    pub fn set_system_message(&mut self, message: impl Into<String>) {
        self.system_message = message.into();
    }

    /// Updates the user message.
    /// ユーザーメッセージを更新します。
    pub fn set_user_message(&mut self, message: impl Into<String>) {
        self.user_message = message.into();
    }
}
