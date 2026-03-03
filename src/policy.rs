use crate::config::SecurityConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicySeverity {
    Allow,
    ApproveRequired,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyReasonCode {
    EmptyCommand,
    DenylistMatch,
    AlwaysAllowPolicy,
    ReadOnlyAllowlisted,
    ReadOnlyBlocked,
    RiskyShell,
    OutsideAllowlist,
    RiskyTool,
    ToolAllowed,
}

impl PolicyReasonCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EmptyCommand => "empty_command",
            Self::DenylistMatch => "denylist_match",
            Self::AlwaysAllowPolicy => "always_allow_policy",
            Self::ReadOnlyAllowlisted => "read_only_allowlisted",
            Self::ReadOnlyBlocked => "read_only_blocked",
            Self::RiskyShell => "risky_shell",
            Self::OutsideAllowlist => "outside_allowlist",
            Self::RiskyTool => "risky_tool",
            Self::ToolAllowed => "tool_allowed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PolicyDecision {
    pub severity: PolicySeverity,
    pub reason_code: PolicyReasonCode,
    pub reason: String,
}

impl PolicyDecision {
    fn allow(reason_code: PolicyReasonCode, reason: impl Into<String>) -> Self {
        Self {
            severity: PolicySeverity::Allow,
            reason_code,
            reason: reason.into(),
        }
    }

    fn approve_required(reason_code: PolicyReasonCode, reason: impl Into<String>) -> Self {
        Self {
            severity: PolicySeverity::ApproveRequired,
            reason_code,
            reason: reason.into(),
        }
    }

    fn deny(reason_code: PolicyReasonCode, reason: impl Into<String>) -> Self {
        Self {
            severity: PolicySeverity::Deny,
            reason_code,
            reason: reason.into(),
        }
    }

    pub fn is_allowed(&self) -> bool {
        !matches!(self.severity, PolicySeverity::Deny)
    }

    pub fn requires_approval(&self) -> bool {
        matches!(self.severity, PolicySeverity::ApproveRequired)
    }

    pub fn reason_code(&self) -> &'static str {
        self.reason_code.as_str()
    }
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
        let cmd = normalize_shell_command(command);
        let cmd_lower = cmd.to_ascii_lowercase();

        if cmd.is_empty() {
            return PolicyDecision::deny(
                PolicyReasonCode::EmptyCommand,
                "empty command is not allowed",
            );
        }

        if self
            .denylist
            .iter()
            .map(|item| normalize_shell_command(item))
            .map(|item| item.to_ascii_lowercase())
            .any(|denied| cmd_lower.contains(&denied))
        {
            return PolicyDecision::deny(
                PolicyReasonCode::DenylistMatch,
                "command is explicitly denied by policy",
            );
        }

        if self.policy == "always_allow" {
            return PolicyDecision::allow(
                PolicyReasonCode::AlwaysAllowPolicy,
                "policy is always_allow",
            );
        }

        let is_allowlisted = self
            .allowlist
            .iter()
            .map(|item| normalize_shell_command(item))
            .map(|item| item.to_ascii_lowercase())
            .any(|allowed| cmd_lower.starts_with(&allowed));

        if self.policy == "read_only" {
            return if is_allowlisted {
                PolicyDecision::allow(
                    PolicyReasonCode::ReadOnlyAllowlisted,
                    "read_only policy allowlisted command",
                )
            } else {
                PolicyDecision::deny(
                    PolicyReasonCode::ReadOnlyBlocked,
                    "read_only policy blocks non-allowlisted command",
                )
            };
        }

        let risky = is_risky_shell_command(&cmd_lower);
        if risky {
            return PolicyDecision::approve_required(
                PolicyReasonCode::RiskyShell,
                "potentially destructive command requires approval",
            );
        }

        if is_allowlisted {
            PolicyDecision::allow(PolicyReasonCode::ToolAllowed, "command allowlisted")
        } else {
            PolicyDecision::approve_required(
                PolicyReasonCode::OutsideAllowlist,
                "command outside allowlist requires approval",
            )
        }
    }

    pub fn evaluate_tool(&self, tool_name: &str, risky: bool) -> PolicyDecision {
        if self
            .denylist
            .iter()
            .any(|denied| tool_name.eq_ignore_ascii_case(denied))
        {
            return PolicyDecision::deny(
                PolicyReasonCode::DenylistMatch,
                "tool is explicitly denied by policy",
            );
        }

        if self.policy == "always_allow" {
            return PolicyDecision::allow(
                PolicyReasonCode::AlwaysAllowPolicy,
                "policy is always_allow",
            );
        }

        if self.policy == "read_only" && risky {
            return PolicyDecision::deny(
                PolicyReasonCode::ReadOnlyBlocked,
                "read_only policy blocks risky tools",
            );
        }

        if risky {
            return PolicyDecision::approve_required(
                PolicyReasonCode::RiskyTool,
                "risky tool requires approval",
            );
        }

        PolicyDecision::allow(PolicyReasonCode::ToolAllowed, "tool allowed by policy")
    }
}

fn normalize_shell_command(command: &str) -> String {
    command.split_whitespace().collect::<Vec<_>>().join(" ")
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::path::PathBuf;

    fn config(policy: &str) -> SecurityConfig {
        SecurityConfig {
            approval_policy: policy.to_owned(),
            allowlist: vec!["git status".to_owned(), "ls".to_owned()],
            denylist: vec!["rm -rf /".to_owned(), "blocked-tool".to_owned()],
        }
    }

    #[derive(Debug, Deserialize)]
    struct PolicyFixture {
        security: SecurityConfig,
        cases: Vec<PolicyFixtureCase>,
    }

    #[derive(Debug, Deserialize)]
    struct PolicyFixtureCase {
        kind: String,
        input: String,
        risky: Option<bool>,
        expected_severity: String,
        expected_reason_code: String,
    }

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("policy")
            .join(name)
    }

    fn expected_severity(raw: &str) -> PolicySeverity {
        match raw {
            "allow" => PolicySeverity::Allow,
            "approve_required" => PolicySeverity::ApproveRequired,
            "deny" => PolicySeverity::Deny,
            value => panic!("unknown fixture severity: {value}"),
        }
    }

    fn run_policy_fixture(name: &str) {
        let path = fixture_path(name);
        let raw = std::fs::read_to_string(&path).expect("policy fixture should be readable");
        let fixture: PolicyFixture =
            toml::from_str(&raw).expect("policy fixture should parse as toml");
        let engine = PolicyEngine::new(&fixture.security);

        for case in fixture.cases {
            let decision = match case.kind.as_str() {
                "shell" => engine.evaluate_shell(&case.input),
                "tool" => engine.evaluate_tool(&case.input, case.risky.unwrap_or(false)),
                value => panic!("unknown fixture case kind: {value}"),
            };

            assert_eq!(
                decision.severity,
                expected_severity(&case.expected_severity),
                "unexpected severity for fixture case: {:?}",
                case
            );
            assert_eq!(
                decision.reason_code(),
                case.expected_reason_code,
                "unexpected reason code for fixture case: {:?}",
                case
            );
        }
    }

    #[test]
    fn permission_gate_fixture_cases_pass() {
        run_policy_fixture("permission_gate.toml");
    }

    #[test]
    fn read_only_fixture_cases_pass() {
        run_policy_fixture("read_only.toml");
    }

    #[test]
    fn shell_command_is_normalized_before_policy_checks() {
        let engine = PolicyEngine::new(&config("permission_gate"));
        let decision = engine.evaluate_shell("   git    status   ");
        assert_eq!(decision.severity, PolicySeverity::Allow);
        assert_eq!(decision.reason_code, PolicyReasonCode::ToolAllowed);
    }

    #[test]
    fn risky_shell_is_approve_required() {
        let engine = PolicyEngine::new(&config("permission_gate"));
        let decision = engine.evaluate_shell("rm -rf ./tmp");
        assert_eq!(decision.severity, PolicySeverity::ApproveRequired);
        assert_eq!(decision.reason_code, PolicyReasonCode::RiskyShell);
        assert!(decision.requires_approval());
    }

    #[test]
    fn read_only_denies_non_allowlisted_shell() {
        let engine = PolicyEngine::new(&config("read_only"));
        let decision = engine.evaluate_shell("git commit -m test");
        assert_eq!(decision.severity, PolicySeverity::Deny);
        assert_eq!(decision.reason_code, PolicyReasonCode::ReadOnlyBlocked);
        assert!(!decision.is_allowed());
    }

    #[test]
    fn risky_tool_requires_approval() {
        let engine = PolicyEngine::new(&config("permission_gate"));
        let decision = engine.evaluate_tool("fs.write", true);
        assert_eq!(decision.severity, PolicySeverity::ApproveRequired);
        assert_eq!(decision.reason_code, PolicyReasonCode::RiskyTool);
    }

    #[test]
    fn denylist_tool_is_denied() {
        let engine = PolicyEngine::new(&config("permission_gate"));
        let decision = engine.evaluate_tool("blocked-tool", false);
        assert_eq!(decision.severity, PolicySeverity::Deny);
        assert_eq!(decision.reason_code, PolicyReasonCode::DenylistMatch);
    }
}
