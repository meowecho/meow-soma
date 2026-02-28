use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::cli::{
    ChatArgs, Cli, Commands, ConfigCommand, ExportFormat, McpCommand, McpServeArgs, SessionCommand,
    SessionExportArgs, SessionResumeArgs, ToolCommand, ToolExecArgs,
};
use crate::config;
use crate::policy::PolicyEngine;
use crate::providers::build_provider;
use crate::runtime::{
    CancellationToken, ContextMessage, RuntimeAgent, RuntimeExecutionContext, RuntimeOperation,
};
use crate::state::StateStore;
use crate::tools::{ToolOutput, ToolRegistry};

pub fn run(cli: Cli) -> Result<()> {
    let config_override = cli.config.clone();

    match cli.command {
        Commands::Config { command } => handle_config(command, config_override.as_ref()),
        command => {
            let cfg = config::load(config_override.as_ref())?;
            config::validate(&cfg)?;

            ensure_storage_dirs(&cfg)?;

            let sqlite_path = config::resolve_path(&cfg.storage.sqlite_path)?;
            let state = StateStore::open(&sqlite_path)?;

            let policy = PolicyEngine::new(&cfg.security);
            let tools = ToolRegistry::new();
            let runtime = RuntimeAgent::new(build_provider(&cfg));
            let profile_name = cfg.project.default_profile.clone();
            let context_window = cfg.runtime.max_steps.max(1) as usize;
            let cancellation = CancellationToken::new(shared_interrupt_flag()?);

            match command {
                Commands::Chat(args) => run_chat(
                    &state,
                    &runtime,
                    &profile_name,
                    context_window,
                    &cancellation,
                    args,
                ),
                Commands::Ask(args) => {
                    run_ask(&state, &runtime, &profile_name, &cancellation, &args.prompt)
                }
                Commands::Run(args) => {
                    run_goal(&state, &runtime, &profile_name, &cancellation, &args.goal)
                }
                Commands::Tool { command } => run_tool_command(&state, &policy, &tools, command),
                Commands::Mcp { command } => run_mcp_command(&state, &policy, &tools, command),
                Commands::Session { command } => run_session_command(&state, command),
                Commands::Config { .. } => unreachable!(),
            }
        }
    }
}

fn run_chat(
    state: &StateStore,
    runtime: &RuntimeAgent,
    profile_name: &str,
    context_window: usize,
    cancellation: &CancellationToken,
    args: ChatArgs,
) -> Result<()> {
    let session_id = state.create_session(args.title.as_deref())?;
    println!(
        "meow chat started (session: {session_id}, profile: {profile_name}, provider: {}:{}).",
        runtime.provider_name(),
        runtime.provider_model()
    );
    println!("commands: /exit /quit");

    let (tx, rx) = mpsc::channel::<ChatInputEvent>();
    thread::spawn(move || {
        let stdin = io::stdin();
        loop {
            let mut input = String::new();
            match stdin.read_line(&mut input) {
                Ok(0) => {
                    let _ = tx.send(ChatInputEvent::Eof);
                    break;
                }
                Ok(_) => {
                    if tx.send(ChatInputEvent::Line(input)).is_err() {
                        break;
                    }
                }
                Err(err) => {
                    let _ = tx.send(ChatInputEvent::Error(err.to_string()));
                    break;
                }
            }
        }
    });

    loop {
        print!("you> ");
        io::stdout().flush()?;

        let Some(input) = wait_for_chat_input(&rx, cancellation)? else {
            println!();
            if cancellation.is_cancelled() {
                println!("chat interrupted by user");
            }
            break;
        };

        let prompt = input.trim();
        if prompt.is_empty() {
            continue;
        }

        if matches!(prompt, "/exit" | "/quit") {
            break;
        }

        let context_messages = load_bounded_context(state, &session_id, context_window)?;
        let runtime_context = RuntimeExecutionContext::new(
            RuntimeOperation::Chat,
            profile_name,
            Some(session_id.clone()),
            context_messages,
        );

        state.add_message(&session_id, "user", prompt)?;
        cancellation.clear();
        match runtime.respond_with_context(&runtime_context, prompt, cancellation) {
            Ok(response) => {
                println!("meow> {}", response.text);
                println!("[{} ms]", response.duration_ms);
                state.add_message(&session_id, "assistant", &response.text)?;
            }
            Err(err) => {
                if cancellation.is_cancelled() {
                    println!();
                    println!("chat interrupted by user");
                    break;
                }
                let message = err.to_string();
                eprintln!("meow error: {message}");
                state.add_message(&session_id, "assistant", &format!("[error] {message}"))?;
            }
        }
    }

    Ok(())
}

fn run_ask(
    state: &StateStore,
    runtime: &RuntimeAgent,
    profile_name: &str,
    cancellation: &CancellationToken,
    prompt: &str,
) -> Result<()> {
    let session_id = state.create_session(Some("ask"))?;
    state.add_message(&session_id, "user", prompt)?;

    let runtime_context = RuntimeExecutionContext::new(
        RuntimeOperation::Ask,
        profile_name,
        Some(session_id.clone()),
        Vec::new(),
    );

    cancellation.clear();
    let response = runtime.respond_with_context(&runtime_context, prompt, cancellation)?;
    println!("{}", response.text);
    println!(
        "[meta] profile={} provider={}:{} duration_ms={}",
        profile_name,
        runtime.provider_name(),
        runtime.provider_model(),
        response.duration_ms
    );

    state.add_message(&session_id, "assistant", &response.text)?;
    Ok(())
}

fn run_goal(
    state: &StateStore,
    runtime: &RuntimeAgent,
    profile_name: &str,
    cancellation: &CancellationToken,
    goal: &str,
) -> Result<()> {
    let runtime_context =
        RuntimeExecutionContext::new(RuntimeOperation::Run, profile_name, None, Vec::new());

    cancellation.clear();
    match runtime.run_goal_with_context(&runtime_context, goal, cancellation) {
        Ok(response) => {
            let run_id = state.record_run(goal, &response.text, "ok")?;
            println!("run_id      : {run_id}");
            println!("profile     : {profile_name}");
            println!(
                "provider    : {}:{}",
                runtime.provider_name(),
                runtime.provider_model()
            );
            println!("started_at  : {}", response.started_at);
            println!("finished_at : {}", response.finished_at);
            println!("duration_ms : {}", response.duration_ms);
            println!("status      : ok");
            println!();
            println!("{}", response.text);
            Ok(())
        }
        Err(err) => {
            state.record_run(goal, &err.to_string(), "error")?;
            Err(err)
        }
    }
}

fn run_tool_command(
    state: &StateStore,
    policy: &PolicyEngine,
    tools: &ToolRegistry,
    command: ToolCommand,
) -> Result<()> {
    match command {
        ToolCommand::List => {
            for spec in tools.list() {
                println!("{}\trisky={}\t{}", spec.name, spec.risky, spec.description);
            }
            Ok(())
        }
        ToolCommand::Exec(args) => {
            let output = execute_tool_with_policy(state, policy, tools, args)?;
            if !output.stdout.is_empty() {
                println!("{}", output.stdout);
            }
            if !output.stderr.is_empty() {
                eprintln!("{}", output.stderr);
            }
            Ok(())
        }
    }
}

fn run_mcp_command(
    state: &StateStore,
    policy: &PolicyEngine,
    tools: &ToolRegistry,
    command: McpCommand,
) -> Result<()> {
    match command {
        McpCommand::Serve(args) => serve_mcp_stdio(state, policy, tools, args),
    }
}

fn serve_mcp_stdio(
    state: &StateStore,
    policy: &PolicyEngine,
    tools: &ToolRegistry,
    args: McpServeArgs,
) -> Result<()> {
    if args.transport != "stdio" {
        bail!("only stdio transport is supported in this MVP scaffold");
    }

    eprintln!("meow mcp stdio server started. one JSON request per line.");

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let raw = line?;
        let trimmed = raw.trim();

        if trimmed.is_empty() {
            continue;
        }

        if matches!(trimmed, "exit" | "quit") {
            break;
        }

        let request: Result<McpRequest, _> = serde_json::from_str(trimmed);
        let response = match request {
            Ok(req) => {
                let request_id = req.id.clone();
                match execute_tool_with_policy(
                    state,
                    policy,
                    tools,
                    ToolExecArgs {
                        name: req.tool,
                        args: req.args,
                        approve: req.approve,
                    },
                ) {
                    Ok(output) => McpResponse {
                        id: request_id,
                        ok: true,
                        output: Some(output),
                        error: None,
                    },
                    Err(err) => McpResponse {
                        id: request_id,
                        ok: false,
                        output: None,
                        error: Some(err.to_string()),
                    },
                }
            }
            Err(err) => McpResponse {
                id: None,
                ok: false,
                output: None,
                error: Some(format!("invalid JSON request: {err}")),
            },
        };

        let line = serde_json::to_string(&response)?;
        writeln!(stdout, "{line}")?;
        stdout.flush()?;
    }

    Ok(())
}

fn run_session_command(state: &StateStore, command: SessionCommand) -> Result<()> {
    match command {
        SessionCommand::List => {
            let sessions = state.list_sessions()?;
            if sessions.is_empty() {
                println!("no saved sessions");
                return Ok(());
            }

            println!("{:<36}  {:<18}  UPDATED_AT", "SESSION_ID", "TITLE");
            for session in sessions {
                let title = session.title.unwrap_or_else(|| "(untitled)".to_owned());
                println!(
                    "{:<36}  {:<18}  {}",
                    session.id,
                    truncate_display(&title, 18),
                    session.updated_at
                );
            }
            Ok(())
        }
        SessionCommand::Resume(args) => resume_session(state, args),
        SessionCommand::Export(args) => export_session(state, args),
    }
}

fn resume_session(state: &StateStore, args: SessionResumeArgs) -> Result<()> {
    let messages = state.get_messages(&args.session_id)?;
    if messages.is_empty() {
        println!("no messages for session {}", args.session_id);
        return Ok(());
    }

    println!("session: {}", args.session_id);
    for (idx, message) in messages.iter().enumerate() {
        println!();
        println!("[{idx}] {} | {}", message.role, message.created_at);
        println!("{}", message.content);
    }

    Ok(())
}

fn export_session(state: &StateStore, args: SessionExportArgs) -> Result<()> {
    let messages = state.get_messages(&args.session_id)?;
    if messages.is_empty() {
        bail!("session {} has no messages", args.session_id);
    }

    let rendered = match args.format {
        ExportFormat::Json => serde_json::to_string_pretty(&messages)?,
        ExportFormat::Markdown => render_markdown_session(&args.session_id, &messages),
    };

    if let Some(path) = args.output {
        write_text_file(&path, &rendered)?;
        println!("exported session to {}", path.display());
    } else {
        println!("{rendered}");
    }

    Ok(())
}

fn run_config_validate(path_override: Option<&PathBuf>) -> Result<()> {
    let cfg = config::load(path_override)?;
    config::validate(&cfg)?;
    println!("config is valid");
    Ok(())
}

fn handle_config(command: ConfigCommand, path_override: Option<&PathBuf>) -> Result<()> {
    match command {
        ConfigCommand::Init(args) => {
            let target = args.output.as_ref().or(path_override);
            let path = config::write_default(target, args.force)?;
            println!("wrote config: {}", path.display());
            Ok(())
        }
        ConfigCommand::Validate => run_config_validate(path_override),
        ConfigCommand::Path => {
            let path = path_override
                .cloned()
                .map(Ok)
                .unwrap_or_else(config::canonical_config_path)?;
            println!("{}", path.display());
            Ok(())
        }
    }
}

fn execute_tool_with_policy(
    state: &StateStore,
    policy: &PolicyEngine,
    tools: &ToolRegistry,
    args: ToolExecArgs,
) -> Result<ToolOutput> {
    if !tools.is_known(&args.name) {
        bail!("unknown tool: {}", args.name);
    }

    let decision = if args.name == "shell" {
        policy.evaluate_shell(&args.args.join(" "))
    } else {
        policy.evaluate_tool(&args.name, ToolRegistry::is_risky(&args.name))
    };

    if !decision.allowed {
        state.record_approval(&args.name, "denied", &decision.reason, false)?;
        bail!("tool execution denied: {}", decision.reason);
    }

    if decision.requires_approval {
        if !args.approve {
            state.record_approval(&args.name, "required", &decision.reason, false)?;
            bail!(
                "tool execution requires approval ({}) - re-run with --approve",
                decision.reason
            );
        }
        state.record_approval(&args.name, "approved", &decision.reason, true)?;
    }

    let tool_output = tools.execute(&args.name, &args.args);
    match tool_output {
        Ok(output) => {
            state.record_tool_call(
                &args.name,
                &args.args.join(" "),
                &output.status,
                &output.stdout,
            )?;
            Ok(output)
        }
        Err(err) => {
            state.record_tool_call(&args.name, &args.args.join(" "), "error", &err.to_string())?;
            Err(err)
        }
    }
}

fn render_markdown_session(session_id: &str, messages: &[crate::state::MessageRecord]) -> String {
    let mut out = format!("# Session {}\n\n", session_id);
    for message in messages {
        out.push_str(&format!(
            "## {} ({})\n\n{}\n\n",
            message.role, message.created_at, message.content
        ));
    }
    out
}

fn write_text_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed creating parent export directory: {}",
                parent.display()
            )
        })?;
    }

    fs::write(path, content)
        .with_context(|| format!("failed writing export file: {}", path.display()))?;
    Ok(())
}

fn ensure_storage_dirs(cfg: &config::MeowConfig) -> Result<()> {
    for raw in [
        cfg.storage.root_dir.as_str(),
        cfg.storage.memory_dir.as_str(),
        cfg.storage.artifacts_dir.as_str(),
        cfg.storage.logs_dir.as_str(),
    ] {
        let path = config::resolve_path(raw)?;
        fs::create_dir_all(&path)
            .with_context(|| format!("failed to create storage directory: {}", path.display()))?;
    }
    Ok(())
}

fn wait_for_chat_input(
    rx: &mpsc::Receiver<ChatInputEvent>,
    cancellation: &CancellationToken,
) -> Result<Option<String>> {
    loop {
        if cancellation.is_cancelled() {
            return Ok(None);
        }

        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(ChatInputEvent::Line(line)) => return Ok(Some(line)),
            Ok(ChatInputEvent::Eof) => return Ok(None),
            Ok(ChatInputEvent::Error(err)) => {
                return Err(anyhow!("failed to read chat input: {err}"));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(None),
        }
    }
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

fn truncate_display(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }

    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }

    let mut out = value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    out.push_str("...");
    out
}

fn shared_interrupt_flag() -> Result<Arc<AtomicBool>> {
    static FLAG: OnceLock<Arc<AtomicBool>> = OnceLock::new();
    static HANDLER_INSTALLED: OnceLock<()> = OnceLock::new();

    let flag = FLAG
        .get_or_init(|| Arc::new(AtomicBool::new(false)))
        .clone();

    if HANDLER_INSTALLED.get().is_none() {
        let handler_flag = Arc::clone(&flag);
        ctrlc::set_handler(move || {
            handler_flag.store(true, Ordering::SeqCst);
        })
        .context("failed to install Ctrl+C handler")?;

        let _ = HANDLER_INSTALLED.set(());
    }

    Ok(flag)
}

#[derive(Debug, Deserialize)]
struct McpRequest {
    id: Option<String>,
    tool: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    approve: bool,
}

#[derive(Debug, Serialize)]
struct McpResponse {
    id: Option<String>,
    ok: bool,
    output: Option<ToolOutput>,
    error: Option<String>,
}

enum ChatInputEvent {
    Line(String),
    Eof,
    Error(String),
}
