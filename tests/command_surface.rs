use std::path::PathBuf;
use std::process::{Command, Output};

fn run_meow(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_meow"))
        .args(args)
        .output()
        .expect("meow command should run")
}

fn fixture_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(relative)
}

#[test]
fn root_help_lists_primary_commands() {
    let output = run_meow(&["--help"]);
    assert!(output.status.success(), "expected success");

    let stdout = String::from_utf8_lossy(&output.stdout);
    for command in ["ask", "run", "tool", "mcp", "session", "config"] {
        assert!(
            stdout.contains(command),
            "root help should list `{command}`"
        );
    }
}

#[test]
fn default_command_path_loads_config_before_tui() {
    let invalid_config = fixture_path("config/invalid_syntax.toml");
    let invalid_config = invalid_config.to_string_lossy().into_owned();
    let output = run_meow(&["--config", &invalid_config]);

    assert!(!output.status.success(), "expected failure");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed parsing TOML config"),
        "stderr should report TOML parsing failure, got: {stderr}"
    );
}

#[test]
fn ask_help_describes_prompt_argument() {
    let output = run_meow(&["ask", "--help"]);
    assert!(output.status.success(), "expected success");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PROMPT"), "help should mention PROMPT");
}

#[test]
fn run_help_describes_goal_argument() {
    let output = run_meow(&["run", "--help"]);
    assert!(output.status.success(), "expected success");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("GOAL"), "help should mention GOAL");
}

#[test]
fn tool_help_lists_subcommands() {
    let output = run_meow(&["tool", "--help"]);
    assert!(output.status.success(), "expected success");

    let stdout = String::from_utf8_lossy(&output.stdout);
    for command in ["list", "exec"] {
        assert!(
            stdout.contains(command),
            "tool help should list `{command}`"
        );
    }
}

#[test]
fn mcp_help_lists_serve_subcommand() {
    let output = run_meow(&["mcp", "--help"]);
    assert!(output.status.success(), "expected success");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("serve"), "mcp help should list `serve`");
}

#[test]
fn session_help_lists_subcommands() {
    let output = run_meow(&["session", "--help"]);
    assert!(output.status.success(), "expected success");

    let stdout = String::from_utf8_lossy(&output.stdout);
    for command in ["list", "resume", "export", "import"] {
        assert!(
            stdout.contains(command),
            "session help should list `{command}`"
        );
    }
}

#[test]
fn config_help_lists_subcommands() {
    let output = run_meow(&["config", "--help"]);
    assert!(output.status.success(), "expected success");

    let stdout = String::from_utf8_lossy(&output.stdout);
    for command in ["init", "validate", "path"] {
        assert!(
            stdout.contains(command),
            "config help should list `{command}`"
        );
    }
}
