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
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::policy::PolicyEngine;
use crate::runtime::{
    CancellationToken, ContextMessage, RuntimeAgent, RuntimeExecutionContext, RuntimeOperation,
};
use crate::state::StateStore;
use crate::tools::{ToolOutput, ToolRegistry};

const THEME_PRIMARY: Color = Color::Rgb(179, 123, 152); // #B37B98
const THEME_BG: Color = Color::Rgb(9, 18, 30);
const THEME_TEXT: Color = Color::Rgb(196, 203, 212);
const THEME_MUTED: Color = Color::Rgb(130, 138, 150);
const THEME_OK: Color = Color::Rgb(130, 198, 160);
const THEME_WARN: Color = Color::Rgb(220, 192, 120);
const THEME_ERROR: Color = Color::Rgb(224, 130, 138);
const MAX_DASHBOARD_HEIGHT_COMPACT: u16 = 22;
const PALETTE_ITEMS: &[(&str, &str, &str)] = &[
    ("Help", "/help", "Show slash commands and shortcuts"),
    ("New Session", "/new", "Start a fresh conversation session"),
    ("Session Info", "/session", "Show current session id"),
    ("Provider Info", "/provider", "Show active provider/model"),
    ("Clear Chat", "/clear", "Clear chat feed"),
    ("List Tools", "/tool", "Show available tools"),
    ("Exit", "/quit", "Exit Meow Soma"),
];

pub fn run_tui(
    state: &StateStore,
    policy: &PolicyEngine,
    tools: &ToolRegistry,
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
        policy,
        tools,
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
    policy: &PolicyEngine,
    tools: &ToolRegistry,
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

                if ui.palette_open {
                    let should_exit =
                        handle_palette_key(key.code, key.modifiers, ui, state, policy, tools)?;
                    if should_exit {
                        break;
                    }
                    continue;
                }

                if matches!(key.code, KeyCode::Char('p'))
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    ui.open_palette();
                    ui.status = "command palette".to_owned();
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

                if matches!(key.code, KeyCode::Char('r'))
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    ui.history_search_next();
                    continue;
                }

                match key.code {
                    KeyCode::Enter => {
                        let should_exit = submit_prompt(
                            terminal,
                            ui,
                            state,
                            policy,
                            tools,
                            runtime,
                            context_window,
                            cancellation,
                        )?;
                        if should_exit {
                            break;
                        }
                    }
                    KeyCode::Backspace => {
                        ui.input.pop();
                        ui.history_cursor = None;
                        ui.clear_history_search();
                    }
                    KeyCode::Up => {
                        ui.history_prev();
                        ui.clear_history_search();
                    }
                    KeyCode::Down => {
                        ui.history_next();
                        ui.clear_history_search();
                    }
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
                        ui.clear_history_search();
                    }
                    KeyCode::Tab => {
                        ui.input.push('\t');
                        ui.history_cursor = None;
                        ui.clear_history_search();
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
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ui: &mut TuiState,
    state: &StateStore,
    policy: &PolicyEngine,
    tools: &ToolRegistry,
    runtime: &RuntimeAgent,
    context_window: usize,
    cancellation: &CancellationToken,
) -> Result<bool> {
    let prompt = ui.input.trim().to_owned();
    ui.clear_input();

    if prompt.is_empty() {
        return Ok(false);
    }

    if prompt.starts_with('/') {
        return handle_slash_command(ui, state, policy, tools, &prompt);
    }

    if ui.pending_approval.is_some() {
        return resolve_pending_approval(ui, state, tools, &prompt);
    }

    ui.remember_history(&prompt);

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
    terminal
        .draw(|frame| ui.draw(frame))
        .context("failed to render thinking state")?;
    cancellation.clear();

    let stream_index = ui.push_assistant_streaming();
    let mut render_stream_chunk = |chunk: &str| -> Result<()> {
        ui.append_to_transcript(stream_index, chunk);
        terminal
            .draw(|frame| ui.draw(frame))
            .context("failed to render streaming chunk")?;
        Ok(())
    };

    match runtime.respond_with_context_streaming(
        &runtime_context,
        &prompt,
        cancellation,
        &mut render_stream_chunk,
    ) {
        Ok(response) => {
            state.add_message(&ui.session_id, "assistant", &response.text)?;
            ui.replace_transcript_content(stream_index, response.text);
            ui.push_activity(
                "response",
                format!("completed in {} ms", response.duration_ms),
            );
            ui.status = format!("ok | {} ms", response.duration_ms);
        }
        Err(err) => {
            let message = err.to_string();
            let has_partial = ui.transcript_entry_has_content(stream_index);
            if has_partial {
                ui.append_to_transcript(stream_index, &format!("\n[error] {message}"));
            } else {
                ui.remove_transcript_entry(stream_index);
                ui.push_error(message.clone());
            }
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

fn handle_slash_command(
    ui: &mut TuiState,
    state: &StateStore,
    policy: &PolicyEngine,
    tools: &ToolRegistry,
    prompt: &str,
) -> Result<bool> {
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
                "commands: /help /home /clear /session /provider /profile <name> /new [title] /tool [name ...] /quit".to_owned();
            ui.push_activity("command", "/help".to_owned());
            Ok(false)
        }
        "home" => {
            ui.status = "home dashboard".to_owned();
            ui.push_activity("command", "/home".to_owned());
            Ok(false)
        }
        "palette" => {
            ui.open_palette();
            ui.status = "command palette".to_owned();
            ui.push_activity("command", "/palette".to_owned());
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
        "tool" => run_tool_from_slash(ui, state, policy, tools, args),
        _ => {
            ui.status = format!("unknown command: /{cmd}");
            ui.push_activity("error", format!("unknown command /{cmd}"));
            Ok(false)
        }
    }
}

fn handle_palette_key(
    code: KeyCode,
    modifiers: KeyModifiers,
    ui: &mut TuiState,
    state: &StateStore,
    policy: &PolicyEngine,
    tools: &ToolRegistry,
) -> Result<bool> {
    if matches!(code, KeyCode::Char('c')) && modifiers.contains(KeyModifiers::CONTROL) {
        ui.status = "exiting".to_owned();
        ui.push_activity("session", "exit requested".to_owned());
        return Ok(true);
    }

    if matches!(code, KeyCode::Esc)
        || (matches!(code, KeyCode::Char('p')) && modifiers.contains(KeyModifiers::CONTROL))
    {
        ui.close_palette();
        ui.status = "palette closed".to_owned();
        return Ok(false);
    }

    match code {
        KeyCode::Enter => {
            let Some(command) = ui.selected_palette_command() else {
                ui.status = "palette has no matching command".to_owned();
                return Ok(false);
            };
            ui.close_palette();
            handle_slash_command(ui, state, policy, tools, command)
        }
        KeyCode::Up => {
            ui.palette_prev();
            Ok(false)
        }
        KeyCode::Down => {
            ui.palette_next();
            Ok(false)
        }
        KeyCode::Backspace => {
            ui.palette_backspace();
            Ok(false)
        }
        KeyCode::Char(ch) => {
            if !modifiers.contains(KeyModifiers::CONTROL) {
                ui.palette_push(ch);
            }
            Ok(false)
        }
        _ => Ok(false),
    }
}

fn resolve_pending_approval(
    ui: &mut TuiState,
    state: &StateStore,
    tools: &ToolRegistry,
    prompt: &str,
) -> Result<bool> {
    let decision = prompt.trim().to_ascii_lowercase();
    let Some(pending) = ui.pending_approval.take() else {
        return Ok(false);
    };

    let approved = matches!(decision.as_str(), "y" | "yes" | "approve");
    let rejected = matches!(decision.as_str(), "n" | "no" | "reject");

    if !approved && !rejected {
        ui.pending_approval = Some(pending.clone());
        ui.status = "approval pending: type y/yes or n/no".to_owned();
        ui.push_activity("approval", "invalid decision input".to_owned());
        return Ok(false);
    }

    if !approved {
        state.record_approval(&pending.name, "rejected", &pending.reason, false)?;
        ui.push(
            "status",
            format!(
                "tool {} rejected (reason: {})",
                pending.name, pending.reason
            ),
        );
        ui.status = "tool rejected".to_owned();
        ui.push_activity("approval", format!("rejected {}", pending.name));
        return Ok(false);
    }

    state.record_approval(&pending.name, "approved", &pending.reason, true)?;
    execute_tool_with_audit(ui, state, tools, &pending.name, &pending.args)
}

fn run_tool_from_slash(
    ui: &mut TuiState,
    state: &StateStore,
    policy: &PolicyEngine,
    tools: &ToolRegistry,
    args: &str,
) -> Result<bool> {
    if args.trim().is_empty() {
        let specs = tools.list();
        let summary = specs
            .iter()
            .map(|spec| format!("{}{}", spec.name, if spec.risky { " [risky]" } else { "" }))
            .collect::<Vec<_>>()
            .join(", ");
        ui.push("tool", format!("available tools: {summary}"));
        ui.status = "tool list loaded".to_owned();
        ui.push_activity("tool", "listed tools".to_owned());
        return Ok(false);
    }

    let parsed = parse_tool_args(args);
    if parsed.is_empty() {
        ui.status = "usage: /tool <name> [args ...]".to_owned();
        return Ok(false);
    }

    let tool_name = parsed[0].clone();
    let tool_args = parsed[1..].to_vec();

    if !tools.is_known(&tool_name) {
        ui.status = format!("unknown tool: {tool_name}");
        ui.push_error(format!("unknown tool: {tool_name}"));
        ui.push_activity("error", format!("unknown tool /tool {tool_name}"));
        return Ok(false);
    }

    let decision = if tool_name == "shell" {
        policy.evaluate_shell(&tool_args.join(" "))
    } else {
        policy.evaluate_tool(&tool_name, ToolRegistry::is_risky(&tool_name))
    };

    if !decision.allowed {
        state.record_approval(&tool_name, "denied", &decision.reason, false)?;
        ui.status = format!("tool denied: {}", decision.reason);
        ui.push_error(format!("tool denied: {}", decision.reason));
        ui.push_activity("approval", format!("denied {}", tool_name));
        return Ok(false);
    }

    if decision.requires_approval {
        state.record_approval(&tool_name, "required", &decision.reason, false)?;
        ui.pending_approval = Some(PendingApproval {
            name: tool_name.clone(),
            args: tool_args.clone(),
            reason: decision.reason.clone(),
        });
        ui.push(
            "status",
            format!(
                "approval required for {} {} ({})",
                tool_name,
                tool_args.join(" "),
                decision.reason
            ),
        );
        ui.status = "approval required: type y/yes or n/no".to_owned();
        ui.push_activity("approval", format!("required {}", tool_name));
        return Ok(false);
    }

    execute_tool_with_audit(ui, state, tools, &tool_name, &tool_args)
}

fn execute_tool_with_audit(
    ui: &mut TuiState,
    state: &StateStore,
    tools: &ToolRegistry,
    tool_name: &str,
    tool_args: &[String],
) -> Result<bool> {
    let joined_args = tool_args.join(" ");
    match tools.execute(tool_name, tool_args) {
        Ok(output) => {
            record_tool_and_render(ui, state, tool_name, &joined_args, output)?;
            Ok(false)
        }
        Err(err) => {
            state.record_tool_call(tool_name, &joined_args, "error", &err.to_string())?;
            ui.push_error(format!("tool {tool_name} failed: {err}"));
            ui.status = "tool error".to_owned();
            ui.push_activity("tool", format!("error {}", tool_name));
            Ok(false)
        }
    }
}

fn record_tool_and_render(
    ui: &mut TuiState,
    state: &StateStore,
    tool_name: &str,
    joined_args: &str,
    output: ToolOutput,
) -> Result<()> {
    state.record_tool_call(tool_name, joined_args, &output.status, &output.stdout)?;
    if !output.stdout.trim().is_empty() {
        ui.push("tool", output.stdout.trim_end().to_owned());
    }
    if !output.stderr.trim().is_empty() {
        ui.push("tool_err", output.stderr.trim_end().to_owned());
    }
    if output.stdout.trim().is_empty() && output.stderr.trim().is_empty() {
        ui.push("tool", format!("{tool_name} completed with no output"));
    }
    ui.status = format!("tool {tool_name}: {}", output.status);
    ui.push_activity("tool", format!("{} {}", tool_name, output.status));
    Ok(())
}

fn parse_tool_args(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(ToOwned::to_owned).collect()
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

#[derive(Debug, Clone)]
struct PendingApproval {
    name: String,
    args: Vec<String>,
    reason: String,
}

#[derive(Debug, Clone)]
struct HistorySearchState {
    query: String,
    matches: Vec<usize>,
    cursor: usize,
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
    history_search: Option<HistorySearchState>,
    pending_approval: Option<PendingApproval>,
    palette_open: bool,
    palette_filter: String,
    palette_cursor: usize,
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
            history_search: None,
            pending_approval: None,
            palette_open: false,
            palette_filter: String::new(),
            palette_cursor: 0,
        }
    }

    fn push_user(&mut self, message: String) {
        self.push("you", message);
    }

    fn push_assistant_streaming(&mut self) -> usize {
        self.transcript.push(TranscriptEntry {
            role: "meow".to_owned(),
            content: String::new(),
        });
        self.enforce_transcript_bound();
        let index = self.transcript.len().saturating_sub(1);
        self.scroll_transcript_bottom();
        index
    }

    fn push_error(&mut self, message: String) {
        self.push("error", message);
    }

    fn push(&mut self, role: &str, content: String) {
        self.transcript.push(TranscriptEntry {
            role: role.to_owned(),
            content,
        });
        self.enforce_transcript_bound();
        self.scroll_transcript_bottom();
    }

    fn append_to_transcript(&mut self, index: usize, chunk: &str) {
        if let Some(entry) = self.transcript.get_mut(index) {
            entry.content.push_str(chunk);
            self.scroll_transcript_bottom();
        }
    }

    fn replace_transcript_content(&mut self, index: usize, content: String) {
        if let Some(entry) = self.transcript.get_mut(index) {
            entry.content = content;
            self.scroll_transcript_bottom();
        }
    }

    fn transcript_entry_has_content(&self, index: usize) -> bool {
        self.transcript
            .get(index)
            .is_some_and(|entry| !entry.content.trim().is_empty())
    }

    fn remove_transcript_entry(&mut self, index: usize) {
        if index < self.transcript.len() {
            self.transcript.remove(index);
            self.transcript_scroll = self.transcript_scroll.min(self.transcript_max_scroll());
        }
    }

    fn enforce_transcript_bound(&mut self) {
        const MAX_ENTRIES: usize = 400;
        if self.transcript.len() > MAX_ENTRIES {
            let extra = self.transcript.len() - MAX_ENTRIES;
            self.transcript.drain(0..extra);
        }
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
        self.clear_history_search();
    }

    fn clear_transcript(&mut self) {
        self.transcript.clear();
        self.transcript_scroll = 0;
        self.pending_approval = None;
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
        self.clear_history_search();
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

    fn history_search_next(&mut self) {
        if self.history.is_empty() {
            self.status = "history search: no history".to_owned();
            return;
        }

        let query = if let Some(state) = &self.history_search {
            let selected_input = state
                .matches
                .get(state.cursor)
                .and_then(|idx| self.history.get(*idx));
            if selected_input.is_some_and(|entry| entry == &self.input) {
                state.query.clone()
            } else {
                self.input.to_ascii_lowercase()
            }
        } else {
            self.input.to_ascii_lowercase()
        };
        let query_changed = self
            .history_search
            .as_ref()
            .is_none_or(|state| state.query != query);

        if query_changed {
            let mut matches = self
                .history
                .iter()
                .enumerate()
                .filter(|(_, item)| item.to_ascii_lowercase().contains(&query))
                .map(|(idx, _)| idx)
                .collect::<Vec<_>>();
            matches.reverse();

            if matches.is_empty() {
                self.status = "history search: no match".to_owned();
                self.history_search = None;
                return;
            }

            self.history_search = Some(HistorySearchState {
                query,
                matches,
                cursor: 0,
            });
        } else if let Some(state) = &mut self.history_search {
            state.cursor = (state.cursor + 1) % state.matches.len();
        }

        if let Some(state) = &self.history_search {
            let history_index = state.matches[state.cursor];
            self.input = self.history[history_index].clone();
            self.history_cursor = Some(history_index);
            self.status = format!(
                "history search {}/{}",
                state.cursor + 1,
                state.matches.len()
            );
        }
    }

    fn clear_history_search(&mut self) {
        self.history_search = None;
    }

    fn open_palette(&mut self) {
        self.palette_open = true;
        self.palette_filter.clear();
        self.palette_cursor = 0;
    }

    fn close_palette(&mut self) {
        self.palette_open = false;
        self.palette_filter.clear();
        self.palette_cursor = 0;
    }

    fn palette_push(&mut self, ch: char) {
        self.palette_filter.push(ch);
        self.sync_palette_cursor();
    }

    fn palette_backspace(&mut self) {
        self.palette_filter.pop();
        self.sync_palette_cursor();
    }

    fn palette_prev(&mut self) {
        let len = self.palette_matches().len();
        if len == 0 {
            self.palette_cursor = 0;
            return;
        }
        if self.palette_cursor == 0 {
            self.palette_cursor = len - 1;
        } else {
            self.palette_cursor -= 1;
        }
    }

    fn palette_next(&mut self) {
        let len = self.palette_matches().len();
        if len == 0 {
            self.palette_cursor = 0;
            return;
        }
        self.palette_cursor = (self.palette_cursor + 1) % len;
    }

    fn selected_palette_command(&self) -> Option<&'static str> {
        let matches = self.palette_matches();
        let selected = matches.get(self.palette_cursor)?;
        Some(PALETTE_ITEMS[*selected].1)
    }

    fn palette_matches(&self) -> Vec<usize> {
        let filter = self.palette_filter.trim().to_ascii_lowercase();
        PALETTE_ITEMS
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                filter.is_empty()
                    || item.0.to_ascii_lowercase().contains(&filter)
                    || item.1.to_ascii_lowercase().contains(&filter)
                    || item.2.to_ascii_lowercase().contains(&filter)
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    fn sync_palette_cursor(&mut self) {
        let len = self.palette_matches().len();
        if len == 0 {
            self.palette_cursor = 0;
        } else if self.palette_cursor >= len {
            self.palette_cursor = len - 1;
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
        let max_height = if self.transcript.is_empty() {
            viewport.height.min(MAX_DASHBOARD_HEIGHT_COMPACT)
        } else {
            viewport.height
        };
        let hero_height = home_hero_height(max_height);
        let max_feed_rows = max_height.saturating_sub(hero_height + 4).max(1);
        let feed_rows = self.desired_feed_rows(max_feed_rows, viewport.width);
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
        if self.palette_open {
            self.draw_palette(frame, root);
        }
    }

    fn desired_feed_rows(&self, max_rows: u16, width: u16) -> u16 {
        if self.transcript.is_empty() {
            return 1.min(max_rows.max(1));
        }

        let total_rows = estimate_visual_line_rows(&self.feed_lines(), width).max(2);
        total_rows.min(max_rows as usize) as u16
    }

    fn feed_lines(&self) -> Vec<Line<'static>> {
        let mut lines = self
            .transcript
            .iter()
            .map(render_entry)
            .collect::<Vec<Line>>();
        if should_surface_status_in_feed(&self.status) {
            lines.push(Line::from(Span::styled(
                format!("status: {}", self.status),
                status_style(&self.status),
            )));
        }
        lines
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
        right_lines.push(Line::from(Span::styled(
            "Ctrl+R history search · Ctrl+P palette",
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

        let lines = self.feed_lines();
        let visible_rows = area.height.max(1) as usize;
        let total_visual_rows = estimate_visual_line_rows(&lines, area.width);
        let max_scroll = total_visual_rows.saturating_sub(visible_rows);
        let scroll = compute_feed_scroll(
            self.transcript_scroll,
            self.transcript_max_scroll(),
            max_scroll,
        );

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
            let placeholder = if self.pending_approval.is_some() {
                "Approval pending: type y/yes or n/no"
            } else if self.in_home_mode() {
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
            Span::styled(
                " shortcuts: /help Ctrl+R(search) Ctrl+P(palette)",
                Style::default().fg(THEME_MUTED),
            ),
        ]))
        .style(Style::default().bg(THEME_BG));
        frame.render_widget(left, columns[0]);

        let right_text = "Update available! Run: meow self-update (soon)".to_owned();

        let right = Paragraph::new(Line::from(vec![Span::styled(
            right_text.clone(),
            status_style(&right_text),
        )]))
        .alignment(Alignment::Right)
        .style(Style::default().bg(THEME_BG));
        frame.render_widget(right, columns[1]);
    }

    fn draw_palette(&self, frame: &mut ratatui::Frame, root: Rect) {
        let width = (root.width.saturating_mul(70) / 100)
            .max(48)
            .min(root.width);
        let height = root.height.min(14).max(8);
        let x = root.x + root.width.saturating_sub(width) / 2;
        let y = root.y + root.height.saturating_sub(height) / 2;
        let area = Rect {
            x,
            y,
            width,
            height,
        };

        frame.render_widget(Clear, area);

        let matches = self.palette_matches();
        let mut lines = Vec::new();
        lines.push(Line::from(vec![
            Span::styled("filter: ", Style::default().fg(THEME_MUTED)),
            Span::styled(
                if self.palette_filter.is_empty() {
                    "(type to search)".to_owned()
                } else {
                    self.palette_filter.clone()
                },
                Style::default().fg(THEME_TEXT),
            ),
        ]));
        lines.push(Line::default());

        if matches.is_empty() {
            lines.push(Line::from(Span::styled(
                "No matching commands",
                Style::default().fg(THEME_MUTED),
            )));
        } else {
            let max_visible = 6_usize.min(matches.len());
            let start = if self.palette_cursor >= max_visible {
                self.palette_cursor + 1 - max_visible
            } else {
                0
            };
            for (visible_idx, item_idx) in matches.iter().enumerate().skip(start).take(max_visible)
            {
                let item = PALETTE_ITEMS[*item_idx];
                let selected = visible_idx == self.palette_cursor;
                let marker = if selected { ">" } else { " " };
                let style = if selected {
                    Style::default()
                        .fg(THEME_PRIMARY)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(THEME_TEXT)
                };

                lines.push(Line::from(vec![
                    Span::styled(format!("{marker} {} ", item.0), style),
                    Span::styled(item.1.to_owned(), Style::default().fg(THEME_MUTED)),
                ]));
            }
        }

        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "Enter: run  Esc: close  Up/Down: navigate",
            Style::default().fg(THEME_MUTED),
        )));

        let overlay = Paragraph::new(lines)
            .block(panel_block(" Command Palette "))
            .style(Style::default().bg(THEME_BG))
            .wrap(Wrap { trim: false });

        frame.render_widget(overlay, area);
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
        || lowered.contains("approval")
        || lowered.contains("update")
    {
        Style::default().fg(THEME_WARN).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(THEME_OK).add_modifier(Modifier::BOLD)
    }
}

fn should_surface_status_in_feed(status: &str) -> bool {
    let normalized = status.trim().to_ascii_lowercase();
    !normalized.is_empty() && normalized != "ready"
}

fn compute_feed_scroll(
    transcript_scroll: usize,
    transcript_max_scroll: usize,
    max_scroll: usize,
) -> u16 {
    if transcript_scroll >= transcript_max_scroll {
        max_scroll.min(u16::MAX as usize) as u16
    } else {
        transcript_scroll.min(max_scroll).min(u16::MAX as usize) as u16
    }
}

fn estimate_visual_line_rows(lines: &[Line<'_>], width: u16) -> usize {
    let wrap_width = width.max(1) as usize;
    lines
        .iter()
        .map(|line| line.width().max(1).div_ceil(wrap_width))
        .sum()
}

fn render_entry(entry: &TranscriptEntry) -> Line<'static> {
    let role_color = match entry.role.as_str() {
        "you" => THEME_PRIMARY,
        "meow" => THEME_OK,
        "tool" => THEME_WARN,
        "tool_err" => THEME_ERROR,
        "status" => THEME_MUTED,
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
    fn history_search_cycles_matching_entries() {
        let mut state = TuiState::new("s".to_owned(), "default".to_owned(), "p".to_owned());
        state.remember_history("build project");
        state.remember_history("git status");
        state.remember_history("git diff");
        state.input = "git".to_owned();

        state.history_search_next();
        assert_eq!(state.input, "git diff");

        state.history_search_next();
        assert_eq!(state.input, "git status");
    }

    #[test]
    fn palette_filters_and_selects_command() {
        let mut state = TuiState::new("s".to_owned(), "default".to_owned(), "p".to_owned());
        state.open_palette();
        state.palette_push('p');
        state.palette_push('r');
        state.palette_push('o');

        let selected = state.selected_palette_command();
        assert_eq!(selected, Some("/provider"));
    }

    #[test]
    fn parse_tool_args_splits_whitespace_words() {
        let parsed = parse_tool_args("shell ls -la");
        assert_eq!(parsed, vec!["shell", "ls", "-la"]);
    }

    #[test]
    fn status_feed_visibility_targets_runtime_states() {
        assert!(should_surface_status_in_feed("thinking..."));
        assert!(should_surface_status_in_feed("error"));
        assert!(should_surface_status_in_feed("interrupted by user"));
        assert!(should_surface_status_in_feed("ok | 123 ms"));
        assert!(should_surface_status_in_feed("home dashboard"));
        assert!(!should_surface_status_in_feed("ready"));
    }

    #[test]
    fn desired_feed_rows_reserves_space_for_status_line() {
        let mut state = TuiState::new("s".to_owned(), "default".to_owned(), "p".to_owned());
        state.push("you", "first".to_owned());
        state.push("meow", "second".to_owned());
        state.status = "ok | 100 ms".to_owned();

        assert_eq!(state.desired_feed_rows(10, 80), 3);
    }

    #[test]
    fn desired_feed_rows_grows_for_wrapped_lines_before_scroll() {
        let mut state = TuiState::new("s".to_owned(), "default".to_owned(), "p".to_owned());
        state.push("you", "this is a long line that wraps".to_owned());
        state.push("meow", "another long line that wraps".to_owned());
        state.status = "ok | 100 ms".to_owned();

        assert_eq!(state.desired_feed_rows(20, 10), 10);
    }

    #[test]
    fn compute_feed_scroll_follows_latest_when_at_bottom() {
        assert_eq!(compute_feed_scroll(10, 10, 5), 5);
        assert_eq!(compute_feed_scroll(3, 10, 5), 3);
        assert_eq!(compute_feed_scroll(8, 10, 5), 5);
    }

    #[test]
    fn estimate_visual_line_rows_accounts_for_wrapping() {
        let lines = vec![Line::from("12345"), Line::from("123456789"), Line::from("")];
        assert_eq!(estimate_visual_line_rows(&lines, 5), 4);
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
