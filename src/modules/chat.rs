pub mod tui;
pub mod cli;

use crate::modules::agent::api::{ChatMessage, ChatRole};
use crate::modules::agent::{AIAgent, AgentEvent};
use anyhow::Result;
use futures_util::{stream::BoxStream, TryStreamExt};
use std::sync::Arc;
use tokio::sync::Mutex;

/// AIエージェントとの単一のチャットセッションを表します。
/// この構造体はUIから独立しており、チャットの状態管理とAIとの対話ロジックに責任を持ちます。
#[derive(Clone)]
pub struct ChatSession {
    agent: Arc<Mutex<AIAgent>>,
    session_messages: Vec<ChatMessage>,
    pub current_model: String,
}

impl ChatSession {
    /// 新しいチャットセッションを作成します。
    pub fn new(base_url: String, default_model: String) -> Self {
        let agent = Arc::new(Mutex::new(AIAgent::new(base_url, default_model.clone())));
        ChatSession {
            agent,
            session_messages: vec![],
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
        self.session_messages.push(user_message);
    }

    /// ツール実行を伴うリアルタイムチャットセッションを開始および管理します。
    /// このメソッドは、UIイベントのストリームを返します。UI層はこのストリームを消費して表示を更新します。
    pub async fn start_realtime_chat(&mut self) -> Result<BoxStream<'static, Result<AgentEvent>>> {
        let agent_arc_clone = self.agent.clone();
        let agent_locked = self.agent.lock().await;
        let current_turn_messages = agent_locked.messages.clone();
        drop(agent_locked);

        let stream = AIAgent::chat_with_tools_realtime(agent_arc_clone, current_turn_messages).await?;
        let stream = stream.map_err(anyhow::Error::from);
        Ok(Box::pin(stream))
    }

    /// 最後のユーザーメッセージとその後のAIの応答を履歴から削除します。
    pub async fn revert_last_turn(&mut self) {
        let mut agent_locked = self.agent.lock().await;
        let initial_history_len = agent_locked.messages.len();

        // エージェントの履歴を元に戻す
        agent_locked.revert_last_user_message();

        // セッションメッセージの履歴を元に戻す
        if self.session_messages.last().is_some_and(|m| m.role == ChatRole::User) {
            self.session_messages.pop();
        }

        // エージェントの履歴とセッションの履歴を同期させる
        while let Some(msg) = agent_locked.messages.last() {
            if msg.role != ChatRole::User && agent_locked.messages.len() >= initial_history_len {
                agent_locked.messages.pop();
            } else {
                break;
            }
        }
        self.session_messages = agent_locked.messages.clone();
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
        agent_locked.list_available_models().await.map_err(anyhow::Error::from)
    }

    /// 現在のセッションメッセージのクローンを取得します。
    pub async fn get_messages(&self) -> Vec<ChatMessage> {
        self.session_messages.clone()
    }

    /// AIの応答が完了した後、最終的なメッセージを履歴に追加します。
    pub async fn add_assistant_message_to_history(&mut self, content: String) {
        let assistant_message = ChatMessage {
            role: ChatRole::Assistant,
            content,
        };
        // エージェントの履歴にも追加
        let mut agent_locked = self.agent.lock().await;
        agent_locked.add_message_to_history(assistant_message.clone());
        // セッションの履歴にも追加
        self.session_messages.push(assistant_message);
    }
}
