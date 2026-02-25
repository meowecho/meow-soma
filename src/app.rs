use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::cli::{
    ChatArgs, Cli, Commands, ConfigCommand, ExportFormat, McpCommand, McpServeArgs, SessionCommand,
    SessionExportArgs, SessionResumeArgs, ToolCommand, ToolExecArgs,
};
use crate::config;
use crate::policy::PolicyEngine;
use crate::providers::build_provider;
use crate::runtime::RuntimeAgent;
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

            match command {
                Commands::Chat(args) => run_chat(&state, &runtime, args),
                Commands::Ask(args) => run_ask(&state, &runtime, &args.prompt),
                Commands::Run(args) => run_goal(&state, &runtime, &args.goal),
                Commands::Tool { command } => run_tool_command(&state, &policy, &tools, command),
                Commands::Mcp { command } => run_mcp_command(&state, &policy, &tools, command),
                Commands::Session { command } => run_session_command(&state, command),
                Commands::Config { .. } => unreachable!(),
            }
        }
    }
}

fn run_chat(state: &StateStore, runtime: &RuntimeAgent, args: ChatArgs) -> Result<()> {
    let session_id = state.create_session(args.title.as_deref())?;
    println!(
        "meow chat started (session: {session_id}, provider: {}:{}). Type /exit to quit.",
        runtime.provider_name(),
        runtime.provider_model()
    );

    let stdin = io::stdin();
    loop {
        print!("you> ");
        io::stdout().flush()?;

        let mut input = String::new();
        let read = stdin.read_line(&mut input)?;
        if read == 0 {
            println!();
            break;
        }

        let prompt = input.trim();
        if prompt.is_empty() {
            continue;
        }

        if matches!(prompt, "/exit" | "/quit") {
            break;
        }

        state.add_message(&session_id, "user", prompt)?;
        let reply = runtime.respond(prompt)?;
        println!("meow> {reply}");
        state.add_message(&session_id, "assistant", &reply)?;
    }

    Ok(())
}

fn run_ask(state: &StateStore, runtime: &RuntimeAgent, prompt: &str) -> Result<()> {
    let session_id = state.create_session(Some("ask"))?;
    state.add_message(&session_id, "user", prompt)?;

    let reply = runtime.respond(prompt)?;
    println!("{reply}");

    state.add_message(&session_id, "assistant", &reply)?;
    Ok(())
}

fn run_goal(state: &StateStore, runtime: &RuntimeAgent, goal: &str) -> Result<()> {
    let output = runtime.run_goal(goal)?;
    let run_id = state.record_run(goal, &output, "ok")?;

    println!("run_id: {run_id}");
    println!("{output}");
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

            for session in sessions {
                println!(
                    "{}\t{}\t{}",
                    session.id,
                    session.title.unwrap_or_else(|| "(untitled)".to_owned()),
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

    for message in messages {
        println!(
            "[{}] {}: {}",
            message.created_at, message.role, message.content
        );
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
