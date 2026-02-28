use std::io;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::runtime::{
    CancellationToken, ContextMessage, RuntimeAgent, RuntimeExecutionContext, RuntimeOperation,
};
use crate::state::StateStore;

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
    ui.push_system(format!("session {session_id} started"));
    ui.push_system("enter submits, /quit exits, esc exits, ctrl+c exits".to_owned());

    let mut stdout = io::stdout();
    enable_raw_mode().context("failed to enable raw mode")?;
    execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to initialize terminal backend")?;
    terminal.clear().context("failed to clear terminal")?;

    let run_result = run_loop(
        &mut terminal,
        &mut ui,
        state,
        runtime,
        profile_name,
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
    profile_name: &str,
    context_window: usize,
    cancellation: &CancellationToken,
) -> Result<()> {
    loop {
        terminal
            .draw(|frame| ui.draw(frame))
            .context("failed to render tui frame")?;

        if cancellation.is_cancelled() {
            ui.status = "interrupted by signal".to_owned();
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
                    break;
                }

                match key.code {
                    KeyCode::Enter => {
                        let should_exit = submit_prompt(
                            ui,
                            state,
                            runtime,
                            profile_name,
                            context_window,
                            cancellation,
                        )?;
                        if should_exit {
                            break;
                        }
                    }
                    KeyCode::Backspace => {
                        ui.input.pop();
                    }
                    KeyCode::Char(ch) => {
                        if key.modifiers.contains(KeyModifiers::CONTROL) {
                            continue;
                        }
                        ui.input.push(ch);
                    }
                    KeyCode::Tab => ui.input.push('\t'),
                    _ => {}
                }
            }
            Event::Resize(_, _) => {
                // redraw is handled on next loop tick
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
    profile_name: &str,
    context_window: usize,
    cancellation: &CancellationToken,
) -> Result<bool> {
    let prompt = ui.input.trim().to_owned();
    ui.input.clear();

    if prompt.is_empty() {
        return Ok(false);
    }

    if matches!(prompt.as_str(), "/quit" | "/exit") {
        ui.status = "exiting".to_owned();
        return Ok(true);
    }

    let context_messages = load_bounded_context(state, &ui.session_id, context_window)?;
    let runtime_context = RuntimeExecutionContext::new(
        RuntimeOperation::Chat,
        profile_name,
        Some(ui.session_id.clone()),
        context_messages,
    );

    ui.push_user(prompt.clone());
    state.add_message(&ui.session_id, "user", &prompt)?;

    ui.status = "thinking...".to_owned();
    cancellation.clear();

    match runtime.respond_with_context(&runtime_context, &prompt, cancellation) {
        Ok(response) => {
            state.add_message(&ui.session_id, "assistant", &response.text)?;
            ui.push_assistant(response.text);
            ui.status = format!("ok | {} ms", response.duration_ms);
        }
        Err(err) => {
            let message = err.to_string();
            ui.push_error(message.clone());
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
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("failed to leave alternate screen")?;
    terminal.show_cursor().context("failed to show cursor")?;
    Ok(())
}

#[derive(Debug, Clone)]
struct TranscriptEntry {
    role: String,
    content: String,
}

struct TuiState {
    session_id: String,
    profile: String,
    provider: String,
    input: String,
    status: String,
    transcript: Vec<TranscriptEntry>,
}

impl TuiState {
    fn new(session_id: String, profile: String, provider: String) -> Self {
        Self {
            session_id,
            profile,
            provider,
            input: String::new(),
            status: "ready".to_owned(),
            transcript: Vec::new(),
        }
    }

    fn push_system(&mut self, message: String) {
        self.push("system", message);
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
    }

    fn draw(&self, frame: &mut ratatui::Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(8),
                Constraint::Length(3),
                Constraint::Length(2),
            ])
            .split(frame.area());

        let lines = self
            .transcript
            .iter()
            .map(render_entry)
            .collect::<Vec<Line>>();

        let transcript = Paragraph::new(lines)
            .block(
                Block::default()
                    .title(format!(
                        "meow | profile={} | provider={} | session={} ",
                        self.profile, self.provider, self.session_id
                    ))
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false });
        frame.render_widget(transcript, chunks[0]);

        let input = Paragraph::new(self.input.as_str())
            .block(Block::default().title("input").borders(Borders::ALL))
            .wrap(Wrap { trim: false });
        frame.render_widget(input, chunks[1]);

        let status = Paragraph::new(self.status.as_str())
            .block(Block::default().title("status").borders(Borders::ALL));
        frame.render_widget(status, chunks[2]);
    }
}

fn render_entry(entry: &TranscriptEntry) -> Line<'static> {
    let role_color = match entry.role.as_str() {
        "you" => Color::Cyan,
        "meow" => Color::Green,
        "error" => Color::Red,
        _ => Color::Yellow,
    };

    Line::from(vec![
        Span::styled(format!("{}: ", entry.role), Style::default().fg(role_color)),
        Span::raw(entry.content.clone()),
    ])
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
}
