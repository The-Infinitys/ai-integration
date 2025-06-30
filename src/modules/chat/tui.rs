use crate::modules::agent::AgentEvent;
use crate::modules::agent::api::{ChatMessage, ChatRole};
use crate::modules::chat::ChatSession;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures_util::stream::StreamExt;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph},
};
use std::{
    io::{self, Stdout},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

enum TuiEvent {
    Input(KeyEvent),
    AgentEvent(AgentEvent),
    Error(String),
    ModelsListed(serde_json::Value),
    ModelSet,
    Reverted,
    StreamComplete,
}

pub struct TuiApp {
    chat_session: ChatSession,
    input: String,
    messages: Vec<ChatMessage>,
    input_scroll: u16,
    should_quit: bool,
    status_message: String,
    last_status_update: Instant,
    event_sender: mpsc::UnboundedSender<TuiEvent>,
    event_receiver: mpsc::UnboundedReceiver<TuiEvent>,
    ai_response_buffer: String,
    tool_output_buffer: String,
    status_text_color: Color,
    message_list_state: ListState,
    is_ai_replying: bool,
    is_user_scrolling: bool,
}

impl TuiApp {
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
                    content: "Type '/exit' or Esc to quit. Use Up/Down arrows to scroll."
                        .to_string(),
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
            input_scroll: 0,
            should_quit: false,
            status_message: "Welcome! Start typing...".to_string(),
            last_status_update: Instant::now(),
            event_sender,
            event_receiver,
            ai_response_buffer: String::new(),
            tool_output_buffer: String::new(),
            status_text_color: Color::White,
            message_list_state: ListState::default(),
            is_ai_replying: false,
            is_user_scrolling: false,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut terminal = setup_terminal()?;
        let res = self.run_app_loop(&mut terminal).await;
        restore_terminal(&mut terminal)?;
        res
    }

    async fn run_app_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
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

            if self.last_status_update.elapsed() > Duration::from_secs(5) {
                self.status_message.clear();
            }

            if let Some(tui_event) = self.event_receiver.recv().await {
                match tui_event {
                    TuiEvent::Input(key_event) => {
                        self.handle_input_event(key_event, terminal.size()?.width)
                            .await?
                    }
                    TuiEvent::AgentEvent(agent_event) => self.handle_agent_event(agent_event).await,
                    TuiEvent::Error(e) => self.handle_error(e),
                    TuiEvent::ModelsListed(models) => self.handle_models_listed(models),
                    TuiEvent::ModelSet => self.handle_model_set(),
                    TuiEvent::Reverted => self.handle_reverted(),
                    TuiEvent::StreamComplete => self.handle_stream_complete().await,
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
            .margin(1)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(size);

        let messages_block = Block::default()
            .borders(Borders::ALL)
            .title("Chat History")
            .border_type(ratatui::widgets::BorderType::Rounded)
            .padding(Padding::horizontal(1));

        let mut list_items: Vec<ListItem> = Vec::new();
        let message_area_width = main_layout[0].width.saturating_sub(2);

        for message in &self.messages {
            let (prefix, color) = match message.role {
                ChatRole::User => ("You: ", Color::Yellow),
                ChatRole::Assistant => ("AI: ", Color::Green),
                ChatRole::System => ("System: ", Color::Cyan),
            };
            list_items.extend(self.create_list_items(
                &message.content,
                prefix,
                color,
                message_area_width,
            ));
        }

        if !self.ai_response_buffer.is_empty() {
            list_items.extend(self.create_list_items(
                &self.ai_response_buffer,
                "AI: ",
                Color::Green,
                message_area_width,
            ));
        }
        if !self.tool_output_buffer.is_empty() {
            list_items.extend(self.create_list_items(
                &self.tool_output_buffer,
                "Tool: ",
                Color::Magenta,
                message_area_width,
            ));
        }

        let list_items_count = list_items.len();
        if list_items_count > 0 {
            if !self.is_user_scrolling {
                self.message_list_state.select(Some(list_items_count - 1));
            } else if let Some(selected) = self.message_list_state.selected() {
                if selected >= list_items_count {
                    self.message_list_state.select(Some(list_items_count - 1));
                }
            }
        }

        let list = List::new(list_items)
            .block(messages_block)
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> ");
        frame.render_stateful_widget(list, main_layout[0], &mut self.message_list_state);

        let input_title = if self.is_ai_replying {
            "Input (AI is replying...)"
        } else {
            "Your Message"
        };
        let input_block = Block::default()
            .borders(Borders::ALL)
            .title(input_title)
            .border_type(ratatui::widgets::BorderType::Rounded);
        let input_text = Paragraph::new(self.input.as_str())
            .style(Style::default().fg(if self.is_ai_replying {
                Color::DarkGray
            } else {
                Color::White
            }))
            .block(input_block)
            .scroll((0, self.input_scroll));
        frame.render_widget(input_text, main_layout[1]);

        if !self.is_ai_replying {
            let cursor_x = main_layout[1].x + 1 + (Span::from(self.input.as_str()).width() as u16)
                - self.input_scroll;
            let cursor_y = main_layout[1].y + 1;
            frame.set_cursor_position((cursor_x, cursor_y));
        }

        let status_bar_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(main_layout[2]);

        let status_text = Paragraph::new(self.status_message.as_str())
            .style(Style::default().fg(self.status_text_color));
        frame.render_widget(status_text, status_bar_layout[0]);

        let help_text = Paragraph::new("Scroll: Up/Down | Quit: Esc")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Right);
        frame.render_widget(help_text, status_bar_layout[1]);
    }

    fn create_list_items<'a>(
        &self,
        content: &'a str,
        prefix: &'a str,
        color: Color,
        width: u16,
    ) -> Vec<ListItem<'a>> {
        let mut items = Vec::new();
        let wrapped_content = textwrap::wrap(content, width as usize);
        let prefix_width = Span::from(prefix).width();

        for (i, line) in wrapped_content.iter().enumerate() {
            let spans = if i == 0 {
                vec![
                    Span::styled(
                        prefix,
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(line.to_string()),
                ]
            } else {
                vec![
                    Span::raw(" ".repeat(prefix_width)),
                    Span::raw(line.to_string()),
                ]
            };
            items.push(ListItem::new(Text::from(Line::from(spans))));
        }
        items
    }

    async fn handle_input_event(&mut self, key_event: KeyEvent, terminal_width: u16) -> Result<()> {
        if self.is_ai_replying {
            if key_event.code == KeyCode::Esc {
                self.should_quit = true;
            }
            return Ok(());
        }

        match key_event.code {
            KeyCode::Enter => self.handle_enter().await,
            KeyCode::Char(c) => self.input.push(c),
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Left => self.input_scroll = self.input_scroll.saturating_sub(1),
            KeyCode::Right => self.input_scroll = self.input_scroll.saturating_add(1),
            KeyCode::Up => {
                self.is_user_scrolling = true;
                if let Some(selected) = self.message_list_state.selected() {
                    if selected > 0 {
                        self.message_list_state.select(Some(selected - 1));
                    }
                }
            }
            KeyCode::Down => {
                self.is_user_scrolling = true;
                if let Some(selected) = self.message_list_state.selected() {
                    self.message_list_state.select(Some(selected + 1));
                } else if !self.messages.is_empty() {
                    self.message_list_state.select(Some(0));
                }
            }
            KeyCode::Esc => self.should_quit = true,
            _ => {}
        }

        let input_width = Span::from(self.input.as_str()).width() as u16;
        let input_area_width = terminal_width.saturating_sub(4);
        if input_width > input_area_width + self.input_scroll {
            self.input_scroll = input_width - input_area_width;
        } else if self.input_scroll > 0 && input_width <= self.input_scroll {
            self.input_scroll = input_width.saturating_sub(1).max(0);
        }

        Ok(())
    }

    async fn handle_enter(&mut self) {
        let input_copy = self.input.trim().to_string();
        if input_copy.is_empty() {
            return;
        }

        self.input.clear();
        self.input_scroll = 0;
        self.is_user_scrolling = false;

        if input_copy.starts_with('/') {
            self.handle_command(&input_copy).await;
        } else {
            self.is_ai_replying = true;
            self.messages.push(ChatMessage {
                role: ChatRole::User,
                content: input_copy.clone(),
            });
            self.set_status_message("Sending message to AI...".to_string(), Color::Yellow);
            self.chat_session.add_user_message(input_copy).await;
            self.start_chat_stream();
        }
    }

    async fn handle_command(&mut self, command: &str) {
        let parts: Vec<&str> = command.split_whitespace().collect();
        let command_name = parts.get(0).unwrap_or(&"");

        match *command_name {
            "/exit" => self.should_quit = true,
            "/model" => {
                if let Some(model_name) = parts.get(1) {
                    let sender = self.event_sender.clone();
                    let model_name = model_name.to_string();
                    let mut chat_session = self.chat_session.clone();
                    tokio::spawn(async move {
                        if chat_session.set_model(model_name).await.is_ok() {
                            sender.send(TuiEvent::ModelSet).unwrap();
                        }
                    });
                }
            }
            "/list" if parts.get(1) == Some(&"models") => {
                let sender = self.event_sender.clone();
                let chat_session = self.chat_session.clone();
                tokio::spawn(async move {
                    match chat_session.list_models().await {
                        Ok(models) => sender.send(TuiEvent::ModelsListed(models)).unwrap(),
                        Err(e) => sender.send(TuiEvent::Error(e.to_string())).unwrap(),
                    }
                });
            }
            "/revert" => {
                self.chat_session.revert_last_turn().await;
                self.messages = self.chat_session.get_messages().await;
                self.event_sender.send(TuiEvent::Reverted).unwrap();
            }
            _ => self.set_status_message(format!("Unknown command: {}", command), Color::Red),
        }
    }

    fn start_chat_stream(&mut self) {
        let sender = self.event_sender.clone();
        let mut chat_session = self.chat_session.clone();
        tokio::spawn(async move {
            match chat_session.start_realtime_chat().await {
                Ok(mut stream) => {
                    while let Some(event_result) = stream.next().await {
                        match event_result {
                            Ok(event) => sender.send(TuiEvent::AgentEvent(event)).unwrap(),
                            Err(e) => sender.send(TuiEvent::Error(e.to_string())).unwrap(),
                        }
                    }
                    sender.send(TuiEvent::StreamComplete).unwrap();
                }
                Err(e) => sender.send(TuiEvent::Error(e.to_string())).unwrap(),
            }
        });
    }

    async fn handle_agent_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::AiResponseChunk(chunk) => {
                self.ai_response_buffer.push_str(&chunk);
                self.set_status_message("AI is typing...".to_string(), Color::LightBlue);
            }
            AgentEvent::ToolCallDetected(tool_call) => {
                self.flush_ai_buffer_to_messages().await;
                self.tool_output_buffer.push_str(&format!(
                    "
--- Tool Call Detected ---
Tool Name: {}
Parameters: {}
-------------------------
",
                    tool_call.tool_name,
                    serde_yaml::to_string(&tool_call.parameters).unwrap_or_default()
                ));
                self.set_status_message("Tool call detected.".to_string(), Color::Yellow);
            }
            AgentEvent::ToolResult(tool_name, result) => {
                self.tool_output_buffer.push_str(&format!(
                    "
--- Tool Result ({}) ---
{}
---------------------------
",
                    tool_name,
                    serde_yaml::to_string(&result).unwrap_or_default()
                ));
                self.set_status_message(format!("Tool {} executed.", tool_name), Color::Green);
            }
            AgentEvent::ToolError(tool_name, error_message) => {
                self.tool_output_buffer.push_str(&format!(
                    "
--- Tool Error ({}) ---
Error: {}
---------------------------
",
                    tool_name, error_message
                ));
                self.set_status_message(format!("Tool {} failed.", tool_name), Color::Red);
            }
            _ => {}
        }
    }

    fn handle_error(&mut self, e: String) {
        self.set_status_message(format!("Error: {}", e), Color::Red);
        self.messages.push(ChatMessage {
            role: ChatRole::System,
            content: format!("Error: {}", e),
        });
        self.is_ai_replying = false;
    }

    fn handle_models_listed(&mut self, models: serde_json::Value) {
        let mut model_list_message = String::from(
            "Available Models:
",
        );
        if let Some(model_list) = models["models"].as_array() {
            for model in model_list {
                if let Some(name) = model["name"].as_str() {
                    model_list_message.push_str(&format!(
                        "- {}
",
                        name
                    ));
                }
            }
        } else {
            model_list_message.push_str(
                "No models found or unexpected response format.
",
            );
        }
        self.messages.push(ChatMessage {
            role: ChatRole::System,
            content: model_list_message,
        });
        self.set_status_message("Models listed.".to_string(), Color::Green);
    }

    fn handle_model_set(&mut self) {
        self.set_status_message(
            format!("Model set to: {}", self.chat_session.current_model),
            Color::Green,
        );
    }

    fn handle_reverted(&mut self) {
        self.set_status_message("Last turn reverted.".to_string(), Color::Green);
    }

    async fn handle_stream_complete(&mut self) {
        self.flush_ai_buffer_to_messages().await;
        let tool_was_used = !self.tool_output_buffer.is_empty();
        self.flush_tool_buffer_to_messages();

        if tool_was_used {
            // If a tool was used, the AI should continue thinking.
            self.set_status_message("AI is considering the tool's result...".to_string(), Color::Yellow);
            self.is_ai_replying = true; // Keep the AI in a replying state
            self.start_chat_stream(); // Start a new chat stream immediately
        } else {
            // If no tool was used, the turn is complete.
            self.set_status_message("AI response complete.".to_string(), Color::Green);
            self.is_ai_replying = false;
        }
    }

    async fn flush_ai_buffer_to_messages(&mut self) {
        if !self.ai_response_buffer.is_empty() {
            let content = self.ai_response_buffer.clone();
            self.chat_session
                .add_assistant_message_to_history(content.clone())
                .await;
            self.messages.push(ChatMessage {
                role: ChatRole::Assistant,
                content,
            });
            self.ai_response_buffer.clear();
        }
    }

    fn flush_tool_buffer_to_messages(&mut self) {
        if !self.tool_output_buffer.is_empty() {
            self.messages.push(ChatMessage {
                role: ChatRole::System,
                content: self.tool_output_buffer.clone(),
            });
            self.tool_output_buffer.clear();
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
