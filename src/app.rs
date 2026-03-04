use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result, bail};

use crate::cli::{
    Cli, Commands, ConfigCommand, ConfigSetupArgs, ExportFormat, McpCommand, SessionCommand,
    SessionExportArgs, SessionImportArgs, SessionResumeArgs, SetupProvider, ToolCommand,
    ToolExecArgs,
};
use crate::config;
use crate::mcp;
use crate::policy::PolicyEngine;
use crate::providers::build_provider;
use crate::runtime::{CancellationToken, RuntimeAgent, RuntimeExecutionContext, RuntimeOperation};
use crate::state::StateStore;
use crate::tools::{ToolOutput, ToolRegistry};
use crate::tui;

pub fn run(cli: Cli) -> Result<()> {
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

            match command {
                None => tui::run_tui(
                    &state,
                    &policy,
                    &tools,
                    &runtime,
                    &profile_name,
                    context_window,
                    &cancellation,
                ),
                Some(Commands::Ask(args)) => {
                    run_ask(&state, &runtime, &profile_name, &cancellation, &args.prompt)
                }
                Some(Commands::Run(args)) => {
                    run_goal(&state, &runtime, &profile_name, &cancellation, &args.goal)
                }
                Some(Commands::Tool { command }) => {
                    run_tool_command(&state, &policy, &tools, command)
                }
                Some(Commands::Mcp { command }) => {
                    run_mcp_command(&state, &policy, &tools, command)
                }
                Some(Commands::Session { command }) => run_session_command(&state, command),
                Some(Commands::Config { .. }) => unreachable!(),
            }
        }
    }
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
        McpCommand::Serve(args) => mcp::serve_stdio(
            args,
            |tool_args| execute_tool_with_policy(state, policy, tools, tool_args),
            || tools.list(),
        ),
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
        SessionCommand::Export(args) => export_session(state, args),
        SessionCommand::Import(args) => import_session(state, args),
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
    validate_session_export_args(&args)?;

    let rendered = if args.all {
        state.export_snapshot_json()?
    } else {
        let session_id = args
            .session_id
            .as_deref()
            .context("missing session id; provide SESSION_ID or --all")?;
        let messages = state.get_messages(session_id)?;
        if messages.is_empty() {
            bail!("session {} has no messages", session_id);
        }

        match args.format.unwrap_or(ExportFormat::Json) {
            ExportFormat::Json => serde_json::to_string_pretty(&messages)?,
            ExportFormat::Markdown => render_markdown_session(session_id, &messages),
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

fn validate_session_export_args(args: &SessionExportArgs) -> Result<()> {
    if args.all {
        if args.session_id.is_some() {
            bail!("--all cannot be combined with SESSION_ID");
        }
        if matches!(args.format, Some(ExportFormat::Markdown)) {
            bail!("--all only supports JSON backup export");
        }
        return Ok(());
    }

    if args.session_id.is_none() {
        bail!("missing session id; provide SESSION_ID or --all");
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
        policy.evaluate_tool(&args.name, ToolRegistry::is_risky(&args.name))
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
    use crate::state::StateSnapshot;
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
}
