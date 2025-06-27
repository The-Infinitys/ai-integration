// src/modules/chat/api.rs

use async_trait::async_trait;
use std::error::Error;

/// `AIAgentApi`トレイトは、AIエージェントとやり取りするための基本的なインターフェースを定義します。
/// 将来的に異なるAIモデルやサービス（例：OpenAI, Gemini, その他のカスタムAI）を使用する際に、
/// このトレイトを実装することで、コアロジックを変更せずにAIバックエンドを切り替えられます。
#[async_trait]
pub trait AIAgentApi {
    /// ユーザーの入力に基づいてAIからの応答を非同期に取得します。
    ///
    /// # 引数
    /// * `user_input` - ユーザーからのテキスト入力。
    ///
    /// # 戻り値
    /// `Result<String, Box<dyn Error>>` - AIからの応答文字列、またはエラー。
    async fn get_ai_response(&self, user_input: &str) -> Result<String, Box<dyn Error>>;
}

/// `DummyAIAgentApi`は`AIAgentApi`トレイトの仮実装です。
/// 実際のAI API呼び出しの代わりに、固定された、または単純な応答を返します。
pub struct DummyAIAgentApi;

#[async_trait]
impl AIAgentApi for DummyAIAgentApi {
    async fn get_ai_response(&self, user_input: &str) -> Result<String, Box<dyn Error>> {
        // ここに実際のAI API呼び出しのロジックが将来的に入ります。
        // 例えば、外部APIへのHTTPリクエストなどが考えられます。
        // 現在は、入力に基づいてダミーの応答を返します。
        let response = format!("AI (仮): あなたの入力「{}」について考えています...", user_input);
        Ok(response)
    }
}
