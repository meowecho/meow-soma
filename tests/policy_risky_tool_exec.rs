use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn run_meow(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_meow"));
    cmd.args(args);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().expect("meow command should run")
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "meow-policy-{label}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("temp dir should be created");
    root
}

fn inline_array(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| format!("\"{value}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

fn write_policy_config(
    root: &Path,
    policy: &str,
    allowlist: &[&str],
    denylist: &[&str],
) -> PathBuf {
    let root_str = root.to_string_lossy();
    let config_path = root.join("config.toml");
    let content = format!(
        r#"[project]
name = "meow-policy-test"
default_profile = "default"

[runtime]
default_provider = "ollama"
max_steps = 10
concurrency = 1
retry_budget = 1

[security]
approval_policy = "{policy}"
allowlist = [{allowlist}]
denylist = [{denylist}]

[storage]
root_dir = "{root}/state"
sqlite_path = "{root}/state/state.db"
memory_dir = "{root}/state/memory"
artifacts_dir = "{root}/state/artifacts"
logs_dir = "{root}/state/logs"

[mcp]
enabled = true
transport = "stdio"
bind = "127.0.0.1:7777"

[providers.ollama]
model = "llama3.1:8b"
endpoint = "http://127.0.0.1:11434"
timeout_secs = 5

[[profiles]]
name = "default"
model = "llama3.1:8b"
tools = ["shell", "fs.read", "fs.write", "echo"]
temperature = 0.2
"#,
        policy = policy,
        allowlist = inline_array(allowlist),
        denylist = inline_array(denylist),
        root = root_str
    );

    fs::write(&config_path, content).expect("policy config should be written");
    config_path
}

#[test]
fn ask_mode_requires_approval_for_risky_tools_and_executes_after_approval() {
    let root = unique_temp_dir("ask-approval");
    let config_path = write_policy_config(&root, "ask", &["tool:echo"], &[]);
    let target = root.join("sandbox").join("approved.txt");

    let config = config_path.to_string_lossy().into_owned();
    let target_path = target.to_string_lossy().into_owned();
    let write_root = root.to_string_lossy().into_owned();

    let blocked = run_meow(
        &[
            "--config",
            &config,
            "tool",
            "exec",
            "fs.write",
            &target_path,
            "hello",
        ],
        &[("MEOW_FS_WRITE_ALLOW_ROOTS", &write_root)],
    );
    assert!(
        !blocked.status.success(),
        "expected approval-required failure"
    );
    let blocked_stderr = String::from_utf8_lossy(&blocked.stderr);
    assert!(
        blocked_stderr.contains("requires approval"),
        "stderr should mention approval requirement, got: {blocked_stderr}"
    );

    let approved = run_meow(
        &[
            "--config",
            &config,
            "tool",
            "exec",
            "--approve",
            "fs.write",
            &target_path,
            "hello",
        ],
        &[("MEOW_FS_WRITE_ALLOW_ROOTS", &write_root)],
    );
    assert!(approved.status.success(), "approved execution should pass");

    let file_content =
        fs::read_to_string(&target).expect("approved command should create the output file");
    assert_eq!(file_content, "hello");
}

#[test]
fn deny_mode_blocks_risky_tools_even_when_approve_flag_is_present() {
    let root = unique_temp_dir("deny-risky");
    let config_path = write_policy_config(&root, "deny", &["tool:fs.write"], &[]);
    let target = root.join("sandbox").join("blocked.txt");

    let config = config_path.to_string_lossy().into_owned();
    let target_path = target.to_string_lossy().into_owned();
    let write_root = root.to_string_lossy().into_owned();

    let output = run_meow(
        &[
            "--config",
            &config,
            "tool",
            "exec",
            "--approve",
            "fs.write",
            &target_path,
            "blocked",
        ],
        &[("MEOW_FS_WRITE_ALLOW_ROOTS", &write_root)],
    );
    assert!(
        !output.status.success(),
        "deny mode should reject risky tools"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("tool execution denied"),
        "stderr should mention policy denial, got: {stderr}"
    );
}

#[test]
fn tool_specifier_denylist_rule_is_enforced_before_approval_flow() {
    let root = unique_temp_dir("specifier-deny");
    let protected = root.join("protected");
    fs::create_dir_all(&protected).expect("protected dir should be created");
    let protected_prefix = protected.to_string_lossy().replace('\\', "/");

    let deny_rule = format!("tool:fs.write {protected_prefix}");
    let config_path = write_policy_config(&root, "ask", &["tool:echo"], &[&deny_rule]);

    let config = config_path.to_string_lossy().into_owned();
    let write_root = root.to_string_lossy().into_owned();
    let blocked_target = protected.join("secret.txt").to_string_lossy().into_owned();

    let blocked = run_meow(
        &[
            "--config",
            &config,
            "tool",
            "exec",
            "--approve",
            "fs.write",
            &blocked_target,
            "secret",
        ],
        &[("MEOW_FS_WRITE_ALLOW_ROOTS", &write_root)],
    );

    assert!(
        !blocked.status.success(),
        "denylist specifier should block execution even with --approve"
    );
    let stderr = String::from_utf8_lossy(&blocked.stderr);
    assert!(
        stderr.contains("tool execution denied"),
        "stderr should mention denial, got: {stderr}"
    );
}
