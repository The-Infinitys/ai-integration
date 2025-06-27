// src/modules/chat.rs

// 各チャットインターフェースの実装ファイルをサブモジュールとして宣言
pub mod cli;

use async_trait::async_trait;
use std::error::Error;

/// `Chat`トレイトは、チャットインターフェースの基本的な機能を定義します。
/// これにより、将来的にCLI以外のUI（例：GUI、Web）を導入する際に、
/// このトレイトを実装するだけでチャットロジックを変更せずに済みます。
#[async_trait]
pub trait Chat {
    /// ユーザーからの入力を非同期に読み込みます。
    async fn get_user_input(&self) -> Result<String, Box<dyn Error>>;

    /// AIエージェントからの応答を非同期に表示します。
    async fn display_ai_response(&self, response: &str) -> Result<(), Box<dyn Error>>;
}
