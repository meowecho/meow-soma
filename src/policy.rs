use crate::config::SecurityConfig;

#[derive(Debug, Clone)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub requires_approval: bool,
    pub reason: String,
}

pub struct PolicyEngine {
    policy: String,
    allowlist: Vec<String>,
    denylist: Vec<String>,
}

impl PolicyEngine {
    pub fn new(config: &SecurityConfig) -> Self {
        Self {
            policy: config.approval_policy.clone(),
            allowlist: config.allowlist.clone(),
            denylist: config.denylist.clone(),
        }
    }

    pub fn evaluate_shell(&self, command: &str) -> PolicyDecision {
        let cmd = command.trim();
        let cmd_lower = cmd.to_lowercase();

        if cmd.is_empty() {
            return PolicyDecision {
                allowed: false,
                requires_approval: false,
                reason: "empty command is not allowed".to_owned(),
            };
        }

        if self
            .denylist
            .iter()
            .any(|denied| cmd_lower.contains(&denied.to_lowercase()))
        {
            return PolicyDecision {
                allowed: false,
                requires_approval: false,
                reason: "command is explicitly denied by policy".to_owned(),
            };
        }

        if self.policy == "always_allow" {
            return PolicyDecision {
                allowed: true,
                requires_approval: false,
                reason: "policy is always_allow".to_owned(),
            };
        }

        let is_allowlisted = self
            .allowlist
            .iter()
            .any(|allowed| cmd_lower.starts_with(&allowed.to_lowercase()));

        if self.policy == "read_only" {
            return if is_allowlisted {
                PolicyDecision {
                    allowed: true,
                    requires_approval: false,
                    reason: "read_only policy allowlisted command".to_owned(),
                }
            } else {
                PolicyDecision {
                    allowed: false,
                    requires_approval: false,
                    reason: "read_only policy blocks non-allowlisted command".to_owned(),
                }
            };
        }

        let risky = is_risky_shell_command(&cmd_lower);
        if risky {
            return PolicyDecision {
                allowed: true,
                requires_approval: true,
                reason: "potentially destructive command requires approval".to_owned(),
            };
        }

        if is_allowlisted {
            PolicyDecision {
                allowed: true,
                requires_approval: false,
                reason: "command allowlisted".to_owned(),
            }
        } else {
            PolicyDecision {
                allowed: true,
                requires_approval: true,
                reason: "command outside allowlist requires approval".to_owned(),
            }
        }
    }

    pub fn evaluate_tool(&self, tool_name: &str, risky: bool) -> PolicyDecision {
        if self
            .denylist
            .iter()
            .any(|denied| tool_name.eq_ignore_ascii_case(denied))
        {
            return PolicyDecision {
                allowed: false,
                requires_approval: false,
                reason: "tool is explicitly denied by policy".to_owned(),
            };
        }

        if self.policy == "always_allow" {
            return PolicyDecision {
                allowed: true,
                requires_approval: false,
                reason: "policy is always_allow".to_owned(),
            };
        }

        if self.policy == "read_only" && risky {
            return PolicyDecision {
                allowed: false,
                requires_approval: false,
                reason: "read_only policy blocks risky tools".to_owned(),
            };
        }

        if risky {
            return PolicyDecision {
                allowed: true,
                requires_approval: true,
                reason: "risky tool requires approval".to_owned(),
            };
        }

        PolicyDecision {
            allowed: true,
            requires_approval: false,
            reason: "tool allowed by policy".to_owned(),
        }
    }
}

fn is_risky_shell_command(cmd_lower: &str) -> bool {
    const RISKY_PATTERNS: &[&str] = &[
        " rm ",
        " rm-",
        "rm ",
        "sudo ",
        "chmod ",
        "chown ",
        "git reset",
        "git clean",
        "mkfs",
        "dd if=",
        "shutdown",
        "reboot",
        "kill -9",
        ">",
        "2>",
    ];

    RISKY_PATTERNS
        .iter()
        .any(|pattern| cmd_lower.contains(pattern))
}
