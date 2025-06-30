// src/modules/tui.rs
use crate::modules::agent::api::{ChatMessage, ChatRole};
use crate::modules::agent::{AIAgent, AgentEvent}; // AgentEvent must derive Debug!
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::stream::StreamExt;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Position},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Terminal,
};
use std::{
    io::{self, Stdout},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, Mutex};
use textwrap;

/// Custom event types for the TUI to handle asynchronous operations.
enum TuiEvent {
    Input(KeyEvent),
    AgentEvent(AgentEvent),
    Error(String),
    CommandExecuted,
    ListModels(serde_json::Value),
    ModelSet,
    Reverted,
    Quit, // アプリケーション終了シグナルを追加
}

/// Represents the state of the TUI application.
pub struct TuiApp {
    chat_session: ChatSession,
    input: String,
    messages: Vec<ChatMessage>,
    _scroll_offset: u16,
    input_scroll: u16,
    should_quit: bool,
    _command_mode: bool,
    status_message: String,
    last_status_update: Instant,
    event_sender: mpsc::UnboundedSender<TuiEvent>,
    event_receiver: mpsc::UnboundedReceiver<TuiEvent>,
    ai_response_buffer: String,
    tool_output_buffer: String,
    _last_user_input: Option<String>,
    status_text_color: Color,
}

impl TuiApp {
    /// Creates a new TuiApp instance.
    pub fn new(ollama_base_url: String, default_ollama_model: String) -> Self {
        let chat_session = ChatSession::new(ollama_base_url, default_ollama_model.clone());
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        Self {
            chat_session,
            input: String::new(),
            messages: vec![
                ChatMessage {
                    role: ChatRole::System,
                    content: format!("Default Ollama Model: {}", default_ollama_model),
                },
                ChatMessage {
                    role: ChatRole::System,
                    content: "AI Integration Chat Session".to_string(),
                },
                ChatMessage {
                    role: ChatRole::System,
                    content: "Type '/exit' to quit.".to_string(),
                },
                ChatMessage {
                    role: ChatRole::System,
                    content: "Type '/model <model_name>' to change model.".to_string(),
                },
                ChatMessage {
                    role: ChatRole::System,
                    content: "Type '/list models' to list available models.".to_string(),
                },
                ChatMessage {
                    role: ChatRole::System,
                    content: "Type '/revert' to undo the last turn.".to_string(),
                },
            ],
            _scroll_offset: 0,
            input_scroll: 0,
            should_quit: false,
            _command_mode: false,
            status_message: "Welcome! Start typing...".to_string(),
            last_status_update: Instant::now(),
            event_sender,
            event_receiver,
            ai_response_buffer: String::new(),
            tool_output_buffer: String::new(),
            _last_user_input: None,
            status_text_color: Color::White,
        }
    }

    /// Runs the main TUI application loop.
    pub async fn run(&mut self) -> Result<()> {
        let mut terminal = setup_terminal()?;
        let res = self.run_app_loop(&mut terminal).await;
        restore_terminal(&mut terminal)?;
        res
    }

    async fn run_app_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        let event_sender_clone = self.event_sender.clone();
        let reader_task = tokio::spawn(async move {
            loop {
                // Poll for event with a timeout, to allow other futures to run
                if event::poll(Duration::from_millis(100)).unwrap() {
                    if let Event::Key(key) = event::read().unwrap() {
                        if key.kind == KeyEventKind::Press {
                            event_sender_clone.send(TuiEvent::Input(key)).unwrap();
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await; // CPU使用率を抑えるための短い遅延
            }
        });


        loop {
            // Draw the UI
            terminal.draw(|frame| self.ui(frame))?;

            let timeout_duration = if self.status_message.is_empty() || self.last_status_update.elapsed() > Duration::from_secs(5) {
                Duration::from_millis(100)
            } else {
                Duration::from_millis(50)
            };

            tokio::select! {
                // Receive events from the channel
                event = self.event_receiver.recv() => {
                    if let Some(tui_event) = event {
                        match tui_event {
                            TuiEvent::Input(key_event) => {
                                self.handle_input_event(key_event, terminal.size()?.width).await?;
                            }
                            TuiEvent::AgentEvent(agent_event) => {
                                self.handle_agent_event(agent_event).await;
                            }
                            TuiEvent::Error(e) => {
                                self.set_status_message(format!("Error: {}", e), Color::Red);
                                self.messages.push(ChatMessage {
                                    role: ChatRole::System,
                                    content: format!("Error: {}", e),
                                });
                            }
                            TuiEvent::CommandExecuted => {
                                self.set_status_message("Command executed successfully.".to_string(), Color::Green);
                                self.input.clear();
                                self.input_scroll = 0;
                            }
                            TuiEvent::ListModels(models) => {
                                let mut model_list_message = String::from("Available Models:\n");
                                if let Some(model_list) = models["models"].as_array() {
                                    for model in model_list {
                                        if let Some(name) = model["name"].as_str() {
                                            model_list_message.push_str(&format!("- {}\n", name));
                                        }
                                    }
                                } else {
                                    model_list_message.push_str("No models found or unexpected response format.\n");
                                }
                                self.messages.push(ChatMessage {
                                    role: ChatRole::System,
                                    content: model_list_message,
                                });
                                self.set_status_message("Models listed.".to_string(), Color::Green);
                            }
                            TuiEvent::ModelSet => {
                                self.set_status_message(format!("Model set to: {}", self.chat_session.current_model), Color::Green);
                            }
                            TuiEvent::Reverted => {
                                self.set_status_message("Last turn reverted.".to_string(), Color::Green);
                            }
                            TuiEvent::Quit => {
                                self.should_quit = true; // 終了シグナルを受け取ったらフラグを設定
                            }
                        }
                    }
                }
                // Timeout to refresh the UI or clear status message
                _ = tokio::time::sleep(timeout_duration) => {
                    if !self.status_message.is_empty() && self.last_status_update.elapsed() > Duration::from_secs(5) {
                        self.status_message.clear();
                    }
                }
            }

            if self.should_quit {
                reader_task.abort(); // イベントリーダータスクを中止
                break;
            }
        }
        Ok(())
    }

    fn ui(&mut self, frame: &mut ratatui::Frame) {
        let size = frame.area();
        let main_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(3), Constraint::Length(1)])
            .split(size);

        // Chat messages area
        let messages_block = Block::default().borders(Borders::ALL).title("Chat History");
        let mut list_items: Vec<ListItem> = Vec::new();

        for message in &self.messages {
            let _color = match message.role {
                ChatRole::User => Color::Blue,
                ChatRole::Assistant => Color::Green,
                ChatRole::System => Color::Cyan, // ChatRole::Tool の代わりに System を使用
            };
            let prefix = match message.role {
                ChatRole::User => "You: ",
                ChatRole::Assistant => "AI: ",
                ChatRole::System => "Tool: ", // ChatRole::Tool の代わりに System を使用
            };

            let wrapped_content = textwrap::wrap(&message.content, (size.width as usize).saturating_sub(4));
            for (i, line) in wrapped_content.iter().enumerate() {
                if i == 0 {
                    list_items.push(ListItem::new(Text::from(Line::from(vec![
                        Span::styled(prefix, Style::default().fg(_color).add_modifier(Modifier::BOLD)),
                        Span::styled(line.to_string(), Style::default().fg(Color::LightGreen)),
                    ]))));
                } else {
                    list_items.push(ListItem::new(Text::from(Line::from(vec![
                        Span::styled("     ", Style::default()),
                        Span::styled(line.to_string(), Style::default().fg(Color::LightGreen)),
                    ]))));
                }
            }
        }

        // Add pending AI response chunks
        if !self.ai_response_buffer.is_empty() {
            let wrapped_content = textwrap::wrap(&self.ai_response_buffer, (size.width as usize).saturating_sub(4));
            for (i, line) in wrapped_content.iter().enumerate() {
                if i == 0 {
                    list_items.push(ListItem::new(Text::from(Line::from(vec![
                        Span::styled("AI: ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                        Span::styled(line.to_string(), Style::default().fg(Color::LightGreen)),
                    ]))));
                } else {
                    list_items.push(ListItem::new(Text::from(Line::from(vec![
                        Span::styled("     ", Style::default()),
                        Span::styled(line.to_string(), Style::default().fg(Color::LightGreen)),
                    ]))));
                }
            }
        }

        // Add pending tool output
        if !self.tool_output_buffer.is_empty() {
            let wrapped_content = textwrap::wrap(&self.tool_output_buffer, (size.width as usize).saturating_sub(4));
            for (i, line) in wrapped_content.iter().enumerate() {
                if i == 0 {
                    list_items.push(ListItem::new(Text::from(Line::from(vec![
                        Span::styled("Tool: ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                        Span::styled(line.to_string(), Style::default().fg(Color::LightYellow)),
                    ]))));
                } else {
                    list_items.push(ListItem::new(Text::from(Line::from(vec![
                        Span::styled("      ", Style::default()),
                        Span::styled(line.to_string(), Style::default().fg(Color::LightYellow)),
                    ]))));
                }
            }
        }


        let list = List::new(list_items)
            .block(messages_block)
            .highlight_style(Style::default().bg(Color::DarkGray));
        frame.render_widget(list, main_layout[0]);

        // Input area
        let input_block = Block::default().borders(Borders::ALL).title("Input");
        let input_text = Paragraph::new(Text::from(self.input.as_str()))
            .style(Style::default().fg(Color::White))
            .block(input_block)
            .wrap(Wrap { trim: false })
            .scroll((0, self.input_scroll));
        frame.render_widget(input_text, main_layout[1]);

        // Adjust cursor position based on Unicode width
        let cursor_x = main_layout[1].x + (Span::from(&self.input).width() as u16).saturating_sub(self.input_scroll) + 1;
        let cursor_y = main_layout[1].y + 1;
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));

        // Status bar
        let status_block = Block::default().borders(Borders::NONE);
        let status_text = Paragraph::new(Text::from(self.status_message.as_str()))
            .style(Style::default().fg(self.status_text_color))
            .block(status_block);
        frame.render_widget(status_text, main_layout[2]);
    }

    async fn handle_input_event(&mut self, key_event: KeyEvent, terminal_width: u16) -> Result<()> {
        if key_event.code == KeyCode::Esc {
            self.event_sender.send(TuiEvent::Quit)?; // Quitイベントを送信
            return Ok(());
        }

        match key_event.code {
            KeyCode::Enter => {
                let input_copy = self.input.trim().to_string();
                self.input.clear();
                self.input_scroll = 0;

                if input_copy.is_empty() {
                    return Ok(());
                }

                if input_copy.starts_with('/') {
                    self.handle_command(&input_copy).await;
                } else {
                    self.messages.push(ChatMessage {
                        role: ChatRole::User,
                        content: input_copy.clone(),
                    });
                    self.set_status_message("Sending message to AI...".to_string(), Color::Yellow);
                    self._last_user_input = Some(input_copy.clone());

                    let sender = self.event_sender.clone();
                    let agent_arc_for_spawn = Arc::clone(&self.chat_session.agent);

                    tokio::spawn(async move {
                        let mut agent_locked_in_spawn = agent_arc_for_spawn.lock().await;
                        agent_locked_in_spawn.add_message_to_history(ChatMessage {
                            role: ChatRole::User,
                            content: input_copy,
                        });
                        let current_turn_messages = agent_locked_in_spawn.messages.clone();
                        drop(agent_locked_in_spawn);

                        match AIAgent::chat_with_tools_realtime(agent_arc_for_spawn.clone(), current_turn_messages).await {
                            Ok(mut stream) => {
                                while let Some(event_result) = stream.next().await {
                                    match event_result {
                                        Ok(event) => {
                                            sender.send(TuiEvent::AgentEvent(event)).unwrap();
                                        }
                                        Err(e) => {
                                            sender.send(TuiEvent::Error(format!("Stream Error: {}", e))).unwrap();
                                            break;
                                        }
                                    }
                                }
                                sender.send(TuiEvent::AgentEvent(AgentEvent::ToolResult(
                                    "StreamingComplete".to_string(),
                                    serde_json::Value::String("".to_string()),
                                ))).unwrap();
                            }
                            Err(e) => {
                                sender.send(TuiEvent::Error(format!("Chat Init Error: {}", e))).unwrap();
                            }
                        }
                    });
                }
            }
            KeyCode::Backspace => {
                self.input.pop();
                // カーソルが入力フィールドの左端を超えないようにスクロールオフセットを調整
                if self.input_scroll > 0 && Span::from(&self.input).width() < self.input_scroll as usize {
                    self.input_scroll = Span::from(&self.input).width() as u16;
                }
            }
            KeyCode::Left => {
                self.input_scroll = self.input_scroll.saturating_sub(1);
            }
            KeyCode::Right => {
                if (self.input_scroll as usize) < Span::from(&self.input).width() { // Unicode width for comparison
                    self.input_scroll += 1;
                }
            }
            KeyCode::Char(c) => {
                self.input.push(c);
                // 入力フィールドの幅を超えたら自動的にスクロールオフセットを調整
                let input_width = (terminal_width as usize).saturating_sub(4); // 境界線などを考慮
                if (Span::from(&self.input).width() as u16 - self.input_scroll) as usize >= input_width { // Unicode width for calculation
                    self.input_scroll = (Span::from(&self.input).width() as u16).saturating_sub(input_width as u16 -1);
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_command(&mut self, command: &str) {
        if command.eq_ignore_ascii_case("/exit") {
            self.event_sender.send(TuiEvent::Quit).unwrap(); // TuiEvent::Quitを送信
        } else if command.starts_with("/model ") {
            let model_name = command.trim_start_matches("/model ").trim().to_string();
            let sender = self.event_sender.clone();
            let mut agent_locked = self.chat_session.agent.lock().await;
            agent_locked.set_model(model_name.clone());
            self.chat_session.current_model = model_name;
            let _ = sender.send(TuiEvent::ModelSet);
            let _ = sender.send(TuiEvent::CommandExecuted);
        } else if command.eq_ignore_ascii_case("/list models") {
            let sender = self.event_sender.clone();
            let chat_session_clone = Arc::clone(&self.chat_session.agent);
            tokio::spawn(async move {
                let agent_locked = chat_session_clone.lock().await;
                match agent_locked.list_available_models().await {
                    Ok(models) => {
                        let _ = sender.send(TuiEvent::ListModels(models));
                    }
                    Err(e) => {
                        let _ = sender.send(TuiEvent::Error(format!("Failed to list models: {}", e)));
                    }
                }
                let _ = sender.send(TuiEvent::CommandExecuted);
            });
        } else if command.eq_ignore_ascii_case("/revert") {
            let sender = self.event_sender.clone();
            let mut agent_locked = self.chat_session.agent.lock().await;
            agent_locked.revert_last_user_message();
            self.messages.pop();
            while let Some(msg) = self.messages.last() {
                if msg.role != ChatRole::User {
                    self.messages.pop();
                } else {
                    break;
                }
            }
            let _ = sender.send(TuiEvent::Reverted);
            let _ = sender.send(TuiEvent::CommandExecuted);
        } else {
            self.set_status_message(format!("Unknown command: {}", command), Color::Red);
            let _ = self.event_sender.send(TuiEvent::CommandExecuted);
        }
    }

    async fn handle_agent_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::AiResponseChunk(chunk) => {
                self.ai_response_buffer.push_str(&chunk);
                self.set_status_message("AI is typing...".to_string(), Color::LightBlue);
            }
            AgentEvent::AddMessageToHistory(message) => {
                if message.role == ChatRole::Assistant && self.ai_response_buffer == message.content {
                    self.messages.push(message);
                    self.ai_response_buffer.clear();
                    self.set_status_message("AI response received.".to_string(), Color::Green);
                } else if message.role == ChatRole::System { // ChatRole::Tool の代わりに System を使用
                    if self.tool_output_buffer == message.content {
                        self.messages.push(message);
                        self.tool_output_buffer.clear();
                        self.set_status_message("Tool output received.".to_string(), Color::Green);
                    }
                }
            }
            AgentEvent::ToolCallDetected(tool_call) => {
                self.tool_output_buffer.push_str(&format!(
                    "\n--- Tool Call Detected ---\nTool Name: {}\nParameters: {}\n-------------------------\n",
                    tool_call.tool_name,
                    serde_yaml::to_string(&tool_call.parameters).unwrap_or_else(|_| "Serialization Error".to_string())
                ));
                self.ai_response_buffer.clear();
                self.set_status_message("Tool call detected.".to_string(), Color::Yellow);
            }
            AgentEvent::ToolExecuting(tool_name) => {
                self.set_status_message(format!("Executing tool: {}...", tool_name), Color::Cyan);
            }
            AgentEvent::ToolResult(tool_name, result) => {
                if tool_name == "StreamingComplete" {
                    if !self.ai_response_buffer.is_empty() {
                        self.messages.push(ChatMessage {
                            role: ChatRole::Assistant,
                            content: self.ai_response_buffer.clone(),
                        });
                        self.ai_response_buffer.clear();
                    }
                    if !self.tool_output_buffer.is_empty() {
                         self.messages.push(ChatMessage {
                            role: ChatRole::System, // ChatRole::Tool の代わりに System を使用
                            content: self.tool_output_buffer.clone(),
                        });
                        self.tool_output_buffer.clear();
                    }
                    self.set_status_message("AI response complete.".to_string(), Color::Green);
                } else {
                    self.tool_output_buffer.push_str(&format!(
                        "\n--- Tool Result ({}) ---\n{}\n---------------------------\n",
                        tool_name,
                        serde_yaml::to_string(&result).unwrap_or_else(|_| "Serialization Error".to_string())
                    ));
                    self.set_status_message(format!("Tool {} executed successfully.", tool_name), Color::Green);
                }
            }
            AgentEvent::ToolError(tool_name, error_message) => {
                self.tool_output_buffer.push_str(&format!(
                    "\n--- Tool Error ({}) ---\nError: {}\n---------------------------\n",
                    tool_name, error_message
                ));
                self.set_status_message(format!("Tool {} failed: {}", tool_name, error_message), Color::Red);
            }
            AgentEvent::Thinking(message) => {
                self.set_status_message(format!("AI thinking: {}", message), Color::Magenta);
            }
            AgentEvent::UserMessageAdded => { /* Handled elsewhere */ }
            AgentEvent::AttemptingToolDetection => {
                self.set_status_message("Attempting tool detection...".to_string(), Color::Yellow);
                if !self.ai_response_buffer.is_empty() {
                    self.messages.push(ChatMessage {
                        role: ChatRole::Assistant,
                        content: self.ai_response_buffer.clone(),
                    });
                    self.ai_response_buffer.clear();
                }
            }
            AgentEvent::PendingDisplayContent(content) => {
                self.ai_response_buffer.push_str(&content);
            }
            AgentEvent::ToolBlockParseWarning(yaml_content) => {
                self.tool_output_buffer.push_str(&format!(
                    "\nWARNING: Could not parse tool block YAML:\n{}\n",
                    yaml_content
                ));
                self.set_status_message("Tool block parse warning.".to_string(), Color::Red);
            }
            AgentEvent::YamlParseError(error_msg, yaml_content) => {
                self.tool_output_buffer.push_str(&format!(
                    "\nYAML Tool Call Parse Error: {}\nContent:\n{}\n",
                    error_msg, yaml_content
                ));
                self.set_status_message("YAML parse error for tool call.".to_string(), Color::Red);
            }
        }
    }

    fn set_status_message(&mut self, message: String, color: Color) {
        self.status_message = message;
        self.status_text_color = color;
        self.last_status_update = Instant::now();
    }
}

/// AIエージェントとの単一のチャットセッションを表します。
pub struct ChatSession {
    agent: Arc<Mutex<AIAgent>>,
    _session_messages: Vec<ChatMessage>,
    pub current_model: String,
}

impl ChatSession {
    /// 新しいチャットセッションを作成します。
    pub fn new(base_url: String, default_model: String) -> Self {
        let agent = Arc::new(Mutex::new(AIAgent::new(base_url, default_model.clone())));
        ChatSession {
            agent,
            _session_messages: vec![],
            current_model: default_model,
        }
    }
}

// Helper functions for terminal setup and restoration
fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
