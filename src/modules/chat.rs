pub mod cli;
pub mod tui;

use crate::modules::agent::api::{AIProvider, ChatMessage, ChatRole};
use crate::modules::agent::{AIAgent, AgentEvent};
use anyhow::Result;
use futures_util::{TryStreamExt, stream::BoxStream};
use std::sync::Arc;
use tokio::sync::Mutex;

/// AIエージェントとの単一のチャットセッションを表します。
/// この構造体はUIから独立しており、チャットの状態管理とAIとの対話ロジックに責任を持ちます。
#[derive(Clone)]
pub struct ChatSession {
    agent: Arc<Mutex<AIAgent>>,
    pub current_model: String,
}

impl ChatSession {
    /// 新しいチャットセッションを作成します。
    pub fn new(provider: AIProvider, base_url: String, default_model: String) -> Self {
        let agent = Arc::new(Mutex::new(AIAgent::new(
            provider,
            base_url,
            default_model.clone(),
        )));
        ChatSession {
            agent,
            current_model: default_model,
        }
    }

    /// ユーザーメッセージをセッション履歴に追加します。
    /// このメソッドはUIに依存せず、内部状態のみを更新します。
    pub async fn add_user_message(&mut self, content: String) {
        let mut agent_locked = self.agent.lock().await;
        let user_message = ChatMessage {
            role: ChatRole::User,
            content,
        };
        agent_locked.add_message_to_history(user_message.clone());
    }

    /// ツール実行を伴うリアルタイムチャットセッションを開始および管理します。
    /// このメソッドは、UIイベントのストリームを返します。UI層はこのストリームを消費して表示を更新します。
    pub async fn start_realtime_chat(&mut self) -> Result<BoxStream<'static, Result<AgentEvent>>> {
        let agent_arc_clone = self.agent.clone();
        let agent_locked = self.agent.lock().await;
        let current_turn_messages = agent_locked.messages.clone();
        drop(agent_locked);

        let stream =
            AIAgent::chat_with_tools_realtime(agent_arc_clone, current_turn_messages).await?;
        let stream = stream.map_err(anyhow::Error::from);
        Ok(Box::pin(stream))
    }

    /// 最後のユーザーメッセージとその後のAIの応答を履歴から削除します。
    pub async fn revert_last_turn(&mut self) {
        let mut agent_locked = self.agent.lock().await;
        // エージェントの履歴を元に戻す
        agent_locked.revert_last_user_message();
    }

    /// AIエージェントが使用するモデルを設定します。
    pub async fn set_model(&mut self, model_name: String) -> Result<()> {
        let mut agent_locked = self.agent.lock().await;
        agent_locked.set_model(model_name.clone());
        self.current_model = model_name;
        Ok(())
    }

    /// 利用可能なモデルのリストを取得します。
    pub async fn list_models(&self) -> Result<serde_json::Value> {
        let agent_locked = self.agent.lock().await;
        agent_locked
            .list_available_models()
            .await
            .map_err(anyhow::Error::from)
    }

    /// 現在のセッションメッセージのクローンを取得します。
    pub async fn get_messages(&self) -> Vec<ChatMessage> {
        let agent_locked = self.agent.lock().await;
        agent_locked.messages.clone()
    }

        

    pub async fn clear_history(&mut self) {
        let mut agent_locked = self.agent.lock().await;
        agent_locked.clear_history();
    }

    pub async fn get_log_path(&self) -> Option<String> {
        let agent_locked = self.agent.lock().await;
        agent_locked.get_log_path()
    }
}
