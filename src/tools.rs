use std::collections::HashMap;
use std::fs;
use std::process::Command;

use anyhow::{Result, bail};
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
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut tools: HashMap<String, Box<dyn Tool>> = HashMap::new();
        register(&mut tools, EchoTool);
        register(&mut tools, ShellTool);
        register(&mut tools, FsReadTool);
        register(&mut tools, FsWriteTool);
        Self { tools }
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
        tool.execute(args)
    }

    pub fn is_known(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    pub fn is_risky(name: &str) -> bool {
        matches!(name, "shell" | "fs.write")
    }
}

fn register<T: Tool + 'static>(map: &mut HashMap<String, Box<dyn Tool>>, tool: T) {
    let name = tool.spec().name;
    map.insert(name, Box::new(tool));
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
        fs::write(path, content)?;

        Ok(ToolOutput {
            status: "ok".to_owned(),
            stdout: format!("wrote {}", path),
            stderr: String::new(),
        })
    }
}
