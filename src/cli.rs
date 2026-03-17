use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "meow", version, about = "Meow-Soma unified AI CLI")]
pub struct Cli {
    #[arg(
        global = true,
        long,
        short = 'c',
        value_name = "PATH",
        help = "Path to meow runtime config (defaults to ~/.meow-soma/config.toml)"
    )]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Ask one question and print one response.
    Ask(AskArgs),
    /// Execute a high-level goal using the single-agent runtime loop.
    Run(RunArgs),
    /// Inspect and execute runtime tools.
    Tool {
        #[command(subcommand)]
        command: ToolCommand,
    },
    /// Serve tool capabilities over MCP transport.
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
    /// Inspect or export persisted sessions.
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },
    /// Manage meow runtime configuration.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Inspect and export runtime telemetry metrics.
    Metrics {
        #[command(subcommand)]
        command: MetricsCommand,
    },
}

#[derive(Debug, Args)]
pub struct AskArgs {
    #[arg(value_name = "PROMPT")]
    pub prompt: String,

    #[arg(
        long,
        short = 's',
        value_name = "SESSION",
        help = "Continue an existing session by id or exact title"
    )]
    pub session: Option<String>,

    #[arg(long, value_enum, default_value = "text")]
    pub output: ResponseOutput,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    #[arg(value_name = "GOAL")]
    pub goal: String,

    #[arg(
        long,
        short = 's',
        value_name = "SESSION",
        help = "Continue an existing session by id or exact title"
    )]
    pub session: Option<String>,

    #[arg(long, value_enum, default_value = "text")]
    pub output: ResponseOutput,
}

#[derive(Debug, Subcommand)]
pub enum ToolCommand {
    /// List tools available to the runtime.
    List,
    /// Execute a tool by name.
    Exec(ToolExecArgs),
}

#[derive(Debug, Args)]
pub struct ToolExecArgs {
    #[arg(value_name = "TOOL")]
    pub name: String,

    #[arg(value_name = "ARGS", trailing_var_arg = true)]
    pub args: Vec<String>,

    #[arg(
        long,
        help = "Approve command execution when the permission gate requires explicit approval"
    )]
    pub approve: bool,
}

#[derive(Debug, Subcommand)]
pub enum McpCommand {
    /// Start MCP server mode.
    Serve(McpServeArgs),
}

#[derive(Debug, Args)]
pub struct McpServeArgs {
    #[arg(long, default_value = "stdio", help = "Transport: stdio")]
    pub transport: String,
}

#[derive(Debug, Subcommand)]
pub enum SessionCommand {
    /// List saved sessions.
    List,
    /// Show messages for one session by id or exact title.
    Resume(SessionResumeArgs),
    /// Open TUI and continue one session by id or exact title.
    Continue(SessionContinueArgs),
    /// Create a branch session by cloning messages from an existing session.
    Fork(SessionForkArgs),
    /// Export one session to JSON or Markdown.
    Export(SessionExportArgs),
    /// Import a full JSON backup snapshot.
    Import(SessionImportArgs),
}

#[derive(Debug, Args)]
pub struct SessionResumeArgs {
    #[arg(value_name = "SESSION")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct SessionContinueArgs {
    #[arg(value_name = "SESSION")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct SessionForkArgs {
    #[arg(value_name = "SESSION")]
    pub session: String,

    #[arg(long, value_name = "TITLE")]
    pub title: Option<String>,
}

#[derive(Debug, Args)]
pub struct SessionExportArgs {
    #[arg(value_name = "SESSION", required_unless_present = "all")]
    pub session_id: Option<String>,

    #[arg(
        long,
        help = "Export all persisted state as one JSON backup snapshot",
        conflicts_with = "session_id"
    )]
    pub all: bool,

    #[arg(long, value_enum, conflicts_with = "all")]
    pub format: Option<ExportFormat>,

    #[arg(long, short = 'o', value_name = "PATH")]
    pub output: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct SessionImportArgs {
    #[arg(value_name = "PATH")]
    pub input: PathBuf,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ExportFormat {
    Json,
    Markdown,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum ResponseOutput {
    Text,
    Json,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Create a default runtime config.
    Init(ConfigInitArgs),
    /// First-run setup helper that initializes config and prints provider-specific next steps.
    Setup(ConfigSetupArgs),
    /// Validate runtime config.
    Validate,
    /// Print the canonical runtime config path.
    Path,
}

#[derive(Debug, Subcommand)]
pub enum MetricsCommand {
    /// Print a summarized telemetry window.
    Summary(MetricsSummaryArgs),
    /// Export telemetry summary as JSON.
    Export(MetricsExportArgs),
}

#[derive(Debug, Args)]
pub struct MetricsSummaryArgs {
    #[arg(long, default_value_t = 7, value_name = "DAYS")]
    pub days: u32,
}

#[derive(Debug, Args)]
pub struct MetricsExportArgs {
    #[arg(long, default_value_t = 7, value_name = "DAYS")]
    pub days: u32,

    #[arg(long, short = 'o', value_name = "PATH")]
    pub output: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ConfigInitArgs {
    #[arg(long, short = 'o', value_name = "PATH")]
    pub output: Option<PathBuf>,

    #[arg(long, help = "Overwrite existing file")]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct ConfigSetupArgs {
    #[arg(long, short = 'o', value_name = "PATH")]
    pub output: Option<PathBuf>,

    #[arg(long, help = "Overwrite existing file")]
    pub force: bool,

    #[arg(long, value_enum, default_value = "ollama")]
    pub provider: SetupProvider,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum SetupProvider {
    Openai,
    Anthropic,
    Ollama,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    #[test]
    fn defaults_to_no_subcommand() {
        let cli = Cli::try_parse_from(["meow"]).expect("parse should succeed");
        assert!(cli.command.is_none());
    }

    #[test]
    fn rejects_removed_tui_subcommand() {
        let err = Cli::try_parse_from(["meow", "tui"]).expect_err("parse should fail");
        assert_eq!(err.kind(), ErrorKind::InvalidSubcommand);
    }

    #[test]
    fn parses_global_config_override() {
        let cli = Cli::try_parse_from(["meow", "--config", "config/dev.local.toml"])
            .expect("parse should succeed");
        assert_eq!(cli.config, Some(PathBuf::from("config/dev.local.toml")));
        assert!(cli.command.is_none());
    }

    #[test]
    fn parses_ask_command() {
        let cli =
            Cli::try_parse_from(["meow", "ask", "hello world"]).expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Ask(args) = command else {
            panic!("expected ask command");
        };

        assert_eq!(args.prompt, "hello world");
        assert!(args.session.is_none());
        assert_eq!(args.output, ResponseOutput::Text);
    }

    #[test]
    fn parses_run_command() {
        let cli = Cli::try_parse_from(["meow", "run", "implement phase 6"])
            .expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Run(args) = command else {
            panic!("expected run command");
        };

        assert_eq!(args.goal, "implement phase 6");
        assert!(args.session.is_none());
        assert_eq!(args.output, ResponseOutput::Text);
    }

    #[test]
    fn parses_ask_command_with_session_and_json_output() {
        let cli = Cli::try_parse_from([
            "meow",
            "ask",
            "hello world",
            "--session",
            "daily",
            "--output",
            "json",
        ])
        .expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Ask(args) = command else {
            panic!("expected ask command");
        };

        assert_eq!(args.session.as_deref(), Some("daily"));
        assert_eq!(args.output, ResponseOutput::Json);
    }

    #[test]
    fn parses_run_command_with_session_and_json_output() {
        let cli = Cli::try_parse_from([
            "meow",
            "run",
            "implement phase 6",
            "--session",
            "work-1",
            "--output",
            "json",
        ])
        .expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Run(args) = command else {
            panic!("expected run command");
        };

        assert_eq!(args.session.as_deref(), Some("work-1"));
        assert_eq!(args.output, ResponseOutput::Json);
    }

    #[test]
    fn parses_tool_list_command() {
        let cli = Cli::try_parse_from(["meow", "tool", "list"]).expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Tool { command } = command else {
            panic!("expected tool command");
        };

        assert!(matches!(command, ToolCommand::List));
    }

    #[test]
    fn parses_tool_exec_command() {
        let cli = Cli::try_parse_from([
            "meow",
            "tool",
            "exec",
            "shell",
            "--approve",
            "echo",
            "hello",
        ])
        .expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Tool { command } = command else {
            panic!("expected tool command");
        };

        let ToolCommand::Exec(args) = command else {
            panic!("expected tool exec command");
        };

        assert_eq!(args.name, "shell");
        assert!(args.approve);
        assert_eq!(args.args, vec!["echo".to_owned(), "hello".to_owned()]);
    }

    #[test]
    fn parses_mcp_serve_command_with_transport() {
        let cli = Cli::try_parse_from(["meow", "mcp", "serve", "--transport", "stdio"])
            .expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Mcp { command } = command else {
            panic!("expected mcp command");
        };

        let McpCommand::Serve(args) = command;

        assert_eq!(args.transport, "stdio");
    }

    #[test]
    fn parses_config_init_command_with_flags() {
        let cli = Cli::try_parse_from([
            "meow",
            "config",
            "init",
            "--output",
            "/tmp/config.toml",
            "--force",
        ])
        .expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Config { command } = command else {
            panic!("expected config command");
        };

        let ConfigCommand::Init(args) = command else {
            panic!("expected config init command");
        };

        assert_eq!(args.output, Some(PathBuf::from("/tmp/config.toml")));
        assert!(args.force);
    }

    #[test]
    fn parses_config_validate_command() {
        let cli =
            Cli::try_parse_from(["meow", "config", "validate"]).expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Config { command } = command else {
            panic!("expected config command");
        };

        assert!(matches!(command, ConfigCommand::Validate));
    }

    #[test]
    fn parses_config_setup_command_with_provider() {
        let cli = Cli::try_parse_from([
            "meow",
            "config",
            "setup",
            "--output",
            "/tmp/config.toml",
            "--provider",
            "openai",
            "--force",
        ])
        .expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Config { command } = command else {
            panic!("expected config command");
        };

        let ConfigCommand::Setup(args) = command else {
            panic!("expected config setup command");
        };

        assert_eq!(args.output, Some(PathBuf::from("/tmp/config.toml")));
        assert!(args.force);
        assert_eq!(args.provider, SetupProvider::Openai);
    }

    #[test]
    fn parses_config_path_command() {
        let cli = Cli::try_parse_from(["meow", "config", "path"]).expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Config { command } = command else {
            panic!("expected config command");
        };

        assert!(matches!(command, ConfigCommand::Path));
    }

    #[test]
    fn parses_metrics_summary_command() {
        let cli = Cli::try_parse_from(["meow", "metrics", "summary", "--days", "14"])
            .expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Metrics { command } = command else {
            panic!("expected metrics command");
        };

        let MetricsCommand::Summary(args) = command else {
            panic!("expected metrics summary command");
        };

        assert_eq!(args.days, 14);
    }

    #[test]
    fn parses_metrics_export_command() {
        let cli = Cli::try_parse_from([
            "meow",
            "metrics",
            "export",
            "--days",
            "30",
            "--output",
            "/tmp/meow-metrics.json",
        ])
        .expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Metrics { command } = command else {
            panic!("expected metrics command");
        };

        let MetricsCommand::Export(args) = command else {
            panic!("expected metrics export command");
        };

        assert_eq!(args.days, 30);
        assert_eq!(args.output, Some(PathBuf::from("/tmp/meow-metrics.json")));
    }

    #[test]
    fn parses_session_export_single_session_defaults_to_json_in_app() {
        let cli = Cli::try_parse_from(["meow", "session", "export", "session-1"])
            .expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Session { command } = command else {
            panic!("expected session command");
        };

        let SessionCommand::Export(args) = command else {
            panic!("expected export command");
        };

        assert_eq!(args.session_id.as_deref(), Some("session-1"));
        assert!(!args.all);
        assert!(args.format.is_none());
    }

    #[test]
    fn parses_session_list_command() {
        let cli = Cli::try_parse_from(["meow", "session", "list"]).expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Session { command } = command else {
            panic!("expected session command");
        };

        assert!(matches!(command, SessionCommand::List));
    }

    #[test]
    fn parses_session_resume_command() {
        let cli = Cli::try_parse_from(["meow", "session", "resume", "session-1"])
            .expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Session { command } = command else {
            panic!("expected session command");
        };

        let SessionCommand::Resume(args) = command else {
            panic!("expected resume command");
        };

        assert_eq!(args.session, "session-1");
    }

    #[test]
    fn parses_session_continue_command() {
        let cli = Cli::try_parse_from(["meow", "session", "continue", "daily"])
            .expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Session { command } = command else {
            panic!("expected session command");
        };

        let SessionCommand::Continue(args) = command else {
            panic!("expected continue command");
        };

        assert_eq!(args.session, "daily");
    }

    #[test]
    fn parses_session_fork_command_with_title() {
        let cli = Cli::try_parse_from([
            "meow",
            "session",
            "fork",
            "session-1",
            "--title",
            "branch-a",
        ])
        .expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Session { command } = command else {
            panic!("expected session command");
        };

        let SessionCommand::Fork(args) = command else {
            panic!("expected fork command");
        };

        assert_eq!(args.session, "session-1");
        assert_eq!(args.title.as_deref(), Some("branch-a"));
    }

    #[test]
    fn parses_session_export_all_backup() {
        let cli = Cli::try_parse_from(["meow", "session", "export", "--all"])
            .expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Session { command } = command else {
            panic!("expected session command");
        };

        let SessionCommand::Export(args) = command else {
            panic!("expected export command");
        };

        assert!(args.all);
        assert!(args.session_id.is_none());
    }

    #[test]
    fn rejects_session_export_without_target_scope() {
        let err =
            Cli::try_parse_from(["meow", "session", "export"]).expect_err("parse should fail");
        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn rejects_session_export_with_all_and_session_id() {
        let err = Cli::try_parse_from(["meow", "session", "export", "session-1", "--all"])
            .expect_err("parse should fail");
        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn rejects_session_export_all_with_markdown_format() {
        let err =
            Cli::try_parse_from(["meow", "session", "export", "--all", "--format", "markdown"])
                .expect_err("parse should fail");
        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn parses_session_import_with_path() {
        let cli = Cli::try_parse_from(["meow", "session", "import", "/tmp/meow-backup.json"])
            .expect("parse should succeed");

        let command = cli.command.expect("command should exist");
        let Commands::Session { command } = command else {
            panic!("expected session command");
        };

        let SessionCommand::Import(args) = command else {
            panic!("expected import command");
        };

        assert_eq!(args.input, PathBuf::from("/tmp/meow-backup.json"));
    }
}
