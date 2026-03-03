use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub risky: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolOutput {
    pub status: String,
    pub stdout: String,
    pub stderr: String,
}

pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    fn execute(&self, args: &[String]) -> Result<ToolOutput>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
    workspace_root: PathBuf,
    approved_write_roots: Vec<PathBuf>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let workspace_root = env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from("."));
        let approved_write_roots = load_approved_write_roots(&workspace_root);
        Self::new_with_scope(workspace_root, approved_write_roots)
    }

    fn new_with_scope(workspace_root: PathBuf, approved_write_roots: Vec<PathBuf>) -> Self {
        let mut tools: HashMap<String, Box<dyn Tool>> = HashMap::new();
        register(&mut tools, EchoTool);
        register(&mut tools, ShellTool);
        register(&mut tools, FsReadTool);
        register(&mut tools, FsWriteTool);
        Self {
            tools,
            workspace_root: normalize_path(&workspace_root),
            approved_write_roots: approved_write_roots
                .into_iter()
                .map(|path| normalize_root_path(&path, &workspace_root))
                .collect(),
        }
    }

    pub fn list(&self) -> Vec<ToolSpec> {
        let mut specs: Vec<ToolSpec> = self.tools.values().map(|tool| tool.spec()).collect();
        specs.sort_by(|a, b| a.name.cmp(&b.name));
        specs
    }

    pub fn execute(&self, name: &str, args: &[String]) -> Result<ToolOutput> {
        let Some(tool) = self.tools.get(name) else {
            bail!("unknown tool: {name}");
        };

        let prepared_args = if name == "fs.write" {
            self.prepare_fs_write_args(args)?
        } else {
            args.to_vec()
        };

        tool.execute(&prepared_args)
    }

    pub fn is_known(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    pub fn is_risky(name: &str) -> bool {
        matches!(name, "shell" | "fs.write")
    }

    fn prepare_fs_write_args(&self, args: &[String]) -> Result<Vec<String>> {
        if args.len() < 2 {
            bail!("fs.write expects path and content");
        }

        let resolved =
            resolve_fs_write_path(&args[0], &self.workspace_root, &self.approved_write_roots)?;

        let mut prepared = args.to_vec();
        prepared[0] = resolved.display().to_string();
        Ok(prepared)
    }
}

fn register<T: Tool + 'static>(map: &mut HashMap<String, Box<dyn Tool>>, tool: T) {
    let name = tool.spec().name;
    map.insert(name, Box::new(tool));
}

fn load_approved_write_roots(workspace_root: &Path) -> Vec<PathBuf> {
    let Some(raw) = env::var_os("MEOW_FS_WRITE_ALLOW_ROOTS") else {
        return Vec::new();
    };

    env::split_paths(&raw)
        .map(|path| normalize_root_path(&path, workspace_root))
        .collect()
}

fn normalize_root_path(path: &Path, workspace_root: &Path) -> PathBuf {
    if path.is_absolute() {
        normalize_path(path)
    } else {
        normalize_path(&workspace_root.join(path))
    }
}

fn resolve_fs_write_path(
    raw_path: &str,
    workspace_root: &Path,
    approved_roots: &[PathBuf],
) -> Result<PathBuf> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        bail!("fs.write path must not be empty");
    }

    let candidate = PathBuf::from(trimmed);
    let normalized = if candidate.is_absolute() {
        normalize_path(&candidate)
    } else {
        normalize_path(&workspace_root.join(candidate))
    };

    let in_workspace = is_within_root(&normalized, workspace_root);
    let in_approved = approved_roots
        .iter()
        .any(|root| is_within_root(&normalized, root));

    if !(in_workspace || in_approved) {
        bail!(
            "fs.write target '{}' is outside workspace '{}' and approved roots",
            normalized.display(),
            workspace_root.display()
        );
    }

    Ok(normalized)
}

fn is_within_root(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

struct EchoTool;

impl Tool for EchoTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "echo".to_owned(),
            description: "Return arguments as plain output".to_owned(),
            risky: false,
        }
    }

    fn execute(&self, args: &[String]) -> Result<ToolOutput> {
        Ok(ToolOutput {
            status: "ok".to_owned(),
            stdout: args.join(" "),
            stderr: String::new(),
        })
    }
}

struct ShellTool;

impl Tool for ShellTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "shell".to_owned(),
            description: "Execute a shell command via zsh -lc".to_owned(),
            risky: true,
        }
    }

    fn execute(&self, args: &[String]) -> Result<ToolOutput> {
        if args.is_empty() {
            bail!("shell tool expects a command");
        }

        let command = args.join(" ");
        let output = Command::new("zsh").arg("-lc").arg(&command).output()?;

        let status = if output.status.success() {
            "ok"
        } else {
            "error"
        };

        Ok(ToolOutput {
            status: status.to_owned(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

struct FsReadTool;

impl Tool for FsReadTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "fs.read".to_owned(),
            description: "Read one file from the local filesystem".to_owned(),
            risky: false,
        }
    }

    fn execute(&self, args: &[String]) -> Result<ToolOutput> {
        let Some(path) = args.first() else {
            bail!("fs.read expects a file path argument");
        };

        let contents = fs::read_to_string(path)?;
        Ok(ToolOutput {
            status: "ok".to_owned(),
            stdout: contents,
            stderr: String::new(),
        })
    }
}

struct FsWriteTool;

impl Tool for FsWriteTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "fs.write".to_owned(),
            description: "Write text to a local file (arg1=path, arg2+=content)".to_owned(),
            risky: true,
        }
    }

    fn execute(&self, args: &[String]) -> Result<ToolOutput> {
        if args.len() < 2 {
            bail!("fs.write expects path and content");
        }

        let path = &args[0];
        let content = args[1..].join(" ");

        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create parent directory: {}", parent.display())
            })?;
        }

        fs::write(path, content)?;

        Ok(ToolOutput {
            status: "ok".to_owned(),
            stdout: format!("wrote {}", path),
            stderr: String::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Mutex;

    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn temp_root(name: &str) -> PathBuf {
        let path = env::temp_dir().join(format!("meow-tools-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).expect("temp root should be created");
        path
    }

    #[test]
    fn fs_write_allows_workspace_relative_path() {
        let workspace = temp_root("workspace-write-ok");
        let registry = ToolRegistry::new_with_scope(workspace.clone(), Vec::new());
        let target = "tmp/allowed.txt".to_owned();

        let output = registry
            .execute("fs.write", &[target, "hello".to_owned()])
            .expect("fs.write should succeed in workspace");

        assert_eq!(output.status, "ok");
        let expected_path = workspace.join("tmp/allowed.txt");
        let content = fs::read_to_string(expected_path).expect("written file should exist");
        assert_eq!(content, "hello");
    }

    #[test]
    fn fs_write_denies_escape_outside_workspace() {
        let workspace = temp_root("workspace-write-deny");
        let registry = ToolRegistry::new_with_scope(workspace, Vec::new());
        let err = registry
            .execute(
                "fs.write",
                &["../outside.txt".to_owned(), "nope".to_owned()],
            )
            .expect_err("outside write should be denied");

        assert!(err.to_string().contains("outside workspace"));
    }

    #[test]
    fn fs_write_allows_explicit_approved_root() {
        let workspace = temp_root("workspace-approved");
        let approved = temp_root("approved");
        let registry = ToolRegistry::new_with_scope(workspace, vec![approved.clone()]);

        let target = approved.join("ok.txt").display().to_string();
        let output = registry
            .execute("fs.write", &[target.clone(), "approved".to_owned()])
            .expect("approved root write should succeed");

        assert_eq!(output.status, "ok");
        let content = fs::read_to_string(approved.join("ok.txt")).expect("approved file exists");
        assert_eq!(content, "approved");
    }

    #[test]
    fn reads_approved_roots_from_env() {
        let _guard = ENV_LOCK.lock().expect("env lock should not be poisoned");
        let workspace = temp_root("workspace-env");
        let approved = temp_root("approved-env");
        let joined = env::join_paths([approved.clone()]).expect("join paths should work");

        unsafe {
            env::set_var("MEOW_FS_WRITE_ALLOW_ROOTS", joined);
        }

        let registry =
            ToolRegistry::new_with_scope(workspace, load_approved_write_roots(&PathBuf::from("/")));

        let target = approved.join("env-ok.txt").display().to_string();
        registry
            .execute("fs.write", &[target, "env".to_owned()])
            .expect("env approved root should be used");

        unsafe {
            env::remove_var("MEOW_FS_WRITE_ALLOW_ROOTS");
        }
    }
}
