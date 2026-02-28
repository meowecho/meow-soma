use std::env;
use std::io;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Local;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::runtime::{
    CancellationToken, ContextMessage, RuntimeAgent, RuntimeExecutionContext, RuntimeOperation,
};
use crate::state::StateStore;

const THEME_PRIMARY: Color = Color::Rgb(179, 123, 152); // #B37B98
const THEME_BG: Color = Color::Rgb(9, 18, 30);
const THEME_TEXT: Color = Color::Rgb(196, 203, 212);
const THEME_MUTED: Color = Color::Rgb(130, 138, 150);
const THEME_OK: Color = Color::Rgb(130, 198, 160);
const THEME_WARN: Color = Color::Rgb(220, 192, 120);
const THEME_ERROR: Color = Color::Rgb(224, 130, 138);
const MAX_DASHBOARD_HEIGHT_COMPACT: u16 = 22;
const MAX_DASHBOARD_HEIGHT_EXPANDED: u16 = 34;

pub fn run_tui(
    state: &StateStore,
    runtime: &RuntimeAgent,
    profile_name: &str,
    context_window: usize,
    cancellation: &CancellationToken,
) -> Result<()> {
    let session_id = state.create_session(Some("tui"))?;
    let provider = format!("{}:{}", runtime.provider_name(), runtime.provider_model());

    let mut ui = TuiState::new(session_id.clone(), profile_name.to_owned(), provider);

    let stdout = io::stdout();
    enable_raw_mode().context("failed to enable raw mode")?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to initialize terminal backend")?;
    terminal.clear().context("failed to clear terminal")?;

    let run_result = run_loop(
        &mut terminal,
        &mut ui,
        state,
        runtime,
        context_window,
        cancellation,
    );

    let restore_result = restore_terminal(&mut terminal);

    if let Err(err) = restore_result {
        return Err(err);
    }

    run_result?;
    println!("meow closed (session: {})", ui.session_id);
    Ok(())
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ui: &mut TuiState,
    state: &StateStore,
    runtime: &RuntimeAgent,
    context_window: usize,
    cancellation: &CancellationToken,
) -> Result<()> {
    loop {
        terminal
            .draw(|frame| ui.draw(frame))
            .context("failed to render tui frame")?;

        if cancellation.is_cancelled() {
            ui.status = "interrupted by signal".to_owned();
            ui.push_activity("signal", "interrupt received".to_owned());
            break;
        }

        if !event::poll(Duration::from_millis(80)).context("failed to poll events")? {
            continue;
        }

        match event::read().context("failed to read terminal event")? {
            Event::Key(key) => {
                if key.kind == KeyEventKind::Release {
                    continue;
                }

                if matches!(key.code, KeyCode::Esc)
                    || (matches!(key.code, KeyCode::Char('c'))
                        && key.modifiers.contains(KeyModifiers::CONTROL))
                {
                    ui.status = "exiting".to_owned();
                    ui.push_activity("session", "exit requested".to_owned());
                    break;
                }

                if matches!(key.code, KeyCode::Char('u'))
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    ui.clear_input();
                    ui.status = "input cleared".to_owned();
                    continue;
                }

                if matches!(key.code, KeyCode::Char('l'))
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    ui.clear_transcript();
                    ui.status = "conversation cleared".to_owned();
                    ui.push_activity("ui", "conversation cleared".to_owned());
                    continue;
                }

                match key.code {
                    KeyCode::Enter => {
                        let should_exit =
                            submit_prompt(ui, state, runtime, context_window, cancellation)?;
                        if should_exit {
                            break;
                        }
                    }
                    KeyCode::Backspace => {
                        ui.input.pop();
                        ui.history_cursor = None;
                    }
                    KeyCode::Up => ui.history_prev(),
                    KeyCode::Down => ui.history_next(),
                    KeyCode::PageUp => ui.scroll_transcript_up(8),
                    KeyCode::PageDown => ui.scroll_transcript_down(8),
                    KeyCode::Home => ui.scroll_transcript_top(),
                    KeyCode::End => ui.scroll_transcript_bottom(),
                    KeyCode::Char(ch) => {
                        if key.modifiers.contains(KeyModifiers::CONTROL) {
                            continue;
                        }
                        ui.input.push(ch);
                        ui.history_cursor = None;
                    }
                    KeyCode::Tab => {
                        ui.input.push('\t');
                        ui.history_cursor = None;
                    }
                    _ => {}
                }
            }
            Event::Resize(_, _) => {
                ui.status = "resized".to_owned();
            }
            _ => {}
        }
    }

    Ok(())
}

fn submit_prompt(
    ui: &mut TuiState,
    state: &StateStore,
    runtime: &RuntimeAgent,
    context_window: usize,
    cancellation: &CancellationToken,
) -> Result<bool> {
    let prompt = ui.input.trim().to_owned();
    ui.clear_input();

    if prompt.is_empty() {
        return Ok(false);
    }

    ui.remember_history(&prompt);

    if prompt.starts_with('/') {
        return handle_slash_command(ui, state, &prompt);
    }

    let context_messages = load_bounded_context(state, &ui.session_id, context_window)?;
    let runtime_context = RuntimeExecutionContext::new(
        RuntimeOperation::Chat,
        ui.profile.as_str(),
        Some(ui.session_id.clone()),
        context_messages,
    );

    ui.push_user(prompt.clone());
    ui.push_activity(
        "request",
        format!("prompt submitted ({} chars)", prompt.chars().count()),
    );
    state.add_message(&ui.session_id, "user", &prompt)?;

    ui.status = "thinking...".to_owned();
    cancellation.clear();

    match runtime.respond_with_context(&runtime_context, &prompt, cancellation) {
        Ok(response) => {
            state.add_message(&ui.session_id, "assistant", &response.text)?;
            ui.push_assistant(response.text);
            ui.push_activity(
                "response",
                format!("completed in {} ms", response.duration_ms),
            );
            ui.status = format!("ok | {} ms", response.duration_ms);
        }
        Err(err) => {
            let message = err.to_string();
            ui.push_error(message.clone());
            ui.push_activity("error", message.clone());
            if cancellation.is_cancelled() {
                ui.status = "interrupted by user".to_owned();
            } else {
                ui.status = "error".to_owned();
            }
            state.add_message(&ui.session_id, "assistant", &format!("[error] {message}"))?;
        }
    }

    Ok(false)
}

fn handle_slash_command(ui: &mut TuiState, state: &StateStore, prompt: &str) -> Result<bool> {
    let (cmd, args) = parse_slash_command(prompt);
    let cmd = cmd.to_ascii_lowercase();

    match cmd.as_str() {
        "quit" | "exit" => {
            ui.status = "exiting".to_owned();
            ui.push_activity("command", "/quit".to_owned());
            Ok(true)
        }
        "help" => {
            ui.status =
                "commands: /help /home /clear /session /provider /profile <name> /new [title] /quit"
                    .to_owned();
            ui.push_activity("command", "/help".to_owned());
            Ok(false)
        }
        "home" => {
            ui.status = "home dashboard".to_owned();
            ui.push_activity("command", "/home".to_owned());
            Ok(false)
        }
        "clear" => {
            ui.clear_transcript();
            ui.status = "conversation cleared".to_owned();
            ui.push_activity("command", "/clear".to_owned());
            Ok(false)
        }
        "session" => {
            ui.status = format!("session {}", ui.session_id);
            ui.push_activity("command", "/session".to_owned());
            Ok(false)
        }
        "provider" | "model" => {
            ui.status = format!("provider {}", ui.provider);
            ui.push_activity("command", format!("/{cmd}"));
            Ok(false)
        }
        "profile" => {
            if args.is_empty() {
                ui.status = "usage: /profile <name>".to_owned();
                ui.push_activity("command", "invalid /profile usage".to_owned());
                return Ok(false);
            }
            ui.profile = args.to_owned();
            ui.status = format!("profile switched to {}", ui.profile);
            ui.push_activity("command", format!("/profile {}", ui.profile));
            Ok(false)
        }
        "new" => {
            let title = if args.is_empty() {
                Some("tui")
            } else {
                Some(args)
            };
            let new_session_id = state.create_session(title)?;
            ui.session_id = new_session_id;
            ui.clear_transcript();
            ui.status = "new session started".to_owned();
            ui.push_activity("session", "started new session".to_owned());
            Ok(false)
        }
        _ => {
            ui.status = format!("unknown command: /{cmd}");
            ui.push_activity("error", format!("unknown command /{cmd}"));
            Ok(false)
        }
    }
}

fn parse_slash_command(raw: &str) -> (&str, &str) {
    let trimmed = raw.trim();
    let body = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let mut parts = body.splitn(2, char::is_whitespace);
    let cmd = parts.next().unwrap_or("").trim();
    let args = parts.next().map(str::trim).unwrap_or("");
    (cmd, args)
}

fn load_bounded_context(
    state: &StateStore,
    session_id: &str,
    context_window: usize,
) -> Result<Vec<ContextMessage>> {
    let messages = state.get_recent_messages(session_id, context_window)?;
    Ok(messages
        .into_iter()
        .map(|item| ContextMessage {
            role: item.role,
            content: item.content,
        })
        .collect())
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode().context("failed to disable raw mode")?;
    terminal.show_cursor().context("failed to show cursor")?;
    Ok(())
}

#[derive(Debug, Clone)]
struct TranscriptEntry {
    role: String,
    content: String,
}

#[derive(Debug, Clone)]
struct ActivityEntry {
    at: String,
    kind: String,
    content: String,
}

struct TuiState {
    session_id: String,
    profile: String,
    provider: String,
    user: String,
    workspace: String,
    input: String,
    status: String,
    transcript: Vec<TranscriptEntry>,
    transcript_scroll: usize,
    activity: Vec<ActivityEntry>,
    history: Vec<String>,
    history_cursor: Option<usize>,
}

impl TuiState {
    fn new(session_id: String, profile: String, provider: String) -> Self {
        Self {
            session_id,
            profile,
            provider,
            user: detect_user_name(),
            workspace: detect_workspace_path(),
            input: String::new(),
            status: "ready".to_owned(),
            transcript: Vec::new(),
            transcript_scroll: 0,
            activity: Vec::new(),
            history: Vec::new(),
            history_cursor: None,
        }
    }

    fn push_user(&mut self, message: String) {
        self.push("you", message);
    }

    fn push_assistant(&mut self, message: String) {
        self.push("meow", message);
    }

    fn push_error(&mut self, message: String) {
        self.push("error", message);
    }

    fn push(&mut self, role: &str, content: String) {
        self.transcript.push(TranscriptEntry {
            role: role.to_owned(),
            content,
        });
        const MAX_ENTRIES: usize = 400;
        if self.transcript.len() > MAX_ENTRIES {
            let extra = self.transcript.len() - MAX_ENTRIES;
            self.transcript.drain(0..extra);
        }
        self.scroll_transcript_bottom();
    }

    fn push_activity(&mut self, kind: &str, content: String) {
        self.activity.push(ActivityEntry {
            at: Local::now().format("%H:%M:%S").to_string(),
            kind: kind.to_owned(),
            content,
        });
        const MAX_ENTRIES: usize = 300;
        if self.activity.len() > MAX_ENTRIES {
            let extra = self.activity.len() - MAX_ENTRIES;
            self.activity.drain(0..extra);
        }
    }

    fn clear_input(&mut self) {
        self.input.clear();
        self.history_cursor = None;
    }

    fn clear_transcript(&mut self) {
        self.transcript.clear();
        self.transcript_scroll = 0;
    }

    fn remember_history(&mut self, value: &str) {
        if value.is_empty() {
            return;
        }

        if self.history.last().is_some_and(|last| last == value) {
            self.history_cursor = None;
            return;
        }

        self.history.push(value.to_owned());
        const MAX_HISTORY: usize = 200;
        if self.history.len() > MAX_HISTORY {
            let extra = self.history.len() - MAX_HISTORY;
            self.history.drain(0..extra);
        }
        self.history_cursor = None;
    }

    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }

        let next = match self.history_cursor {
            None => self.history.len() - 1,
            Some(0) => 0,
            Some(idx) => idx.saturating_sub(1),
        };

        self.history_cursor = Some(next);
        self.input = self.history[next].clone();
    }

    fn history_next(&mut self) {
        if self.history.is_empty() {
            return;
        }

        match self.history_cursor {
            None => {}
            Some(idx) if idx + 1 >= self.history.len() => {
                self.history_cursor = None;
                self.input.clear();
            }
            Some(idx) => {
                let next = idx + 1;
                self.history_cursor = Some(next);
                self.input = self.history[next].clone();
            }
        }
    }

    fn scroll_transcript_up(&mut self, amount: usize) {
        self.transcript_scroll = self.transcript_scroll.saturating_sub(amount);
    }

    fn scroll_transcript_down(&mut self, amount: usize) {
        let max = self.transcript_max_scroll();
        self.transcript_scroll = self.transcript_scroll.saturating_add(amount).min(max);
    }

    fn scroll_transcript_top(&mut self) {
        self.transcript_scroll = 0;
    }

    fn scroll_transcript_bottom(&mut self) {
        self.transcript_scroll = self.transcript_max_scroll();
    }

    fn transcript_max_scroll(&self) -> usize {
        self.transcript.len().saturating_sub(1)
    }

    fn in_home_mode(&self) -> bool {
        self.transcript.is_empty()
    }

    fn draw(&self, frame: &mut ratatui::Frame) {
        let viewport = frame.area();
        let max_height = viewport.height.min(if self.transcript.is_empty() {
            MAX_DASHBOARD_HEIGHT_COMPACT
        } else {
            MAX_DASHBOARD_HEIGHT_EXPANDED
        });
        let hero_height = home_hero_height(max_height);
        let max_feed_rows = max_height.saturating_sub(hero_height + 4).max(1);
        let feed_rows = self.desired_feed_rows(max_feed_rows);
        let root_height = hero_height + feed_rows + 4;

        let root = Rect {
            x: viewport.x,
            y: viewport.y,
            width: viewport.width,
            height: root_height.min(max_height),
        };

        let home = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(hero_height),
                Constraint::Length(feed_rows),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(root);

        self.draw_home(frame, home[0]);
        self.draw_conversation_feed(frame, home[1]);
        self.draw_input(frame, home[2]);
        self.draw_footer(frame, home[3]);
    }

    fn desired_feed_rows(&self, max_rows: u16) -> u16 {
        if self.transcript.is_empty() {
            return 1.min(max_rows.max(1));
        }

        let desired = self.transcript.len().max(2).min(max_rows as usize);
        desired as u16
    }

    fn draw_home(&self, frame: &mut ratatui::Frame, area: Rect) {
        let shell = panel_block(format!(" Meow Soma v{} ", env!("CARGO_PKG_VERSION")));
        frame.render_widget(shell.clone(), area);
        let inner = shell.inner(area);

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(61), Constraint::Percentage(39)])
            .split(inner);

        let left_parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(6),
                Constraint::Length(2),
                Constraint::Min(1),
            ])
            .split(columns[0]);

        let welcome = Paragraph::new(vec![Line::from(Span::styled(
            format!("Welcome back {}!", self.user),
            Style::default().fg(THEME_TEXT).add_modifier(Modifier::BOLD),
        ))])
        .alignment(Alignment::Center)
        .style(Style::default().fg(THEME_TEXT).bg(THEME_BG))
        .wrap(Wrap { trim: false });
        frame.render_widget(welcome, left_parts[0]);

        let mascot = Paragraph::new(vec![
            Line::from(Span::styled(
                "       /\\_/\\",
                Style::default().fg(THEME_PRIMARY),
            )),
            Line::from(Span::styled(
                "      / o o \\",
                Style::default().fg(THEME_PRIMARY),
            )),
            Line::from(Span::styled(
                "     (   \"   )",
                Style::default().fg(THEME_PRIMARY),
            )),
            Line::from(Span::styled(
                "      \\~(*)~/",
                Style::default().fg(THEME_PRIMARY),
            )),
            Line::from(Span::styled(
                "       // \\\\",
                Style::default().fg(THEME_PRIMARY),
            )),
        ])
        .alignment(Alignment::Center)
        .style(Style::default().fg(THEME_TEXT).bg(THEME_BG))
        .wrap(Wrap { trim: false });
        frame.render_widget(mascot, left_parts[1]);

        let meta = Paragraph::new(vec![
            Line::from(Span::styled(
                format!("{} · {}", self.provider, self.profile),
                Style::default().fg(THEME_MUTED),
            )),
            Line::from(Span::styled(
                self.workspace.clone(),
                Style::default().fg(THEME_MUTED),
            )),
        ])
        .alignment(Alignment::Center)
        .style(Style::default().fg(THEME_TEXT).bg(THEME_BG))
        .wrap(Wrap { trim: false });
        frame.render_widget(meta, left_parts[2]);

        let mut right_lines = Vec::new();
        right_lines.push(Line::from(Span::styled(
            "Tips for getting started",
            Style::default()
                .fg(THEME_PRIMARY)
                .add_modifier(Modifier::BOLD),
        )));
        right_lines.push(Line::from(Span::styled(
            "Run /help to view shortcuts",
            Style::default().fg(THEME_MUTED),
        )));
        right_lines.push(Line::from(Span::styled(
            "Press Enter to start chatting",
            Style::default().fg(THEME_MUTED),
        )));
        right_lines.push(Line::from(Span::styled(
            "Use /new to reset the session",
            Style::default().fg(THEME_MUTED),
        )));
        right_lines.push(Line::default());
        right_lines.push(Line::from(Span::styled(
            "----------------------------",
            Style::default().fg(THEME_PRIMARY),
        )));
        right_lines.push(Line::from(Span::styled(
            "Recent activity",
            Style::default()
                .fg(THEME_PRIMARY)
                .add_modifier(Modifier::BOLD),
        )));
        right_lines.push(Line::default());

        let recent = self.activity.iter().rev().take(4).collect::<Vec<_>>();
        if recent.is_empty() {
            right_lines.push(Line::from(Span::styled(
                "No recent activity",
                Style::default().fg(THEME_MUTED),
            )));
        } else {
            for item in recent.iter().rev() {
                right_lines.push(render_activity(item));
            }
        }

        let right = Paragraph::new(right_lines)
            .block(
                Block::default()
                    .borders(Borders::LEFT)
                    .border_style(Style::default().fg(THEME_PRIMARY))
                    .style(Style::default().bg(THEME_BG)),
            )
            .style(Style::default().fg(THEME_TEXT).bg(THEME_BG))
            .wrap(Wrap { trim: false });
        frame.render_widget(right, columns[1]);
    }

    fn draw_conversation_feed(&self, frame: &mut ratatui::Frame, area: Rect) {
        if area.height == 0 {
            return;
        }

        let lines = self
            .transcript
            .iter()
            .map(render_entry)
            .collect::<Vec<Line>>();
        let visible_rows = area.height.max(1) as usize;
        let max_scroll = lines.len().saturating_sub(visible_rows);
        let scroll = self
            .transcript_scroll
            .min(max_scroll)
            .min(u16::MAX as usize) as u16;

        let conversation = Paragraph::new(lines)
            .scroll((scroll, 0))
            .style(Style::default().fg(THEME_TEXT).bg(THEME_BG))
            .wrap(Wrap { trim: false });
        frame.render_widget(conversation, area);
    }

    fn draw_input(&self, frame: &mut ratatui::Frame, area: Rect) {
        let prompt = Span::styled(
            "> ",
            Style::default()
                .fg(THEME_PRIMARY)
                .add_modifier(Modifier::BOLD),
        );

        let line = if self.input.is_empty() {
            let placeholder = if self.in_home_mode() {
                "Try \"explain this repository architecture\""
            } else {
                "Type a message or run /help"
            };
            Line::from(vec![
                prompt,
                Span::styled(placeholder, Style::default().fg(THEME_MUTED)),
            ])
        } else {
            Line::from(vec![
                prompt,
                Span::styled(self.input.clone(), Style::default().fg(THEME_TEXT)),
            ])
        };

        let input = Paragraph::new(vec![line])
            .block(input_block())
            .style(Style::default().bg(THEME_BG))
            .wrap(Wrap { trim: false });
        frame.render_widget(input, area);
    }

    fn draw_footer(&self, frame: &mut ratatui::Frame, area: Rect) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(area);

        let left = Paragraph::new(Line::from(vec![
            Span::styled("?", Style::default().fg(THEME_PRIMARY)),
            Span::styled(" for shortcuts (/help)", Style::default().fg(THEME_MUTED)),
        ]))
        .style(Style::default().bg(THEME_BG));
        frame.render_widget(left, columns[0]);

        let right_text = if self.status.contains("error") || self.status.contains("thinking") {
            self.status.clone()
        } else {
            "Update available! Run: meow self-update (soon)".to_owned()
        };

        let right = Paragraph::new(Line::from(vec![Span::styled(
            right_text.clone(),
            status_style(&right_text),
        )]))
        .alignment(Alignment::Right)
        .style(Style::default().bg(THEME_BG));
        frame.render_widget(right, columns[1]);
    }
}

fn panel_block(title: impl Into<String>) -> Block<'static> {
    Block::default()
        .title(Span::styled(
            title.into(),
            Style::default()
                .fg(THEME_PRIMARY)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(THEME_PRIMARY))
        .style(Style::default().bg(THEME_BG))
}

fn input_block() -> Block<'static> {
    Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_style(Style::default().fg(THEME_PRIMARY))
        .style(Style::default().bg(THEME_BG))
}

fn status_style(status: &str) -> Style {
    let lowered = status.to_ascii_lowercase();
    if lowered.contains("error") || lowered.contains("unknown") {
        Style::default()
            .fg(THEME_ERROR)
            .add_modifier(Modifier::BOLD)
    } else if lowered.contains("thinking")
        || lowered.contains("interrupt")
        || lowered.contains("update")
    {
        Style::default().fg(THEME_WARN).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(THEME_OK).add_modifier(Modifier::BOLD)
    }
}

fn render_entry(entry: &TranscriptEntry) -> Line<'static> {
    let role_color = match entry.role.as_str() {
        "you" => THEME_PRIMARY,
        "meow" => THEME_OK,
        "error" => THEME_ERROR,
        _ => THEME_MUTED,
    };

    Line::from(vec![
        Span::styled(
            format!("{}: ", entry.role),
            Style::default().fg(role_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(entry.content.clone(), Style::default().fg(THEME_TEXT)),
    ])
}

fn render_activity(entry: &ActivityEntry) -> Line<'static> {
    let kind_color = match entry.kind.as_str() {
        "error" => THEME_ERROR,
        "response" => THEME_OK,
        "request" => THEME_PRIMARY,
        _ => THEME_MUTED,
    };

    Line::from(vec![
        Span::styled(
            format!("[{}] {} ", entry.at, entry.kind),
            Style::default().fg(kind_color),
        ),
        Span::styled(entry.content.clone(), Style::default().fg(THEME_TEXT)),
    ])
}

fn detect_user_name() -> String {
    env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "friend".to_owned())
}

fn detect_workspace_path() -> String {
    let cwd = env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| ".".to_owned());

    let with_tilde = env::var("HOME")
        .ok()
        .and_then(|home| cwd.strip_prefix(&home).map(|rest| format!("~{}", rest)))
        .unwrap_or(cwd);

    truncate_middle(&with_tilde, 68)
}

fn truncate_middle(input: &str, max_chars: usize) -> String {
    let chars = input.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return input.to_owned();
    }

    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }

    let left_len = (max_chars - 3) / 2;
    let right_len = max_chars - 3 - left_len;

    let left = chars.iter().take(left_len).collect::<String>();
    let right = chars
        .iter()
        .skip(chars.len().saturating_sub(right_len))
        .collect::<String>();

    format!("{left}...{right}")
}

fn home_hero_height(total_height: u16) -> u16 {
    if total_height >= 42 {
        18
    } else if total_height >= 34 {
        16
    } else if total_height >= 26 {
        14
    } else if total_height >= 20 {
        12
    } else {
        total_height.saturating_sub(4).max(8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_entry_prefixes_role() {
        let entry = TranscriptEntry {
            role: "you".to_owned(),
            content: "hello".to_owned(),
        };

        let rendered = render_entry(&entry);
        let plain = rendered
            .spans
            .iter()
            .map(|span| span.content.clone())
            .collect::<Vec<_>>()
            .join("");

        assert_eq!(plain, "you: hello");
    }

    #[test]
    fn transcript_is_bounded() {
        let mut state = TuiState::new("s".to_owned(), "default".to_owned(), "p".to_owned());
        for idx in 0..450 {
            state.push("you", format!("{idx}"));
        }

        assert_eq!(state.transcript.len(), 400);
        assert_eq!(
            state.transcript.first().map(|x| x.content.as_str()),
            Some("50")
        );
    }

    #[test]
    fn parse_slash_command_splits_args() {
        let (cmd, args) = parse_slash_command("/profile coding");
        assert_eq!(cmd, "profile");
        assert_eq!(args, "coding");

        let (cmd, args) = parse_slash_command("/help");
        assert_eq!(cmd, "help");
        assert_eq!(args, "");
    }

    #[test]
    fn history_navigation_roundtrip() {
        let mut state = TuiState::new("s".to_owned(), "default".to_owned(), "p".to_owned());
        state.remember_history("first");
        state.remember_history("second");

        state.history_prev();
        assert_eq!(state.input, "second");

        state.history_prev();
        assert_eq!(state.input, "first");

        state.history_next();
        assert_eq!(state.input, "second");

        state.history_next();
        assert_eq!(state.input, "");
    }

    #[test]
    fn truncate_middle_preserves_ends() {
        let value = "abcdefghijklmnopqrstuvwxyz";
        let output = truncate_middle(value, 10);
        assert_eq!(output, "abc...wxyz");
    }

    #[test]
    fn home_hero_height_uses_size_bands() {
        assert_eq!(home_hero_height(44), 18);
        assert_eq!(home_hero_height(34), 16);
        assert_eq!(home_hero_height(26), 14);
        assert_eq!(home_hero_height(20), 12);
        assert_eq!(home_hero_height(12), 8);
    }
}
