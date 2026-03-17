use std::env;
use std::io;
use std::time::{Duration, Instant};

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

use crate::memory::{InitProjectOutcome, InstructionMemory, InstructionScope};
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
const MASCOT_FRAMES: [[&str; 5]; 4] = [
    [
        "       /\\_/\\",
        "      / o o \\",
        "     (   \"   )",
        "      \\~(*)~/",
        "       // \\\\",
    ],
    [
        "       /\\_/\\",
        "      / o o \\",
        "     (  -\"-  )",
        "      \\~(*)~/",
        "      _// \\\\_",
    ],
    [
        "       /\\_/\\",
        "      / ^ ^ \\",
        "     (   \"   )",
        "      \\~(*)~/",
        "       // \\\\",
    ],
    [
        "       /\\_/\\",
        "      / o o \\",
        "     (   .   )",
        "      \\~(*)~/",
        "      _// \\\\_",
    ],
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlashCommandId {
    Help,
    Home,
    Init,
    Memory,
    Palette,
    Clear,
    Session,
    Provider,
    Profile,
    New,
    Tool,
    Status,
    Quit,
}

#[derive(Debug, Clone, Copy)]
struct SlashCommandSpec {
    id: SlashCommandId,
    name: &'static str,
    aliases: &'static [&'static str],
    usage: &'static str,
    summary: &'static str,
    palette_label: &'static str,
    palette_visible: bool,
}

const SLASH_COMMANDS: &[SlashCommandSpec] = &[
    SlashCommandSpec {
        id: SlashCommandId::Help,
        name: "help",
        aliases: &["h", "commands"],
        usage: "/help",
        summary: "Show available slash commands",
        palette_label: "Help",
        palette_visible: true,
    },
    SlashCommandSpec {
        id: SlashCommandId::Home,
        name: "home",
        aliases: &["dashboard"],
        usage: "/home",
        summary: "Return focus to home dashboard",
        palette_label: "Home",
        palette_visible: true,
    },
    SlashCommandSpec {
        id: SlashCommandId::Init,
        name: "init",
        aliases: &["bootstrap"],
        usage: "/init [--force]",
        summary: "Initialize the project instruction memory file",
        palette_label: "Init Instruction Memory",
        palette_visible: true,
    },
    SlashCommandSpec {
        id: SlashCommandId::Memory,
        name: "memory",
        aliases: &["mem"],
        usage: "/memory [status|show|paths|reload]",
        summary: "Inspect and reload instruction memory scopes",
        palette_label: "Memory Status",
        palette_visible: true,
    },
    SlashCommandSpec {
        id: SlashCommandId::Palette,
        name: "palette",
        aliases: &["pal"],
        usage: "/palette",
        summary: "Open command palette",
        palette_label: "Command Palette",
        palette_visible: false,
    },
    SlashCommandSpec {
        id: SlashCommandId::Clear,
        name: "clear",
        aliases: &["cls"],
        usage: "/clear",
        summary: "Clear transcript feed",
        palette_label: "Clear Chat",
        palette_visible: true,
    },
    SlashCommandSpec {
        id: SlashCommandId::Session,
        name: "session",
        aliases: &["sid"],
        usage: "/session",
        summary: "Show current session id",
        palette_label: "Session Info",
        palette_visible: true,
    },
    SlashCommandSpec {
        id: SlashCommandId::Provider,
        name: "provider",
        aliases: &["model"],
        usage: "/provider",
        summary: "Show active provider/model",
        palette_label: "Provider Info",
        palette_visible: true,
    },
    SlashCommandSpec {
        id: SlashCommandId::Profile,
        name: "profile",
        aliases: &["persona"],
        usage: "/profile <name>",
        summary: "Switch runtime profile",
        palette_label: "Switch Profile",
        palette_visible: false,
    },
    SlashCommandSpec {
        id: SlashCommandId::New,
        name: "new",
        aliases: &["reset"],
        usage: "/new [title]",
        summary: "Start a fresh conversation session",
        palette_label: "New Session",
        palette_visible: true,
    },
    SlashCommandSpec {
        id: SlashCommandId::Tool,
        name: "tool",
        aliases: &["tools"],
        usage: "/tool [name ...]",
        summary: "List tools or run one tool",
        palette_label: "Tools",
        palette_visible: true,
    },
    SlashCommandSpec {
        id: SlashCommandId::Status,
        name: "status",
        aliases: &["state"],
        usage: "/status",
        summary: "Show live TUI/runtime status summary",
        palette_label: "Runtime Status",
        palette_visible: true,
    },
    SlashCommandSpec {
        id: SlashCommandId::Quit,
        name: "quit",
        aliases: &["exit", "q"],
        usage: "/quit",
        summary: "Exit Meow Soma",
        palette_label: "Exit",
        palette_visible: true,
    },
];

pub fn run_tui(
    state: &StateStore,
    policy: &PolicyEngine,
    tools: &ToolRegistry,
    runtime: &RuntimeAgent,
    instruction_memory: InstructionMemory,
    profile_name: &str,
    context_window: usize,
    cancellation: &CancellationToken,
) -> Result<()> {
    let session_id = state.create_session(Some("tui"))?;
    let provider = format!("{}:{}", runtime.provider_name(), runtime.provider_model());

    let mut ui = TuiState::new(
        session_id.clone(),
        profile_name.to_owned(),
        provider,
        instruction_memory,
    );

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
    restore_result?;

    run_result?;
    println!("meow closed (session: {})", ui.session_id);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
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
        ui.advance_animation();
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

#[allow(clippy::too_many_arguments)]
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

    if let Err(err) = ui.instruction_memory.reload() {
        ui.push_activity(
            "memory",
            format!("failed to refresh instruction memory before prompt: {err}"),
        );
    }

    let context_messages = load_bounded_context(
        state,
        &ui.session_id,
        context_window,
        &ui.instruction_memory,
    )?;
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
    let request_started = Instant::now();

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
            record_response_telemetry(state, runtime, response.duration_ms, true, None, ui);
            ui.replace_transcript_content(stream_index, response.text);
            ui.push_activity(
                "response",
                format!("completed in {} ms", response.duration_ms),
            );
            ui.status = format!("ok | {} ms", response.duration_ms);
        }
        Err(err) => {
            let message = err.to_string();
            let error_kind = derive_error_kind_from_message(&message);
            record_response_telemetry(
                state,
                runtime,
                request_started.elapsed().as_millis(),
                false,
                Some(error_kind.as_str()),
                ui,
            );
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
    let command = resolve_slash_command(if cmd.is_empty() { "help" } else { &cmd });

    match command.map(|spec| spec.id) {
        Some(SlashCommandId::Quit) => {
            ui.status = "exiting".to_owned();
            ui.push_activity("command", "/quit".to_owned());
            Ok(true)
        }
        Some(SlashCommandId::Help) => {
            ui.push("status", format_slash_help());
            ui.status = "slash help loaded".to_owned();
            ui.push_activity("command", "/help".to_owned());
            Ok(false)
        }
        Some(SlashCommandId::Home) => {
            ui.status = "home dashboard".to_owned();
            ui.push_activity("command", "/home".to_owned());
            Ok(false)
        }
        Some(SlashCommandId::Init) => handle_init_slash(ui, args),
        Some(SlashCommandId::Memory) => handle_memory_slash(ui, args),
        Some(SlashCommandId::Palette) => {
            ui.open_palette();
            ui.status = "command palette".to_owned();
            ui.push_activity("command", "/palette".to_owned());
            Ok(false)
        }
        Some(SlashCommandId::Clear) => {
            ui.clear_transcript();
            ui.status = "conversation cleared".to_owned();
            ui.push_activity("command", "/clear".to_owned());
            Ok(false)
        }
        Some(SlashCommandId::Session) => {
            ui.status = format!("session {}", ui.session_id);
            ui.push_activity("command", "/session".to_owned());
            Ok(false)
        }
        Some(SlashCommandId::Provider) => {
            ui.status = format!("provider {}", ui.provider);
            ui.push_activity("command", "/provider".to_owned());
            Ok(false)
        }
        Some(SlashCommandId::Profile) => {
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
        Some(SlashCommandId::New) => {
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
        Some(SlashCommandId::Tool) => run_tool_from_slash(ui, state, policy, tools, args),
        Some(SlashCommandId::Status) => {
            let pending = if ui.pending_approval.is_some() {
                "yes"
            } else {
                "no"
            };
            let memory_status = format_memory_scope_status(&ui.instruction_memory);
            let memory_paths = format_memory_path_status(&ui.instruction_memory);
            ui.push(
                "status",
                format!(
                    "session={} | provider={} | profile={} | transcript_entries={} | pending_approval={} | memory={}\n{}",
                    ui.session_id,
                    ui.provider,
                    ui.profile,
                    ui.transcript.len(),
                    pending,
                    memory_status,
                    memory_paths,
                ),
            );
            ui.status = "status loaded".to_owned();
            ui.push_activity("command", "/status".to_owned());
            Ok(false)
        }
        None => {
            let suggestions = suggest_slash_commands(&cmd, 3);
            if suggestions.is_empty() {
                ui.status = format!("unknown command: /{cmd}");
                ui.push_activity("error", format!("unknown command /{cmd}"));
            } else {
                let hint = suggestions
                    .iter()
                    .map(|name| format!("/{name}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                ui.status = format!("unknown command: /{cmd} (try {hint})");
                ui.push_activity("error", format!("unknown command /{cmd} -> {hint}"));
            }
            Ok(false)
        }
    }
}

fn handle_init_slash(ui: &mut TuiState, args: &str) -> Result<bool> {
    let force = match parse_init_force_flag(args) {
        Ok(force) => force,
        Err(message) => {
            ui.status = message.clone();
            ui.push_activity("command", "invalid /init usage".to_owned());
            ui.push_error(message);
            return Ok(false);
        }
    };

    match ui.instruction_memory.init_project_file(force) {
        Ok(result) => {
            match result.outcome {
                InitProjectOutcome::Created => {
                    ui.push(
                        "status",
                        format!(
                            "initialized project instruction file: {}",
                            result.path.display()
                        ),
                    );
                    ui.status = "project memory initialized".to_owned();
                }
                InitProjectOutcome::Overwritten => {
                    ui.push(
                        "status",
                        format!(
                            "overwrote project instruction file: {}",
                            result.path.display()
                        ),
                    );
                    ui.status = "project memory overwritten".to_owned();
                }
                InitProjectOutcome::AlreadyExists => {
                    ui.push(
                        "status",
                        format!(
                            "project instruction file already exists: {} (use /init --force)",
                            result.path.display()
                        ),
                    );
                    ui.status = "project memory already initialized".to_owned();
                }
            }
            ui.push_activity(
                "command",
                format!("/init{}", if force { " --force" } else { "" }),
            );
        }
        Err(err) => {
            ui.push_error(format!("failed to initialize project memory: {err}"));
            ui.status = "memory init failed".to_owned();
            ui.push_activity("error", format!("memory init failed: {err}"));
        }
    }

    Ok(false)
}

fn handle_memory_slash(ui: &mut TuiState, args: &str) -> Result<bool> {
    let command = args
        .split_whitespace()
        .next()
        .unwrap_or("status")
        .to_ascii_lowercase();

    match command.as_str() {
        "" | "status" => {
            ui.push(
                "status",
                format!(
                    "memory precedence: user < local < project\nscopes: {}\n{}",
                    format_memory_scope_status(&ui.instruction_memory),
                    format_memory_path_status(&ui.instruction_memory),
                ),
            );
            ui.status = "memory status loaded".to_owned();
            ui.push_activity("command", "/memory status".to_owned());
        }
        "show" => {
            if let Some(block) = ui.instruction_memory.effective_context_block() {
                ui.push("status", block);
            } else {
                ui.push(
                    "status",
                    "no instruction memory loaded (create one with /init)".to_owned(),
                );
            }
            ui.status = "memory content loaded".to_owned();
            ui.push_activity("command", "/memory show".to_owned());
        }
        "paths" => {
            ui.push("status", format_memory_path_status(&ui.instruction_memory));
            ui.status = "memory paths loaded".to_owned();
            ui.push_activity("command", "/memory paths".to_owned());
        }
        "reload" => match ui.instruction_memory.reload() {
            Ok(()) => {
                ui.push(
                    "status",
                    format!(
                        "memory reloaded: {}",
                        format_memory_scope_status(&ui.instruction_memory)
                    ),
                );
                ui.status = "memory reloaded".to_owned();
                ui.push_activity("command", "/memory reload".to_owned());
            }
            Err(err) => {
                ui.push_error(format!("memory reload failed: {err}"));
                ui.status = "memory reload failed".to_owned();
                ui.push_activity("error", format!("memory reload failed: {err}"));
            }
        },
        _ => {
            ui.push(
                "status",
                "usage: /memory [status|show|paths|reload]".to_owned(),
            );
            ui.status = "invalid /memory usage".to_owned();
            ui.push_activity("command", "invalid /memory usage".to_owned());
        }
    }

    Ok(false)
}

fn parse_init_force_flag(args: &str) -> std::result::Result<bool, String> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return Ok(false);
    }

    let mut force = false;
    for token in trimmed.split_whitespace() {
        match token {
            "--force" | "-f" | "force" => force = true,
            _ => {
                return Err(format!(
                    "usage: /init [--force] (unexpected argument: {token})"
                ));
            }
        }
    }

    Ok(force)
}

fn format_memory_scope_status(memory: &InstructionMemory) -> String {
    let loaded_scopes = memory.snapshot().loaded_scope_count();
    let scope_states = memory
        .snapshot()
        .scopes
        .iter()
        .map(|scope| format!("{}={}", scope.scope, scope.status_label()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("loaded_scopes={loaded_scopes}; {scope_states}")
}

fn format_memory_path_status(memory: &InstructionMemory) -> String {
    let paths = memory.paths();
    [
        format_scope_path_status(memory, InstructionScope::User, &paths.user),
        format_scope_path_status(memory, InstructionScope::Local, &paths.local),
        format_scope_path_status(memory, InstructionScope::Project, &paths.project),
    ]
    .join(" | ")
}

fn format_scope_path_status(
    memory: &InstructionMemory,
    scope: InstructionScope,
    path: &std::path::Path,
) -> String {
    let status = memory
        .snapshot()
        .scopes
        .iter()
        .find(|item| item.scope == scope)
        .map(|item| item.status_label())
        .unwrap_or("missing");
    format!("{scope}={} ({status})", path.display())
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
            handle_slash_command(ui, state, policy, tools, &command)
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
        state.record_approval(
            &pending.name,
            "rejected",
            &pending.reason_code,
            &pending.reason,
            false,
        )?;
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

    state.record_approval(
        &pending.name,
        "approved",
        &pending.reason_code,
        &pending.reason,
        true,
    )?;
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

    if !decision.is_allowed() {
        state.record_approval(
            &tool_name,
            "denied",
            decision.reason_code(),
            &decision.reason,
            false,
        )?;
        ui.status = format!("tool denied: {}", decision.reason);
        ui.push_error(format!("tool denied: {}", decision.reason));
        ui.push_activity("approval", format!("denied {}", tool_name));
        return Ok(false);
    }

    if decision.requires_approval() {
        state.record_approval(
            &tool_name,
            "required",
            decision.reason_code(),
            &decision.reason,
            false,
        )?;
        ui.pending_approval = Some(PendingApproval {
            name: tool_name.clone(),
            args: tool_args.clone(),
            reason_code: decision.reason_code().to_owned(),
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

fn resolve_slash_command(name: &str) -> Option<&'static SlashCommandSpec> {
    let normalized = name.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }

    SLASH_COMMANDS.iter().find(|spec| {
        spec.name == normalized || spec.aliases.iter().any(|alias| *alias == normalized)
    })
}

fn format_slash_help() -> String {
    let mut lines = Vec::with_capacity(SLASH_COMMANDS.len() + 1);
    lines.push("available slash commands:".to_owned());

    for spec in SLASH_COMMANDS {
        let alias_suffix = if spec.aliases.is_empty() {
            String::new()
        } else {
            format!(
                " (aliases: {})",
                spec.aliases
                    .iter()
                    .map(|alias| format!("/{alias}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        lines.push(format!(
            "- {}: {}{}",
            spec.usage, spec.summary, alias_suffix
        ));
    }

    lines.join("\n")
}

fn suggest_slash_commands(raw: &str, limit: usize) -> Vec<&'static str> {
    let needle = raw.trim().to_ascii_lowercase();
    if needle.is_empty() || limit == 0 {
        return Vec::new();
    }

    let mut prefix_matches = SLASH_COMMANDS
        .iter()
        .filter(|spec| {
            spec.name.starts_with(&needle)
                || spec.aliases.iter().any(|alias| alias.starts_with(&needle))
        })
        .map(|spec| spec.name)
        .collect::<Vec<_>>();
    prefix_matches.sort_unstable();
    prefix_matches.dedup();
    if !prefix_matches.is_empty() {
        prefix_matches.truncate(limit);
        return prefix_matches;
    }

    let mut fuzzy = SLASH_COMMANDS
        .iter()
        .flat_map(|spec| {
            let mut pairs = Vec::with_capacity(spec.aliases.len() + 1);
            pairs.push((levenshtein_distance(&needle, spec.name), spec.name));
            for alias in spec.aliases {
                pairs.push((levenshtein_distance(&needle, alias), spec.name));
            }
            pairs
        })
        .collect::<Vec<_>>();
    fuzzy.sort_by(|(dist_a, name_a), (dist_b, name_b)| {
        dist_a.cmp(dist_b).then_with(|| name_a.cmp(name_b))
    });

    let mut suggestions = Vec::new();
    for (distance, name) in fuzzy {
        if distance > 3 {
            break;
        }
        if suggestions.contains(&name) {
            continue;
        }
        suggestions.push(name);
        if suggestions.len() == limit {
            break;
        }
    }

    suggestions
}

fn levenshtein_distance(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    if a.is_empty() {
        return b.chars().count();
    }
    if b.is_empty() {
        return a.chars().count();
    }

    let b_chars = b.chars().collect::<Vec<_>>();
    let mut prev_row = (0..=b_chars.len()).collect::<Vec<_>>();
    let mut row = vec![0; b_chars.len() + 1];

    for (i, a_char) in a.chars().enumerate() {
        row[0] = i + 1;
        for (j, b_char) in b_chars.iter().enumerate() {
            let cost = if a_char == *b_char { 0 } else { 1 };
            row[j + 1] = (row[j] + 1)
                .min(prev_row[j + 1] + 1)
                .min(prev_row[j] + cost);
        }
        prev_row.clone_from_slice(&row);
    }

    prev_row[b_chars.len()]
}

fn palette_specs() -> Vec<&'static SlashCommandSpec> {
    SLASH_COMMANDS
        .iter()
        .filter(|spec| spec.palette_visible)
        .collect()
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
    instruction_memory: &InstructionMemory,
) -> Result<Vec<ContextMessage>> {
    let mut context_messages = instruction_memory
        .effective_context_block()
        .map(|content| {
            vec![ContextMessage {
                role: "instruction_memory".to_owned(),
                content,
            }]
        })
        .unwrap_or_default();

    context_messages.extend(
        state
            .get_recent_messages(session_id, context_window)?
            .into_iter()
            .map(|item| ContextMessage {
                role: item.role,
                content: item.content,
            }),
    );

    Ok(context_messages)
}

fn record_response_telemetry(
    state: &StateStore,
    runtime: &RuntimeAgent,
    duration_ms: u128,
    success: bool,
    error_kind: Option<&str>,
    ui: &mut TuiState,
) {
    if let Err(err) = state.record_response_latency(
        "chat",
        runtime.provider_name(),
        runtime.provider_model(),
        duration_ms,
        success,
        error_kind,
    ) {
        ui.push_activity("telemetry", format!("failed to record chat metric: {err}"));
    }
}

fn derive_error_kind_from_message(message: &str) -> String {
    if let Some(idx) = message.find("kind=") {
        let raw = &message[idx + 5..];
        if let Some(kind) = raw
            .split(|c: char| c.is_whitespace() || c == ')' || c == ',')
            .next()
            && !kind.is_empty()
        {
            return kind.to_owned();
        }
    }

    let lowered = message.to_ascii_lowercase();
    if lowered.contains("interrupted") {
        "interrupted".to_owned()
    } else {
        "unknown".to_owned()
    }
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
    reason_code: String,
    reason: String,
}

#[derive(Debug, Clone)]
struct HistorySearchState {
    query: String,
    matches: Vec<usize>,
    cursor: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FeedRowCache {
    width: u16,
    rows: usize,
}

#[derive(Debug, Default)]
struct FeedCache {
    transcript_revision: u64,
    status: String,
    lines: Vec<Line<'static>>,
    visual_rows: Option<FeedRowCache>,
}

#[derive(Debug)]
struct PreparedFeed {
    lines: Vec<Line<'static>>,
    total_visual_rows: usize,
}

struct TuiState {
    instruction_memory: InstructionMemory,
    session_id: String,
    profile: String,
    provider: String,
    user: String,
    workspace: String,
    input: String,
    status: String,
    transcript: Vec<TranscriptEntry>,
    transcript_revision: u64,
    feed_cache: FeedCache,
    transcript_scroll: usize,
    activity: Vec<ActivityEntry>,
    history: Vec<String>,
    history_cursor: Option<usize>,
    history_search: Option<HistorySearchState>,
    pending_approval: Option<PendingApproval>,
    palette_open: bool,
    palette_filter: String,
    palette_cursor: usize,
    animation_tick: u64,
}

impl TuiState {
    fn new(
        session_id: String,
        profile: String,
        provider: String,
        instruction_memory: InstructionMemory,
    ) -> Self {
        Self {
            instruction_memory,
            session_id,
            profile,
            provider,
            user: detect_user_name(),
            workspace: detect_workspace_path(),
            input: String::new(),
            status: "ready".to_owned(),
            transcript: Vec::new(),
            transcript_revision: 0,
            feed_cache: FeedCache::default(),
            transcript_scroll: 0,
            activity: Vec::new(),
            history: Vec::new(),
            history_cursor: None,
            history_search: None,
            pending_approval: None,
            palette_open: false,
            palette_filter: String::new(),
            palette_cursor: 0,
            animation_tick: 0,
        }
    }

    fn advance_animation(&mut self) {
        self.animation_tick = self.animation_tick.wrapping_add(1);
    }

    fn mascot_frame_index(&self) -> usize {
        ((self.animation_tick / 2) % MASCOT_FRAMES.len() as u64) as usize
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
        self.mark_transcript_changed();
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
        self.mark_transcript_changed();
        self.scroll_transcript_bottom();
    }

    fn append_to_transcript(&mut self, index: usize, chunk: &str) {
        if let Some(entry) = self.transcript.get_mut(index) {
            entry.content.push_str(chunk);
            self.mark_transcript_changed();
            self.scroll_transcript_bottom();
        }
    }

    fn replace_transcript_content(&mut self, index: usize, content: String) {
        if let Some(entry) = self.transcript.get_mut(index) {
            entry.content = content;
            self.mark_transcript_changed();
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
            self.mark_transcript_changed();
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
        self.mark_transcript_changed();
        self.transcript_scroll = 0;
        self.pending_approval = None;
    }

    fn mark_transcript_changed(&mut self) {
        self.transcript_revision = self.transcript_revision.wrapping_add(1);
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

    fn selected_palette_command(&self) -> Option<String> {
        let matches = self.palette_matches();
        let selected = matches.get(self.palette_cursor)?;
        let items = palette_specs();
        let command = items.get(*selected)?;
        Some(format!("/{}", command.name))
    }

    fn palette_matches(&self) -> Vec<usize> {
        let filter = self
            .palette_filter
            .trim()
            .trim_start_matches('/')
            .to_ascii_lowercase();
        palette_specs()
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                filter.is_empty()
                    || item.palette_label.to_ascii_lowercase().contains(&filter)
                    || item.name.to_ascii_lowercase().contains(&filter)
                    || item.summary.to_ascii_lowercase().contains(&filter)
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

    fn draw(&mut self, frame: &mut ratatui::Frame) {
        let viewport = frame.area();
        let max_height = if self.transcript.is_empty() {
            viewport.height.min(MAX_DASHBOARD_HEIGHT_COMPACT)
        } else {
            viewport.height
        };
        let hero_height = home_hero_height(max_height);
        let fixed_rows = hero_height.saturating_add(4);
        let max_feed_rows = max_height.saturating_sub(fixed_rows).max(1);
        let feed_rows = self.desired_feed_rows(max_feed_rows, viewport.width);
        let feed = self.prepare_feed(viewport.width);
        let root_height = hero_height.saturating_add(feed_rows).saturating_add(4);

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
        self.draw_conversation_feed(frame, home[1], feed);
        self.draw_input(frame, home[2]);
        self.draw_footer(frame, home[3]);
        if self.palette_open {
            self.draw_palette(frame, root);
        }
    }

    fn desired_feed_rows(&mut self, max_rows: u16, width: u16) -> u16 {
        if self.transcript.is_empty() {
            return 1;
        }

        let total_rows = self.feed_visual_rows(width);
        self.desired_feed_rows_from_total(max_rows, total_rows)
    }

    fn desired_feed_rows_from_total(&self, max_rows: u16, total_rows: usize) -> u16 {
        if self.transcript.is_empty() {
            return 1;
        }

        let bounded = total_rows.max(2).min(usize::from(max_rows));
        u16::try_from(bounded).unwrap_or(max_rows)
    }

    fn prepare_feed(&mut self, width: u16) -> PreparedFeed {
        let total_visual_rows = self.feed_visual_rows(width);
        PreparedFeed {
            lines: self.feed_cache.lines.clone(),
            total_visual_rows,
        }
    }

    fn ensure_feed_cache_current(&mut self) {
        if self.feed_cache.transcript_revision == self.transcript_revision
            && self.feed_cache.status == self.status
        {
            return;
        }

        self.feed_cache.lines.clear();
        self.feed_cache
            .lines
            .extend(self.transcript.iter().map(render_entry));
        if should_surface_status_in_feed(&self.status) {
            self.feed_cache.lines.push(Line::from(Span::styled(
                format!("status: {}", self.status),
                status_style(&self.status),
            )));
        }
        self.feed_cache.transcript_revision = self.transcript_revision;
        self.feed_cache.status.clone_from(&self.status);
        self.feed_cache.visual_rows = None;
    }

    fn feed_visual_rows(&mut self, width: u16) -> usize {
        self.ensure_feed_cache_current();
        if let Some(cached) = self.feed_cache.visual_rows
            && cached.width == width
        {
            return cached.rows;
        }

        let rows = estimate_visual_line_rows(&self.feed_cache.lines, width);
        self.feed_cache.visual_rows = Some(FeedRowCache { width, rows });
        rows
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

        let mascot_lines = MASCOT_FRAMES[self.mascot_frame_index()]
            .iter()
            .map(|line| {
                Line::from(Span::styled(
                    (*line).to_owned(),
                    Style::default().fg(THEME_PRIMARY),
                ))
            })
            .collect::<Vec<_>>();

        let mascot = Paragraph::new(mascot_lines)
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

        let mut right_lines = vec![
            Line::from(Span::styled(
                "Recent activity",
                Style::default()
                    .fg(THEME_PRIMARY)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::default(),
        ];

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

    fn draw_conversation_feed(&self, frame: &mut ratatui::Frame, area: Rect, feed: PreparedFeed) {
        if area.height == 0 {
            return;
        }

        let visible_rows = usize::from(area.height.max(1));
        let max_scroll = feed.total_visual_rows.saturating_sub(visible_rows);
        let scroll = compute_feed_scroll(
            self.transcript_scroll,
            self.transcript_max_scroll(),
            max_scroll,
        );

        let conversation = Paragraph::new(feed.lines)
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
        let height = root.height.clamp(8, 14);
        let x = root.x + root.width.saturating_sub(width) / 2;
        let y = root.y + root.height.saturating_sub(height) / 2;
        let area = Rect {
            x,
            y,
            width,
            height,
        };

        frame.render_widget(Clear, area);

        let items = palette_specs();
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
                let item = items[*item_idx];
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
                    Span::styled(format!("{marker} {} ", item.palette_label), style),
                    Span::styled(format!("/{}", item.name), Style::default().fg(THEME_MUTED)),
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

fn saturating_visual_row_total(rows: impl IntoIterator<Item = usize>) -> usize {
    rows.into_iter()
        .fold(0_usize, |total, row_count| total.saturating_add(row_count))
}

fn estimate_visual_line_rows(lines: &[Line<'_>], width: u16) -> usize {
    let wrap_width = usize::from(width.max(1));
    saturating_visual_row_total(
        lines
            .iter()
            .map(|line| line.width().max(1).div_ceil(wrap_width)),
    )
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
    use crate::memory::InstructionPaths;
    use uuid::Uuid;

    fn test_instruction_memory() -> InstructionMemory {
        let root = std::env::temp_dir().join(format!("meow-tui-memory-{}", Uuid::new_v4()));
        InstructionMemory::from_paths(InstructionPaths {
            user: root.join("user.md"),
            local: root.join("local.md"),
            project: root.join("project.md"),
            project_root: root,
        })
        .expect("test instruction memory should load")
    }

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
        let mut state = TuiState::new(
            "s".to_owned(),
            "default".to_owned(),
            "p".to_owned(),
            test_instruction_memory(),
        );
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
    fn slash_registry_resolves_aliases() {
        assert_eq!(
            resolve_slash_command("model").map(|spec| spec.id),
            Some(SlashCommandId::Provider)
        );
        assert_eq!(
            resolve_slash_command("tools").map(|spec| spec.id),
            Some(SlashCommandId::Tool)
        );
        assert_eq!(
            resolve_slash_command("mem").map(|spec| spec.id),
            Some(SlashCommandId::Memory)
        );
        assert_eq!(
            resolve_slash_command("q").map(|spec| spec.id),
            Some(SlashCommandId::Quit)
        );
    }

    #[test]
    fn slash_help_includes_status_and_aliases() {
        let help = format_slash_help();
        assert!(help.contains("/status"));
        assert!(help.contains("/memory"));
        assert!(help.contains("/model"));
        assert!(help.contains("/mem"));
        assert!(help.contains("/tools"));
    }

    #[test]
    fn init_flag_parser_supports_force_and_rejects_unknown_tokens() {
        assert_eq!(
            parse_init_force_flag("").expect("empty args should parse"),
            false
        );
        assert_eq!(
            parse_init_force_flag("--force").expect("force args should parse"),
            true
        );
        assert_eq!(
            parse_init_force_flag("-f").expect("short force args should parse"),
            true
        );
        let err = parse_init_force_flag("--unknown").expect_err("unexpected args should fail");
        assert!(err.contains("unexpected argument"));
    }

    #[test]
    fn slash_suggestions_cover_prefix_and_fuzzy_matches() {
        let prefix = suggest_slash_commands("pro", 3);
        assert_eq!(prefix, vec!["profile", "provider"]);

        let fuzzy = suggest_slash_commands("provder", 3);
        assert!(fuzzy.contains(&"provider"));
    }

    #[test]
    fn history_navigation_roundtrip() {
        let mut state = TuiState::new(
            "s".to_owned(),
            "default".to_owned(),
            "p".to_owned(),
            test_instruction_memory(),
        );
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
        let mut state = TuiState::new(
            "s".to_owned(),
            "default".to_owned(),
            "p".to_owned(),
            test_instruction_memory(),
        );
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
        let mut state = TuiState::new(
            "s".to_owned(),
            "default".to_owned(),
            "p".to_owned(),
            test_instruction_memory(),
        );
        state.open_palette();
        state.palette_push('p');
        state.palette_push('r');
        state.palette_push('o');
        state.palette_push('v');

        let selected = state.selected_palette_command();
        assert_eq!(selected.as_deref(), Some("/provider"));
    }

    #[test]
    fn palette_supports_slash_prefixed_filter_and_skips_profile() {
        let mut state = TuiState::new(
            "s".to_owned(),
            "default".to_owned(),
            "p".to_owned(),
            test_instruction_memory(),
        );
        state.open_palette();
        for ch in "/prov".chars() {
            state.palette_push(ch);
        }

        let selected = state.selected_palette_command();
        assert_eq!(selected.as_deref(), Some("/provider"));

        let has_profile = palette_specs().iter().any(|spec| spec.name == "profile");
        assert!(!has_profile);
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
        let mut state = TuiState::new(
            "s".to_owned(),
            "default".to_owned(),
            "p".to_owned(),
            test_instruction_memory(),
        );
        state.push("you", "first".to_owned());
        state.push("meow", "second".to_owned());
        state.status = "ok | 100 ms".to_owned();

        assert_eq!(state.desired_feed_rows(10, 80), 3);
    }

    #[test]
    fn desired_feed_rows_grows_for_wrapped_lines_before_scroll() {
        let mut state = TuiState::new(
            "s".to_owned(),
            "default".to_owned(),
            "p".to_owned(),
            test_instruction_memory(),
        );
        state.push("you", "this is a long line that wraps".to_owned());
        state.push("meow", "another long line that wraps".to_owned());
        state.status = "ok | 100 ms".to_owned();

        assert_eq!(state.desired_feed_rows(20, 10), 10);
    }

    #[test]
    fn feed_cache_invalidates_for_status_transcript_and_width_changes() {
        let mut state = TuiState::new(
            "s".to_owned(),
            "default".to_owned(),
            "p".to_owned(),
            test_instruction_memory(),
        );
        state.push("you", "short line".to_owned());

        let baseline_rows = state.feed_visual_rows(20);
        assert_eq!(state.feed_cache.lines.len(), 1);
        assert_eq!(
            state.feed_cache.visual_rows,
            Some(FeedRowCache {
                width: 20,
                rows: baseline_rows
            })
        );

        state.status = "thinking...".to_owned();
        let status_rows = state.feed_visual_rows(20);
        assert!(status_rows >= baseline_rows);
        assert_eq!(state.feed_cache.lines.len(), 2);
        assert_eq!(state.feed_cache.status, "thinking...");

        let revision_after_status = state.feed_cache.transcript_revision;
        state.append_to_transcript(0, " plus more text");
        let wider_rows = state.feed_visual_rows(40);
        assert_ne!(state.feed_cache.transcript_revision, revision_after_status);
        assert_eq!(
            state.feed_cache.visual_rows,
            Some(FeedRowCache {
                width: 40,
                rows: wider_rows
            })
        );
        assert!(wider_rows <= state.feed_visual_rows(20));
    }

    #[test]
    fn compute_feed_scroll_follows_latest_when_at_bottom() {
        assert_eq!(compute_feed_scroll(10, 10, 5), 5);
        assert_eq!(compute_feed_scroll(3, 10, 5), 3);
        assert_eq!(compute_feed_scroll(8, 10, 5), 5);
        assert_eq!(
            compute_feed_scroll(usize::MAX, usize::MAX, usize::MAX),
            u16::MAX
        );
    }

    #[test]
    fn estimate_visual_line_rows_accounts_for_wrapping() {
        let lines = vec![Line::from("12345"), Line::from("123456789"), Line::from("")];
        assert_eq!(estimate_visual_line_rows(&lines, 5), 4);
    }

    #[test]
    fn saturating_visual_row_total_clamps_on_overflow() {
        let rows = saturating_visual_row_total([usize::MAX - 2, 10]);
        assert_eq!(rows, usize::MAX);
    }

    #[test]
    fn transcript_scroll_boundaries_are_stable() {
        let mut state = TuiState::new(
            "s".to_owned(),
            "default".to_owned(),
            "p".to_owned(),
            test_instruction_memory(),
        );
        for idx in 0..5 {
            state.push("you", idx.to_string());
        }

        state.scroll_transcript_top();
        state.scroll_transcript_up(usize::MAX);
        assert_eq!(state.transcript_scroll, 0);

        state.scroll_transcript_down(usize::MAX);
        assert_eq!(state.transcript_scroll, state.transcript_max_scroll());

        state.scroll_transcript_down(usize::MAX);
        assert_eq!(state.transcript_scroll, state.transcript_max_scroll());

        state.scroll_transcript_up(2);
        assert_eq!(
            state.transcript_scroll,
            state.transcript_max_scroll().saturating_sub(2)
        );
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

    #[test]
    fn mascot_animation_cycles_frames() {
        let mut state = TuiState::new(
            "s".to_owned(),
            "default".to_owned(),
            "p".to_owned(),
            test_instruction_memory(),
        );
        assert_eq!(state.mascot_frame_index(), 0);

        state.advance_animation();
        assert_eq!(state.mascot_frame_index(), 0);

        state.advance_animation();
        assert_eq!(state.mascot_frame_index(), 1);

        for _ in 0..6 {
            state.advance_animation();
        }
        assert_eq!(state.mascot_frame_index(), 0);
    }
}
