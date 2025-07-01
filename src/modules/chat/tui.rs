use crate::modules::agent::AgentEvent;
use crate::modules::agent::api::{AIProvider, ChatMessage, ChatRole};
use crate::modules::chat::ChatSession;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
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
use tokio::task::JoinHandle;

use syntect::easy::HighlightLines;
use syntect::highlighting::{ Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::{LinesWithEndings, as_24_bit_terminal_escaped};

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
    chat_stream_handle: Option<JoinHandle<()>>,
    syntax_set: SyntaxSet,
    theme: Theme,
    default_system_prompt: String,
}

impl TuiApp {
    pub fn new(provider: AIProvider, base_url: String, default_model: String) -> Self {
        let chat_session = ChatSession::new(provider, base_url, default_model.clone());
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        Self {
            chat_session,
            input: String::new(),
            messages: vec![],
            input_scroll: 0,
            should_quit: false,
            status_message: "Welcome! Type /help for commands.".to_string(),
            last_status_update: Instant::now(),
            event_sender,
            event_receiver,
            ai_response_buffer: String::new(),
            tool_output_buffer: String::new(),
            status_text_color: Color::White,
            message_list_state: ListState::default(),
            is_ai_replying: false,
            is_user_scrolling: false,
            chat_stream_handle: None,
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme: ThemeSet::load_defaults().themes["base16-ocean.dark"].clone(),
            default_system_prompt: include_str!("../default-prompt.md").to_string(),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut terminal = setup_terminal()?;
        self.messages = self.chat_session.get_messages().await;
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
                            if event_sender_clone.send(TuiEvent::Input(key)).is_err() {
                                break; // Stop if receiver is dropped
                            }
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
            .borders(Borders::NONE)
            .title("")
            .padding(Padding::horizontal(1));

        let mut list_items: Vec<ListItem> = Vec::new();
        let message_area_width = main_layout[0].width.saturating_sub(2);

        for message in &self.messages {
            if message.role == ChatRole::System && message.content == self.default_system_prompt {
                continue; // Skip rendering the default system prompt
            }
            list_items.extend(self.create_list_item(
                &message.content,
                match message.role {
                    ChatRole::User => "You: ",
                    ChatRole::Assistant => "AI: ",
                    ChatRole::System => "System: ",
                    ChatRole::Tool => "Tool: ",
                },
                match message.role {
                    ChatRole::User => Color::Yellow,
                    ChatRole::Assistant => Color::Green,
                    ChatRole::System => Color::Cyan,
                    ChatRole::Tool => Color::Blue,
                },
                message_area_width,
            ));
        }

        // Display live AI response and tool output
        if !self.ai_response_buffer.is_empty() {
            list_items.extend(self.create_list_item(
                &self.ai_response_buffer,
                "AI: ",
                Color::Green,
                message_area_width,
            ));
        }

        if !self.tool_output_buffer.is_empty() {
            list_items.extend(self.create_list_item(
                &self.tool_output_buffer,
                "Tool: ",
                Color::Blue,
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
            "Input (AI is replying... Ctrl+C to cancel)"
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
            let cursor_x = main_layout[1].x + 1 + (Span::from(&self.input[..]).width() as u16)
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

        let help_text = Paragraph::new("Scroll: Up/Down | Quit: Esc | Help: /help")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Right);
        frame.render_widget(help_text, status_bar_layout[1]);
    }

    fn create_list_item<'a>(
        &self,
        content: &'a str,
        prefix: &'a str,
        color: Color,
        width: u16,
    ) -> Vec<ListItem<'a>> {
        let mut list_items: Vec<ListItem<'a>> = Vec::new();
        let mut in_code_block = false;
        let mut code_block_lang = "txt";
        let mut is_first_line_of_message = true;

        for line_str in LinesWithEndings::from(content) {
            if line_str.trim().starts_with("```") {
                in_code_block = !in_code_block;
                if in_code_block {
                    let lang_specifier = line_str.trim().trim_start_matches("```").trim();
                    if !lang_specifier.is_empty() {
                        code_block_lang = lang_specifier;
                    } else {
                        code_block_lang = "txt";
                    }
                }
                list_items.push(ListItem::new(Text::from(Line::from(Span::raw(line_str.to_string())))));
                is_first_line_of_message = false; // Reset after code block marker
                continue;
            }

            if in_code_block {
                let syntax = self
                    .syntax_set
                    .find_syntax_by_token(code_block_lang)
                    .unwrap_or_else(|| self.syntax_set.find_syntax_by_extension("txt").unwrap());
                let mut code_highlighter = HighlightLines::new(&syntax, &self.theme);
                let highlighted_line: String = match code_highlighter.highlight_line(line_str.trim_end(), &self.syntax_set) {
                    Ok(regions) => as_24_bit_terminal_escaped(&regions[..], false),
                    Err(_) => line_str.to_string(), // Fallback to raw line if highlighting fails
                };
                list_items.push(ListItem::new(Text::from(Line::from(Span::raw(highlighted_line)))));
                is_first_line_of_message = false; // Reset after code line
            } else {
                let wrapped_content = textwrap::wrap(line_str.trim_end(), width as usize);
                let prefix_width = Span::from(prefix).width();

                for wrapped_line in wrapped_content.iter() {
                    let spans = if is_first_line_of_message {
                        vec![
                            Span::styled(
                                prefix,
                                Style::default().fg(color).add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(wrapped_line.to_string()),
                        ]
                    } else {
                        vec![
                            Span::raw(" ".repeat(prefix_width)),
                            Span::raw(wrapped_line.to_string()),
                        ]
                    };
                    list_items.push(ListItem::new(Text::from(Line::from(spans))));
                    is_first_line_of_message = false; // After the first line, set to false
                }
            }
        }
        list_items
    }

    async fn handle_input_event(&mut self, key_event: KeyEvent, terminal_width: u16) -> Result<()> {
        // AI応答中でもCtrl+CとEscは処理する
        if self.is_ai_replying {
            match key_event.code {
                KeyCode::Char('c') if key_event.modifiers == KeyModifiers::CONTROL => {
                    self.cancel_ai_response();
                }
                KeyCode::Esc => {
                    self.should_quit = true;
                }
                _ => {} // 他のキーは無視
            }
        }

        // AI応答中かどうかに関わらず処理するキー
        match key_event.code {
            KeyCode::Enter => {
                if !self.is_ai_replying {
                    // AI応答中はEnterを無視
                    self.handle_enter().await;
                }
            }
            KeyCode::Char('!') if self.input.is_empty() && !self.is_ai_replying => {
                self.input.push_str("/shell ");
            }
            KeyCode::Char(c) if !self.is_ai_replying => {
                self.input.push(c);
            }
            KeyCode::Backspace if !self.is_ai_replying => {
                self.input.pop();
            }
            KeyCode::Left if !self.is_ai_replying => {
                self.input_scroll = self.input_scroll.saturating_sub(1);
            }
            KeyCode::Right if !self.is_ai_replying => {
                self.input_scroll = self.input_scroll.saturating_add(1);
            }
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
                // ここでlist_items_countを正確に計算する必要がある
                let mut temp_list_items: Vec<ListItem> = Vec::new();
                let message_area_width = terminal_width.saturating_sub(2);

                for message in &self.messages {
                    temp_list_items.extend(self.create_list_item(
                        &message.content,
                        "",           // prefixはここでは不要
                        Color::White, // colorはここでは不要
                        message_area_width,
                    ));
                }
                if !self.ai_response_buffer.is_empty() {
                    temp_list_items.extend(self.create_list_item(
                        &self.ai_response_buffer,
                        "",
                        Color::White,
                        message_area_width,
                    ));
                }
                if !self.tool_output_buffer.is_empty() {
                    temp_list_items.extend(self.create_list_item(
                        &self.tool_output_buffer,
                        "",
                        Color::White,
                        message_area_width,
                    ));
                }
                let list_items_count = temp_list_items.len();

                if let Some(selected) = self.message_list_state.selected() {
                    if selected < list_items_count - 1 {
                        self.message_list_state.select(Some(selected + 1));
                    } else {
                        // 既に一番下までスクロールしている場合
                        self.is_user_scrolling = false; // ユーザーによるスクロールを解除
                    }
                } else if !temp_list_items.is_empty() {
                    self.message_list_state.select(Some(0));
                }
            }
            KeyCode::Esc => self.should_quit = true,
            _ => {}
        }

        // 入力フィールドのスクロールロジックはAI応答中以外のみ適用
        if !self.is_ai_replying {
            let input_width = Span::from(self.input.as_str()).width() as u16;
            let input_area_width = terminal_width.saturating_sub(4);
            if input_width > input_area_width + self.input_scroll {
                self.input_scroll = input_width - input_area_width;
            } else if self.input_scroll > 0 && input_width <= self.input_scroll {
                self.input_scroll = input_width.saturating_sub(1);
            }
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
        let command_name = parts.first().unwrap_or(&"");
        let command_copy = command.to_string();

        match *command_name {
            "/exit" | "/quit" => self.should_quit = true,
            "/shell" => {
                self.is_ai_replying = true;
                self.messages.push(ChatMessage {
                    role: ChatRole::User,
                    content: command_copy.clone(),
                });
                self.set_status_message("Executing shell command...".to_string(), Color::Yellow);
                self.chat_session.add_user_message(command_copy).await;
                self.start_chat_stream();
            }
            "/model" => {
                if let Some(model_name) = parts.get(1) {
                    let sender = self.event_sender.clone();
                    let model_name = model_name.to_string();
                    let mut chat_session = self.chat_session.clone();
                    tokio::spawn(async move {
                        if chat_session.set_model(model_name).await.is_ok() {
                            let _ = sender.send(TuiEvent::ModelSet);
                        }
                    });
                } else {
                    self.set_status_message("Usage: /model <model_name>".to_string(), Color::Red);
                }
            }
            "/list" if parts.get(1) == Some(&"models") => {
                let sender = self.event_sender.clone();
                let chat_session = self.chat_session.clone();
                tokio::spawn(async move {
                    match chat_session.list_models().await {
                        Ok(models) => {
                            let _ = sender.send(TuiEvent::ModelsListed(models));
                        }
                        Err(e) => {
                            let _ = sender.send(TuiEvent::Error(e.to_string()));
                        }
                    }
                });
            }
            "/revert" => {
                self.chat_session.revert_last_turn().await;
                self.messages = self.chat_session.get_messages().await;
                let _ = self.event_sender.send(TuiEvent::Reverted);
            }
            "/clear" => {
                self.chat_session.clear_history().await;
                self.messages = self.chat_session.get_messages().await;
                self.set_status_message("Chat history cleared.".to_string(), Color::Green);
            }
            "/log" => {
                let log_path = self.chat_session.get_log_path().await;
                let message = match log_path {
                    Some(path) => format!("Log file is at: {}", path),
                    None => "Logging is not configured.".to_string(),
                };
                self.messages.push(ChatMessage {
                    role: ChatRole::System,
                    content: message,
                });
            }
            "/help" => {
                let help_text = "Available commands:

                - /help: Show this help message

                - /shell <command>: Execute a shell command via the AI

                - /model <model_name>: Switch AI model

                - /list models: List available models

                - /revert: Undo your last message and the AI's response

                - /clear: Clear the chat history

                - /log: Show the path to the current log file

                - /exit or /quit: Exit the application

                Shortcuts:

                - !: Enter shell mode (same as typing /shell )

                - Ctrl+C: Cancel the current AI response

                - Esc: Quit the application"
                    .to_string();
                self.messages.push(ChatMessage {
                    role: ChatRole::System,
                    content: help_text,
                });
            }
            _ => {
                self.set_status_message(format!("Unknown command: {}", command_name), Color::Red);
            }
        }
    }

    fn start_chat_stream(&mut self) {
        let sender = self.event_sender.clone();
        let mut chat_session = self.chat_session.clone();

        if let Some(handle) = self.chat_stream_handle.take() {
            handle.abort();
        }

        let handle = tokio::spawn(async move {
            match chat_session.start_realtime_chat().await {
                Ok(mut stream) => {
                    while let Some(event_result) = stream.next().await {
                        match event_result {
                            Ok(event) => {
                                if sender.send(TuiEvent::AgentEvent(event)).is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                let _ = sender.send(TuiEvent::Error(e.to_string()));
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = sender.send(TuiEvent::Error(e.to_string()));
                }
            }
            let _ = sender.send(TuiEvent::StreamComplete);
        });
        self.chat_stream_handle = Some(handle);
    }

    async fn handle_agent_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::AiResponseChunk(chunk) => {
                self.ai_response_buffer.push_str(&chunk);
                self.set_status_message("AI is typing...".to_string(), Color::LightBlue);
            }
            AgentEvent::ToolCallDetected(tool_call) => {
                self.tool_output_buffer.push_str(&format!(
                    "
--- Tool Call: {} ---
{}",
                    tool_call.tool_name,
                    serde_yaml::to_string(&tool_call.parameters).unwrap_or_default()
                ));
                self.set_status_message("Tool call detected...".to_string(), Color::Yellow);
            }
            AgentEvent::ToolExecuting(name) => {
                self.set_status_message(format!("Executing: {}...", name), Color::Cyan);
            }
            AgentEvent::ToolResult(tool_name, result) => {
                self.tool_output_buffer.push_str(&format!(
                    "
--- Tool Result ({}) ---
{}
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
",
                    tool_name, error_message
                ));
                self.set_status_message(format!("Tool {} failed.", tool_name), Color::Red);
            }
            AgentEvent::Thinking(msg) => self.set_status_message(msg, Color::LightBlue),
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
        self.ai_response_buffer.clear();
        self.tool_output_buffer.clear();
        self.chat_stream_handle = None;

        // Refresh messages from chat session to ensure consistency
        self.messages = self.chat_session.get_messages().await;

        self.set_status_message("Ready.".to_string(), Color::Green);
        self.is_ai_replying = false;
        self.is_user_scrolling = false;
    }

    fn cancel_ai_response(&mut self) {
        if let Some(handle) = self.chat_stream_handle.take() {
            handle.abort();
            self.is_ai_replying = false;
            self.ai_response_buffer.clear();
            self.tool_output_buffer.clear();
            self.messages.push(ChatMessage {
                role: ChatRole::System,
                content: "AI response cancelled by user.".to_string(),
            });
            self.set_status_message("AI response cancelled.".to_string(), Color::Yellow);
        }
    }

    fn set_status_message(&mut self, message: String, color: Color) {
        self.status_message = message;
        self.status_text_color = color;
        self.last_status_update = Instant::now();
    }
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    // 既存のraw modeを無効にしてから再度有効にする
    disable_raw_mode()?;
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
