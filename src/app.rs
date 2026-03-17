use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::cli::{
    AskArgs, Cli, Commands, ConfigCommand, ConfigSetupArgs, ExportFormat, McpCommand,
    MetricsCommand, ResponseOutput, RunArgs, SessionCommand, SessionExportArgs, SessionForkArgs,
    SessionImportArgs, SessionResumeArgs, SetupProvider, ToolCommand, ToolExecArgs,
};
use crate::config;
use crate::mcp;
use crate::memory::InstructionMemory;
use crate::policy::PolicyEngine;
use crate::providers::build_provider;
use crate::runtime::{
    CancellationToken, ContextMessage, RuntimeAgent, RuntimeExecutionContext, RuntimeOperation,
};
use crate::state::StateStore;
use crate::tools::{ToolOutput, ToolRegistry};
use crate::tui;

pub fn run(cli: Cli) -> Result<()> {
    let started = Instant::now();
    let config_override = cli.config.clone();

    match cli.command {
        Some(Commands::Config { command }) => handle_config(command, config_override.as_ref()),
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
            let mode = command_mode(command.as_ref());
            if mode != "metrics" {
                record_startup_metric(&state, mode, started.elapsed().as_millis());
            }

            match command {
                None => tui::run_tui(
                    &state,
                    &policy,
                    &tools,
                    &runtime,
                    load_instruction_memory(&cfg)?,
                    &profile_name,
                    context_window,
                    &cancellation,
                    None,
                ),
                Some(Commands::Ask(args)) => run_ask(
                    &state,
                    &runtime,
                    &load_instruction_memory(&cfg)?,
                    &profile_name,
                    context_window,
                    &cancellation,
                    args,
                ),
                Some(Commands::Run(args)) => run_goal(
                    &state,
                    &runtime,
                    &load_instruction_memory(&cfg)?,
                    &profile_name,
                    context_window,
                    &cancellation,
                    args,
                ),
                Some(Commands::Tool { command }) => {
                    run_tool_command(&state, &policy, &tools, command)
                }
                Some(Commands::Mcp { command }) => {
                    run_mcp_command(&state, &policy, &tools, command)
                }
                Some(Commands::Session { command }) => match command {
                    SessionCommand::Continue(args) => {
                        let session = state.resolve_session_ref(&args.session)?;
                        tui::run_tui(
                            &state,
                            &policy,
                            &tools,
                            &runtime,
                            load_instruction_memory(&cfg)?,
                            &profile_name,
                            context_window,
                            &cancellation,
                            Some(session.id.as_str()),
                        )
                    }
                    other => run_session_command(&state, other),
                },
                Some(Commands::Metrics { command }) => run_metrics_command(&state, command),
                Some(Commands::Config { .. }) => unreachable!(),
            }
        }
    }
}

fn run_ask(
    state: &StateStore,
    runtime: &RuntimeAgent,
    instruction_memory: &InstructionMemory,
    profile_name: &str,
    context_window: usize,
    cancellation: &CancellationToken,
    args: AskArgs,
) -> Result<()> {
    let session_id = resolve_or_create_session(state, args.session.as_deref(), "ask")?;
    let context_messages =
        load_bounded_context(state, &session_id, context_window, instruction_memory)?;

    let runtime_context = RuntimeExecutionContext::new(
        RuntimeOperation::Ask,
        profile_name,
        Some(session_id.clone()),
        context_messages,
    );

    state.add_message(&session_id, "user", &args.prompt)?;

    cancellation.clear();
    let request_started = Instant::now();
    match runtime.respond_with_context(&runtime_context, &args.prompt, cancellation) {
        Ok(response) => {
            state.add_message(&session_id, "assistant", &response.text)?;
            record_response_metric(
                state,
                "ask",
                runtime.provider_name(),
                runtime.provider_model(),
                response.duration_ms,
                true,
                None,
            );
            render_ask_output(args.output, &session_id, profile_name, runtime, &response)?;
            Ok(())
        }
        Err(err) => {
            let kind = derive_error_kind(&err);
            record_response_metric(
                state,
                "ask",
                runtime.provider_name(),
                runtime.provider_model(),
                request_started.elapsed().as_millis(),
                false,
                Some(kind.as_str()),
            );
            render_ask_error_output(args.output, &session_id, profile_name, runtime, &kind, &err)?;
            Err(err)
        }
    }
}

fn run_goal(
    state: &StateStore,
    runtime: &RuntimeAgent,
    instruction_memory: &InstructionMemory,
    profile_name: &str,
    context_window: usize,
    cancellation: &CancellationToken,
    args: RunArgs,
) -> Result<()> {
    let session_id = resolve_or_create_session(state, args.session.as_deref(), "run")?;
    let context_messages =
        load_bounded_context(state, &session_id, context_window, instruction_memory)?;
    let runtime_context = RuntimeExecutionContext::new(
        RuntimeOperation::Run,
        profile_name,
        Some(session_id.clone()),
        context_messages,
    );

    state.add_message(&session_id, "user", &args.goal)?;

    cancellation.clear();
    let request_started = Instant::now();
    match runtime.run_goal_with_context(&runtime_context, &args.goal, cancellation) {
        Ok(response) => {
            let run_id = state.record_run(&args.goal, &response.text, "ok")?;
            state.add_message(&session_id, "assistant", &response.text)?;
            record_response_metric(
                state,
                "run",
                runtime.provider_name(),
                runtime.provider_model(),
                response.duration_ms,
                true,
                None,
            );
            render_run_output(
                args.output,
                &run_id,
                &session_id,
                profile_name,
                runtime,
                &response,
            )?;
            Ok(())
        }
        Err(err) => {
            let run_id = state.record_run(&args.goal, &err.to_string(), "error")?;
            let kind = derive_error_kind(&err);
            record_response_metric(
                state,
                "run",
                runtime.provider_name(),
                runtime.provider_model(),
                request_started.elapsed().as_millis(),
                false,
                Some(kind.as_str()),
            );
            render_run_error_output(
                args.output,
                &run_id,
                &session_id,
                profile_name,
                runtime,
                &kind,
                &err,
            )?;
            Err(err)
        }
    }
}

#[derive(Debug, Serialize)]
struct AskJsonOutput<'a> {
    session_id: &'a str,
    profile: &'a str,
    provider: &'a str,
    model: &'a str,
    duration_ms: u128,
    status: &'a str,
    response: &'a str,
}

#[derive(Debug, Serialize)]
struct AskJsonErrorOutput<'a> {
    session_id: &'a str,
    profile: &'a str,
    provider: &'a str,
    model: &'a str,
    status: &'a str,
    error_kind: &'a str,
    error: &'a str,
}

fn render_ask_output(
    output: ResponseOutput,
    session_id: &str,
    profile_name: &str,
    runtime: &RuntimeAgent,
    response: &crate::runtime::RuntimeResponse,
) -> Result<()> {
    match output {
        ResponseOutput::Text => {
            println!("{}", response.text);
            println!(
                "[meta] session={} profile={} provider={}:{} duration_ms={}",
                session_id,
                profile_name,
                runtime.provider_name(),
                runtime.provider_model(),
                response.duration_ms
            );
        }
        ResponseOutput::Json => {
            let payload = AskJsonOutput {
                session_id,
                profile: profile_name,
                provider: runtime.provider_name(),
                model: runtime.provider_model(),
                duration_ms: response.duration_ms,
                status: "ok",
                response: &response.text,
            };
            println!(
                "{}",
                serde_json::to_string(&payload).context("failed to serialize ask response json")?
            );
        }
    }

    Ok(())
}

fn render_ask_error_output(
    output: ResponseOutput,
    session_id: &str,
    profile_name: &str,
    runtime: &RuntimeAgent,
    error_kind: &str,
    err: &anyhow::Error,
) -> Result<()> {
    if !matches!(output, ResponseOutput::Json) {
        return Ok(());
    }

    let payload = AskJsonErrorOutput {
        session_id,
        profile: profile_name,
        provider: runtime.provider_name(),
        model: runtime.provider_model(),
        status: "error",
        error_kind,
        error: &err.to_string(),
    };
    println!(
        "{}",
        serde_json::to_string(&payload).context("failed to serialize ask error json")?
    );
    Ok(())
}

#[derive(Debug, Serialize)]
struct RunJsonOutput<'a> {
    run_id: &'a str,
    session_id: &'a str,
    profile: &'a str,
    provider: &'a str,
    model: &'a str,
    started_at: &'a str,
    finished_at: &'a str,
    duration_ms: u128,
    status: &'a str,
    response: &'a str,
}

#[derive(Debug, Serialize)]
struct RunJsonErrorOutput<'a> {
    run_id: &'a str,
    session_id: &'a str,
    profile: &'a str,
    provider: &'a str,
    model: &'a str,
    status: &'a str,
    error_kind: &'a str,
    error: &'a str,
}

fn render_run_output(
    output: ResponseOutput,
    run_id: &str,
    session_id: &str,
    profile_name: &str,
    runtime: &RuntimeAgent,
    response: &crate::runtime::RuntimeResponse,
) -> Result<()> {
    match output {
        ResponseOutput::Text => {
            println!("run_id      : {run_id}");
            println!("session_id  : {session_id}");
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
        }
        ResponseOutput::Json => {
            let payload = RunJsonOutput {
                run_id,
                session_id,
                profile: profile_name,
                provider: runtime.provider_name(),
                model: runtime.provider_model(),
                started_at: &response.started_at,
                finished_at: &response.finished_at,
                duration_ms: response.duration_ms,
                status: "ok",
                response: &response.text,
            };
            println!(
                "{}",
                serde_json::to_string(&payload).context("failed to serialize run response json")?
            );
        }
    }

    Ok(())
}

fn render_run_error_output(
    output: ResponseOutput,
    run_id: &str,
    session_id: &str,
    profile_name: &str,
    runtime: &RuntimeAgent,
    error_kind: &str,
    err: &anyhow::Error,
) -> Result<()> {
    if !matches!(output, ResponseOutput::Json) {
        return Ok(());
    }

    let payload = RunJsonErrorOutput {
        run_id,
        session_id,
        profile: profile_name,
        provider: runtime.provider_name(),
        model: runtime.provider_model(),
        status: "error",
        error_kind,
        error: &err.to_string(),
    };
    println!(
        "{}",
        serde_json::to_string(&payload).context("failed to serialize run error json")?
    );
    Ok(())
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
        McpCommand::Serve(args) => mcp::serve_stdio(
            args,
            |tool_args| execute_tool_with_policy(state, policy, tools, tool_args),
            || tools.list(),
        ),
    }
}

fn run_metrics_command(state: &StateStore, command: MetricsCommand) -> Result<()> {
    match command {
        MetricsCommand::Summary(args) => {
            let summary = state.telemetry_summary(args.days)?;
            println!("window_days       : {}", summary.window_days);
            println!("from              : {}", summary.from);
            println!("to                : {}", summary.to);
            println!("total_events      : {}", summary.total_events);
            println!();
            println!(
                "startup_latency_ms: {}",
                render_latency_summary(&summary.startup)
            );
            println!(
                "response_latency_ms: {}",
                render_latency_summary(&summary.response)
            );
            println!("response_failures : {}", summary.response_failures);
            println!();
            if summary.error_categories.is_empty() {
                println!("error_categories  : none");
            } else {
                println!("error_categories:");
                for item in summary.error_categories {
                    println!("  - {}: {}", item.kind, item.count);
                }
            }
            Ok(())
        }
        MetricsCommand::Export(args) => {
            let summary = state.telemetry_summary(args.days)?;
            let payload =
                serde_json::to_string_pretty(&summary).context("failed serializing metrics")?;
            if let Some(path) = args.output {
                write_text_file(&path, &payload)?;
                println!("exported metrics summary to {}", path.display());
            } else {
                println!("{payload}");
            }
            Ok(())
        }
    }
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
        SessionCommand::Fork(args) => fork_session(state, args),
        SessionCommand::Continue(_) => {
            bail!("`session continue` must be routed through runtime command handler")
        }
        SessionCommand::Export(args) => export_session(state, args),
        SessionCommand::Import(args) => import_session(state, args),
    }
}

fn resume_session(state: &StateStore, args: SessionResumeArgs) -> Result<()> {
    let session = state.resolve_session_ref(&args.session)?;
    let messages = state.get_messages(&session.id)?;
    if messages.is_empty() {
        println!("no messages for session {}", session.id);
        return Ok(());
    }

    println!("session: {}", session.id);
    if let Some(title) = session.title.as_deref() {
        println!("title  : {title}");
    }
    for (idx, message) in messages.iter().enumerate() {
        println!();
        println!("[{idx}] {} | {}", message.role, message.created_at);
        println!("{}", message.content);
    }

    Ok(())
}

fn export_session(state: &StateStore, args: SessionExportArgs) -> Result<()> {
    validate_session_export_args(&args)?;

    let rendered = if args.all {
        state.export_snapshot_json()?
    } else {
        let session_ref = args
            .session_id
            .as_deref()
            .context("missing session reference; provide SESSION or --all")?;
        let session = state.resolve_session_ref(session_ref)?;
        let messages = state.get_messages(&session.id)?;
        if messages.is_empty() {
            bail!("session {} has no messages", session.id);
        }

        match args.format.unwrap_or(ExportFormat::Json) {
            ExportFormat::Json => serde_json::to_string_pretty(&messages)?,
            ExportFormat::Markdown => render_markdown_session(&session.id, &messages),
        }
    };

    if let Some(path) = args.output {
        write_text_file(&path, &rendered)?;
        if args.all {
            println!("exported backup snapshot to {}", path.display());
        } else {
            println!("exported session to {}", path.display());
        }
    } else {
        println!("{rendered}");
    }

    Ok(())
}

fn import_session(state: &StateStore, args: SessionImportArgs) -> Result<()> {
    let payload = fs::read_to_string(&args.input)
        .with_context(|| format!("failed reading backup file: {}", args.input.display()))?;
    state.import_snapshot_json(&payload)?;
    println!("imported backup snapshot from {}", args.input.display());
    Ok(())
}

fn fork_session(state: &StateStore, args: SessionForkArgs) -> Result<()> {
    let source = state.resolve_session_ref(&args.session)?;
    let fork_id = state.fork_session(&source.id, args.title.as_deref())?;
    println!("forked session: {fork_id}");
    println!("source       : {}", source.id);
    if let Some(title) = args.title {
        println!("title        : {title}");
    }
    Ok(())
}

fn validate_session_export_args(args: &SessionExportArgs) -> Result<()> {
    if args.all {
        if args.session_id.is_some() {
            bail!("--all cannot be combined with SESSION");
        }
        if matches!(args.format, Some(ExportFormat::Markdown)) {
            bail!("--all only supports JSON backup export");
        }
        return Ok(());
    }

    if args.session_id.is_none() {
        bail!("missing session reference; provide SESSION or --all");
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
        ConfigCommand::Setup(args) => run_config_setup(args, path_override),
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

fn command_mode(command: Option<&Commands>) -> &'static str {
    match command {
        None => "tui",
        Some(Commands::Ask(_)) => "ask",
        Some(Commands::Run(_)) => "run",
        Some(Commands::Tool { .. }) => "tool",
        Some(Commands::Mcp { .. }) => "mcp",
        Some(Commands::Session {
            command: SessionCommand::Continue(_),
        }) => "tui_resume",
        Some(Commands::Session { .. }) => "session",
        Some(Commands::Config { .. }) => "config",
        Some(Commands::Metrics { .. }) => "metrics",
    }
}

fn render_latency_summary(summary: &crate::state::TelemetryLatencySummary) -> String {
    format!(
        "count={} avg={} p50={} p95={} min={} max={}",
        summary.count,
        summary.avg_ms,
        summary.p50_ms,
        summary.p95_ms,
        summary.min_ms,
        summary.max_ms
    )
}

fn record_startup_metric(state: &StateStore, mode: &str, latency_ms: u128) {
    if let Err(err) = state.record_startup_latency(mode, latency_ms) {
        eprintln!("[warn] failed to record startup telemetry: {err}");
    }
}

fn record_response_metric(
    state: &StateStore,
    operation: &str,
    provider: &str,
    model: &str,
    latency_ms: u128,
    success: bool,
    error_kind: Option<&str>,
) {
    if let Err(err) =
        state.record_response_latency(operation, provider, model, latency_ms, success, error_kind)
    {
        eprintln!("[warn] failed to record response telemetry: {err}");
    }
}

fn derive_error_kind(err: &anyhow::Error) -> String {
    let text = err.to_string();
    if let Some(idx) = text.find("kind=") {
        let raw = &text[idx + 5..];
        if let Some(kind) = raw
            .split(|c: char| c.is_whitespace() || c == ')' || c == ',')
            .next()
            && !kind.is_empty()
        {
            return kind.to_owned();
        }
    }

    let lowered = text.to_ascii_lowercase();
    if lowered.contains("interrupted") {
        "interrupted".to_owned()
    } else {
        "unknown".to_owned()
    }
}

fn run_config_setup(args: ConfigSetupArgs, path_override: Option<&PathBuf>) -> Result<()> {
    let target = args.output.as_ref().or(path_override);
    let path = config::write_default(target, args.force)?;
    set_default_provider(&path, args.provider)?;

    println!("initialized config: {}", path.display());
    println!("next steps:");
    println!("  1) meow --config {} config validate", path.display());

    match args.provider {
        SetupProvider::Openai => {
            println!("  2) export OPENAI_API_KEY=<your_openai_key>");
        }
        SetupProvider::Anthropic => {
            println!("  2) export ANTHROPIC_API_KEY=<your_anthropic_key>");
        }
        SetupProvider::Ollama => {
            println!("  2) start local model runtime: ollama serve");
        }
    }

    println!("  3) meow --config {} ask \"health check\"", path.display());
    Ok(())
}

fn set_default_provider(path: &Path, provider: SetupProvider) -> Result<()> {
    let provider_name = match provider {
        SetupProvider::Openai => "openai",
        SetupProvider::Anthropic => "anthropic",
        SetupProvider::Ollama => "ollama",
    };

    let path_buf = path.to_path_buf();
    let mut cfg = config::load(Some(&path_buf))?;
    cfg.runtime.default_provider = provider_name.to_owned();

    let content =
        toml::to_string_pretty(&cfg).context("failed to serialize configured setup profile")?;
    fs::write(path, content)
        .with_context(|| format!("failed writing setup config: {}", path.display()))?;
    Ok(())
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
        policy.evaluate_tool(&args.name, &args.args, ToolRegistry::is_risky(&args.name))
    };

    if !decision.is_allowed() {
        state.record_approval(
            &args.name,
            "denied",
            decision.reason_code(),
            &decision.reason,
            false,
        )?;
        bail!("tool execution denied: {}", decision.reason);
    }

    if decision.requires_approval() {
        if !args.approve {
            state.record_approval(
                &args.name,
                "required",
                decision.reason_code(),
                &decision.reason,
                false,
            )?;
            bail!(
                "tool execution requires approval ({}) - re-run with --approve",
                decision.reason
            );
        }
        state.record_approval(
            &args.name,
            "approved",
            decision.reason_code(),
            &decision.reason,
            true,
        )?;
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

fn load_instruction_memory(cfg: &config::MeowConfig) -> Result<InstructionMemory> {
    let cwd = std::env::current_dir().context("failed to determine current working directory")?;
    InstructionMemory::load(cfg, &cwd)
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

fn resolve_or_create_session(
    state: &StateStore,
    session_ref: Option<&str>,
    default_title: &str,
) -> Result<String> {
    if let Some(reference) = session_ref {
        let normalized = reference.trim();
        if normalized.is_empty() {
            bail!("session reference must not be empty");
        }

        let id_match = state.get_session(normalized)?;
        let mut title_matches = state.find_sessions_by_title(normalized)?;

        if let Some(session) = id_match {
            if title_matches
                .iter()
                .any(|title_match| title_match.id != session.id)
            {
                let title_ids = title_matches
                    .iter()
                    .map(|item| item.id.clone())
                    .collect::<Vec<_>>()
                    .join(", ");
                bail!(
                    "session reference `{normalized}` is ambiguous; it matches session id `{}` and title matches [{title_ids}]",
                    session.id
                );
            }
            return Ok(session.id);
        }

        match title_matches.len() {
            0 => state.create_session(Some(normalized)),
            1 => Ok(title_matches.remove(0).id),
            _ => {
                let ids = title_matches
                    .iter()
                    .map(|session| session.id.clone())
                    .collect::<Vec<_>>()
                    .join(", ");
                bail!(
                    "session reference `{normalized}` is ambiguous; multiple sessions share this title: {ids}"
                );
            }
        }
    } else {
        state.create_session(Some(default_title))
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{StateSnapshot, TelemetrySummary};
    use serde_json::Value;
    use uuid::Uuid;

    fn temp_db_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}.db", Uuid::new_v4()))
    }

    fn temp_output_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}.tmp", Uuid::new_v4()))
    }

    #[test]
    fn validate_export_args_rejects_markdown_for_all() {
        let args = SessionExportArgs {
            session_id: None,
            all: true,
            format: Some(ExportFormat::Markdown),
            output: None,
        };

        let err = validate_session_export_args(&args).expect_err("validation should fail");
        assert!(err.to_string().contains("only supports JSON"));
    }

    #[test]
    fn export_session_markdown_writes_markdown_for_single_session() {
        let db_path = temp_db_path("meow-app-export-md");
        let out_path = temp_output_path("meow-app-export-md");
        let state = StateStore::open(&db_path).expect("state store should open");

        let session_id = state
            .create_session(Some("md"))
            .expect("session should be created");
        state
            .add_message(&session_id, "user", "hello markdown")
            .expect("message should be added");

        export_session(
            &state,
            SessionExportArgs {
                session_id: Some(session_id.clone()),
                all: false,
                format: Some(ExportFormat::Markdown),
                output: Some(out_path.clone()),
            },
        )
        .expect("export should succeed");

        let content = fs::read_to_string(&out_path).expect("exported file should be readable");
        assert!(content.contains(&format!("# Session {session_id}")));
        assert!(content.contains("hello markdown"));
    }

    #[test]
    fn export_session_all_writes_snapshot_json() {
        let db_path = temp_db_path("meow-app-export-all");
        let out_path = temp_output_path("meow-app-export-all");
        let state = StateStore::open(&db_path).expect("state store should open");

        let session_id = state
            .create_session(Some("all"))
            .expect("session should be created");
        state
            .add_message(&session_id, "user", "hello backup")
            .expect("message should be added");

        export_session(
            &state,
            SessionExportArgs {
                session_id: None,
                all: true,
                format: None,
                output: Some(out_path.clone()),
            },
        )
        .expect("export should succeed");

        let payload = fs::read_to_string(&out_path).expect("snapshot should be readable");
        let snapshot: StateSnapshot =
            serde_json::from_str(&payload).expect("snapshot json should parse");
        assert!(!snapshot.sessions.is_empty());
        assert!(!snapshot.messages.is_empty());
    }

    #[test]
    fn export_session_accepts_title_reference() {
        let db_path = temp_db_path("meow-app-export-title-ref");
        let out_path = temp_output_path("meow-app-export-title-ref");
        let state = StateStore::open(&db_path).expect("state store should open");

        let session_id = state
            .create_session(Some("named-session"))
            .expect("session should be created");
        state
            .add_message(&session_id, "user", "hello title export")
            .expect("message should be added");

        export_session(
            &state,
            SessionExportArgs {
                session_id: Some("named-session".to_owned()),
                all: false,
                format: Some(ExportFormat::Markdown),
                output: Some(out_path.clone()),
            },
        )
        .expect("export by title should succeed");

        let content = fs::read_to_string(&out_path).expect("exported file should be readable");
        assert!(content.contains("hello title export"));
    }

    #[test]
    fn fork_session_clones_message_history() {
        let db_path = temp_db_path("meow-app-fork-session");
        let state = StateStore::open(&db_path).expect("state store should open");

        let source_id = state
            .create_session(Some("fork-source"))
            .expect("session should be created");
        state
            .add_message(&source_id, "user", "hello fork")
            .expect("message should be added");

        fork_session(
            &state,
            SessionForkArgs {
                session: "fork-source".to_owned(),
                title: Some("fork-branch".to_owned()),
            },
        )
        .expect("fork command should succeed");

        let branch = state
            .find_sessions_by_title("fork-branch")
            .expect("fork sessions should query");
        assert_eq!(branch.len(), 1);

        let fork_messages = state
            .get_messages(&branch[0].id)
            .expect("fork messages should load");
        assert_eq!(fork_messages.len(), 1);
        assert_eq!(fork_messages[0].content, "hello fork");
    }

    #[test]
    fn import_session_restores_from_json_file() {
        let source_db = temp_db_path("meow-app-import-src");
        let target_db = temp_db_path("meow-app-import-dst");
        let backup_path = temp_output_path("meow-app-import-json");

        let source = StateStore::open(&source_db).expect("source state should open");
        let session_id = source
            .create_session(Some("src"))
            .expect("session should be created");
        source
            .add_message(&session_id, "user", "snapshot body")
            .expect("message should be added");

        let payload = source
            .export_snapshot_json()
            .expect("snapshot should export to json");
        fs::write(&backup_path, payload).expect("backup file should be written");

        let target = StateStore::open(&target_db).expect("target state should open");
        import_session(
            &target,
            SessionImportArgs {
                input: backup_path.clone(),
            },
        )
        .expect("import should succeed");

        let sessions = target.list_sessions().expect("sessions should load");
        assert_eq!(sessions.len(), 1);
        let messages = target
            .get_messages(&sessions[0].id)
            .expect("messages should load");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "snapshot body");
    }

    #[test]
    fn config_setup_writes_selected_provider() {
        let output = temp_output_path("meow-config-setup");
        run_config_setup(
            ConfigSetupArgs {
                output: Some(output.clone()),
                force: true,
                provider: SetupProvider::Openai,
            },
            None,
        )
        .expect("config setup should succeed");

        let cfg = config::load(Some(&output)).expect("setup config should load");
        assert_eq!(cfg.runtime.default_provider, "openai");
    }

    #[test]
    fn config_setup_without_force_fails_if_config_exists() {
        let output = temp_output_path("meow-config-setup-existing");
        run_config_setup(
            ConfigSetupArgs {
                output: Some(output.clone()),
                force: true,
                provider: SetupProvider::Openai,
            },
            None,
        )
        .expect("initial setup should succeed");

        let err = run_config_setup(
            ConfigSetupArgs {
                output: Some(output),
                force: false,
                provider: SetupProvider::Anthropic,
            },
            None,
        )
        .expect_err("setup should fail when file already exists");

        assert!(err.to_string().contains("config already exists"));
    }

    #[test]
    fn metrics_export_writes_json_summary() {
        let db_path = temp_db_path("meow-app-metrics-export");
        let out_path = temp_output_path("meow-app-metrics-export");
        let state = StateStore::open(&db_path).expect("state store should open");
        state
            .record_startup_latency("ask", 140)
            .expect("startup telemetry should be recorded");
        state
            .record_response_latency("ask", "openai", "gpt-4.1", 520, false, Some("timeout"))
            .expect("response telemetry should be recorded");

        run_metrics_command(
            &state,
            MetricsCommand::Export(crate::cli::MetricsExportArgs {
                days: 7,
                output: Some(out_path.clone()),
            }),
        )
        .expect("metrics export should succeed");

        let payload = fs::read_to_string(&out_path).expect("metrics export file should exist");
        let summary: TelemetrySummary =
            serde_json::from_str(&payload).expect("metrics export should be valid JSON");
        assert_eq!(summary.window_days, 7);
        assert_eq!(summary.total_events, 2);
        assert_eq!(summary.response_failures, 1);
        assert_eq!(summary.error_categories.len(), 1);
        assert_eq!(summary.error_categories[0].kind, "timeout");
    }

    #[test]
    fn command_mode_maps_session_continue_to_tui_resume() {
        let command = Commands::Session {
            command: SessionCommand::Continue(crate::cli::SessionContinueArgs {
                session: "daily".to_owned(),
            }),
        };
        assert_eq!(command_mode(Some(&command)), "tui_resume");
    }

    #[test]
    fn ask_json_error_payload_shape_is_stable() {
        let payload = AskJsonErrorOutput {
            session_id: "session-1",
            profile: "default",
            provider: "openai",
            model: "gpt-4.1",
            status: "error",
            error_kind: "timeout",
            error: "request timed out",
        };

        let raw = serde_json::to_string(&payload).expect("payload should serialize");
        let value: Value = serde_json::from_str(&raw).expect("payload should parse");
        assert_eq!(value["status"], "error");
        assert_eq!(value["error_kind"], "timeout");
        assert_eq!(value["session_id"], "session-1");
        assert_eq!(value["provider"], "openai");
    }

    #[test]
    fn run_json_error_payload_shape_is_stable() {
        let payload = RunJsonErrorOutput {
            run_id: "run-1",
            session_id: "session-1",
            profile: "default",
            provider: "openai",
            model: "gpt-4.1",
            status: "error",
            error_kind: "auth",
            error: "unauthorized",
        };

        let raw = serde_json::to_string(&payload).expect("payload should serialize");
        let value: Value = serde_json::from_str(&raw).expect("payload should parse");
        assert_eq!(value["status"], "error");
        assert_eq!(value["error_kind"], "auth");
        assert_eq!(value["run_id"], "run-1");
        assert_eq!(value["session_id"], "session-1");
    }

    #[test]
    fn resolve_or_create_session_creates_new_session_for_unknown_reference() {
        let db_path = temp_db_path("meow-app-create-session-ref");
        let state = StateStore::open(&db_path).expect("state store should open");

        let session_id = resolve_or_create_session(&state, Some("phase4-demo"), "ask")
            .expect("unknown reference should create a new session");

        let stored = state
            .get_session(&session_id)
            .expect("session query should succeed")
            .expect("created session should exist");
        assert_eq!(stored.title.as_deref(), Some("phase4-demo"));
    }
}
