use crate::modules::agent::api::{ChatMessage, ChatRole};
use crate::modules::agent::AgentEvent;
use crate::modules::chat::ChatSession; // 親モジュールからChatSessionをインポート
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
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap, ListState},
    Terminal,
};
use std::{
    io::{self, Stdout},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use textwrap;

/// TUIが非同期操作を処理するためのカスタムイベントタイプ。
enum TuiEvent {
    Input(KeyEvent),
    AgentEvent(AgentEvent),
    Error(String),
    CommandExecuted,
    ListModels(serde_json::Value),
    ModelSet,
    Reverted,
    Quit,
}

/// TUIアプリケーションの状態を表します。
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
    status_text_color: Color,
    message_list_state: ListState,
}

impl TuiApp {
    /// 新しいTuiAppインスタンスを作成します。
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
            status_text_color: Color::White,
            message_list_state: ListState::default(),
        }
    }

    /// TUIアプリケーションのメインループを実行します。
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
                if event::poll(Duration::from_millis(100)).unwrap() {
                    if let Event::Key(key) = event::read().unwrap() {
                        if key.kind == KeyEventKind::Press {
                            event_sender_clone.send(TuiEvent::Input(key)).unwrap();
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        loop {
            terminal.draw(|frame| self.ui(frame))?;

            let timeout_duration = if self.status_message.is_empty() || self.last_status_update.elapsed() > Duration::from_secs(5) {
                Duration::from_millis(100)
            } else {
                Duration::from_millis(50)
            };

            tokio::select! {
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
                                self.message_list_state.select(Some(self.messages.len() - 1));
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
                                self.message_list_state.select(Some(self.messages.len() - 1));
                                self.set_status_message("Models listed.".to_string(), Color::Green);
                            }
                            TuiEvent::ModelSet => {
                                self.set_status_message(format!("Model set to: {}", self.chat_session.current_model), Color::Green);
                            }
                            TuiEvent::Reverted => {
                                self.set_status_message("Last turn reverted.".to_string(), Color::Green);
                                if !self.messages.is_empty() {
                                    self.message_list_state.select(Some(self.messages.len() - 1));
                                } else {
                                    self.message_list_state.select(None);
                                }
                            }
                            TuiEvent::Quit => {
                                self.should_quit = true;
                            }
                        }
                    }
                }
                _ = tokio::time::sleep(timeout_duration) => {
                    if !self.status_message.is_empty() && self.last_status_update.elapsed() > Duration::from_secs(5) {
                        self.status_message.clear();
                    }
                }
            }

            if self.should_quit {
                reader_task.abort();
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

        let messages_block = Block::default().borders(Borders::ALL).title("Chat History");
        let mut list_items: Vec<ListItem> = Vec::new();

        for message in &self.messages {
            let color = match message.role {
                ChatRole::User => Color::Yellow,
                ChatRole::Assistant => Color::Green,
                ChatRole::System => Color::Cyan,
            };
            let prefix = match message.role {
                ChatRole::User => "You: ",
                ChatRole::Assistant => "AI: ",
                ChatRole::System => "System: ",
            };

            let wrapped_content = textwrap::wrap(&message.content, (size.width as usize).saturating_sub(4));
            for (i, line) in wrapped_content.iter().enumerate() {
                if i == 0 {
                    list_items.push(ListItem::new(Text::from(Line::from(vec![
                        Span::styled(prefix, Style::default().fg(color).add_modifier(Modifier::BOLD)),
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

        if !self.messages.is_empty() || !self.ai_response_buffer.is_empty() || !self.tool_output_buffer.is_empty() {
            let total_display_items = self.messages.iter().map(|msg| textwrap::wrap(&msg.content, (size.width as usize).saturating_sub(4)).len()).sum::<usize>()
                + if !self.ai_response_buffer.is_empty() { textwrap::wrap(&self.ai_response_buffer, (size.width as usize).saturating_sub(4)).len() } else { 0 }
                + if !self.tool_output_buffer.is_empty() { textwrap::wrap(&self.tool_output_buffer, (size.width as usize).saturating_sub(4)).len() } else { 0 };

            if total_display_items > 0 {
                 self.message_list_state.select(Some(total_display_items - 1));
            } else {
                self.message_list_state.select(None);
            }
        } else {
            self.message_list_state.select(None);
        }

        frame.render_stateful_widget(list, main_layout[0], &mut self.message_list_state);

        let input_block = Block::default().borders(Borders::ALL).title("Input");
        let input_text = Paragraph::new(Text::from(self.input.as_str()))
            .style(Style::default().fg(Color::White))
            .block(input_block)
            .wrap(Wrap { trim: false })
            .scroll((0, self.input_scroll));
        frame.render_widget(input_text, main_layout[1]);

        let current_input_width = Span::from(&self.input).width() as u16;
        let cursor_x = main_layout[1].x + current_input_width.saturating_sub(self.input_scroll) + 1;
        let cursor_y = main_layout[1].y + 1;
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));

        let status_block = Block::default().borders(Borders::NONE);
        let status_text = Paragraph::new(Text::from(self.status_message.as_str()))
            .style(Style::default().fg(self.status_text_color))
            .block(status_block);
        frame.render_widget(status_text, main_layout[2]);
    }

    async fn handle_input_event(&mut self, key_event: KeyEvent, terminal_width: u16) -> Result<()> {
        if key_event.code == KeyCode::Esc {
            let _ = self.event_sender.send(TuiEvent::Quit);
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
                    self.message_list_state.select(Some(self.messages.len() - 1));

                    self.set_status_message("Sending message to AI...".to_string(), Color::Yellow);

                    self.chat_session.add_user_message(input_copy).await;

                    let sender = self.event_sender.clone();
                    let mut chat_session = self.chat_session.start_realtime_chat().await.unwrap(); // エラーハンドリングを改善すべき

                    tokio::spawn(async move {
                        while let Some(event_result) = chat_session.next().await {
                            match event_result {
                                Ok(event) => {
                                    let _ = sender.send(TuiEvent::AgentEvent(event));
                                }
                                Err(e) => {
                                    let _ = sender.send(TuiEvent::Error(format!("Stream Error: {}", e)));
                                    break;
                                }
                            }
                        }
                        let _ = sender.send(TuiEvent::AgentEvent(AgentEvent::ToolResult(
                            "StreamingComplete".to_string(),
                            serde_json::Value::String("".to_string()),
                        )));
                    });
                }
            }
            KeyCode::Backspace => {
                self.input.pop();
                let rendered_input_width = Span::from(&self.input).width() as u16;
                let input_area_width = terminal_width.saturating_sub(4);
                if self.input_scroll > 0 && rendered_input_width < self.input_scroll + input_area_width {
                    self.input_scroll = rendered_input_width.saturating_sub(input_area_width.saturating_sub(1));
                    if self.input_scroll > rendered_input_width {
                        self.input_scroll = 0;
                    }
                }
            }
            KeyCode::Left => {
                self.input_scroll = self.input_scroll.saturating_sub(1);
            }
            KeyCode::Right => {
                if (self.input_scroll as usize) < Span::from(&self.input).width() {
                    self.input_scroll += 1;
                }
            }
            KeyCode::Char(c) => {
                self.input.push(c);
                let input_width_in_chars = Span::from(&self.input).width() as u16;
                let input_field_display_width = terminal_width.saturating_sub(4);
                if input_width_in_chars.saturating_sub(self.input_scroll) >= input_field_display_width {
                    self.input_scroll = input_width_in_chars.saturating_sub(input_field_display_width.saturating_sub(1));
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_command(&mut self, command: &str) {
        if command.eq_ignore_ascii_case("/exit") {
            let _ = self.event_sender.send(TuiEvent::Quit);
        } else if command.starts_with("/model ") {
            let model_name = command.trim_start_matches("/model ").trim().to_string();
            if self.chat_session.set_model(model_name).await.is_ok() {
                let _ = self.event_sender.send(TuiEvent::ModelSet);
            }
            let _ = self.event_sender.send(TuiEvent::CommandExecuted);
        } else if command.eq_ignore_ascii_case("/list models") {
            self.set_status_message("Listing models...".to_string(), Color::Yellow);
            let sender = self.event_sender.clone();
            let models = self.chat_session.list_models().await;
            tokio::spawn(async move {
                match models {
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
            self.chat_session.revert_last_turn().await;
            self.messages = self.chat_session.get_messages().await;
            let _ = self.event_sender.send(TuiEvent::Reverted);
            let _ = self.event_sender.send(TuiEvent::CommandExecuted);
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
            AgentEvent::AddMessageToHistory(_) => {}
            AgentEvent::ToolCallDetected(tool_call) => {
                if !self.ai_response_buffer.is_empty() {
                    self.messages.push(ChatMessage {
                        role: ChatRole::Assistant,
                        content: self.ai_response_buffer.clone(),
                    });
                    self.ai_response_buffer.clear();
                    self.message_list_state.select(Some(self.messages.len() - 1));
                }
                self.tool_output_buffer.push_str(&format!(
                    "\n--- Tool Call Detected ---\nTool Name: {}\nParameters: {}\n-------------------------\n",
                    tool_call.tool_name,
                    serde_yaml::to_string(&tool_call.parameters).unwrap_or_else(|_| "Serialization Error".to_string())
                ));
                self.set_status_message("Tool call detected.".to_string(), Color::Yellow);
            }
            AgentEvent::ToolExecuting(tool_name) => {
                self.set_status_message(format!("Executing tool: {}...", tool_name), Color::Cyan);
            }
            AgentEvent::ToolResult(tool_name, result) => {
                if tool_name == "StreamingComplete" {
                    if !self.ai_response_buffer.is_empty() {
                        self.chat_session.add_assistant_message_to_history(self.ai_response_buffer.clone()).await;
                        self.messages.push(ChatMessage {
                            role: ChatRole::Assistant,
                            content: self.ai_response_buffer.clone(),
                        });
                        self.ai_response_buffer.clear();
                    }
                    if !self.tool_output_buffer.is_empty() {
                         self.messages.push(ChatMessage {
                            role: ChatRole::System,
                            content: self.tool_output_buffer.clone(),
                        });
                        self.tool_output_buffer.clear();
                    }
                    self.set_status_message("AI response complete.".to_string(), Color::Green);
                    self.message_list_state.select(Some(self.messages.len() - 1));
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
            AgentEvent::UserMessageAdded => {}
            AgentEvent::AttemptingToolDetection => {
                self.set_status_message("Attempting tool detection...".to_string(), Color::Yellow);
                if !self.ai_response_buffer.is_empty() {
                    self.messages.push(ChatMessage {
                        role: ChatRole::Assistant,
                        content: self.ai_response_buffer.clone(),
                    });
                    self.ai_response_buffer.clear();
                    self.message_list_state.select(Some(self.messages.len() - 1));
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