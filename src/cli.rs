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
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Start an interactive chat session.
    Chat(ChatArgs),
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
}

#[derive(Debug, Args)]
pub struct ChatArgs {
    #[arg(long, help = "Optional title for the new chat session")]
    pub title: Option<String>,
}

#[derive(Debug, Args)]
pub struct AskArgs {
    #[arg(value_name = "PROMPT")]
    pub prompt: String,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    #[arg(value_name = "GOAL")]
    pub goal: String,
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
    /// Show messages for one session.
    Resume(SessionResumeArgs),
    /// Export one session to JSON or Markdown.
    Export(SessionExportArgs),
}

#[derive(Debug, Args)]
pub struct SessionResumeArgs {
    #[arg(value_name = "SESSION_ID")]
    pub session_id: String,
}

#[derive(Debug, Args)]
pub struct SessionExportArgs {
    #[arg(value_name = "SESSION_ID")]
    pub session_id: String,

    #[arg(long, value_enum, default_value = "json")]
    pub format: ExportFormat,

    #[arg(long, short = 'o', value_name = "PATH")]
    pub output: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ExportFormat {
    Json,
    Markdown,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Create a default runtime config.
    Init(ConfigInitArgs),
    /// Validate runtime config.
    Validate,
    /// Print the canonical runtime config path.
    Path,
}

#[derive(Debug, Args)]
pub struct ConfigInitArgs {
    #[arg(long, short = 'o', value_name = "PATH")]
    pub output: Option<PathBuf>,

    #[arg(long, help = "Overwrite existing file")]
    pub force: bool,
}
