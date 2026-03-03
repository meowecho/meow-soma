mod app;
mod cli;
mod config;
mod mcp;
mod policy;
mod providers;
mod runtime;
mod state;
mod tools;
mod tui;

use anyhow::Result;
use clap::Parser;

use crate::cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();
    app::run(cli)
}
