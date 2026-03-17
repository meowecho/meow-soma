use crate::config::{
    APPROVAL_POLICY_ALLOW, APPROVAL_POLICY_ASK, APPROVAL_POLICY_DENY, SecurityConfig,
    normalize_approval_policy,
};

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
    mode: PolicyMode,
    allowlist: Vec<PolicyRule>,
    denylist: Vec<PolicyRule>,
}

impl PolicyEngine {
    pub fn new(config: &SecurityConfig) -> Self {
        Self {
            mode: PolicyMode::from_raw(&config.approval_policy),
            allowlist: parse_rules(&config.allowlist),
            denylist: parse_rules(&config.denylist),
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

        if self.matches_shell_rule(&self.denylist, &cmd_lower, RuleListKind::Deny) {
            return PolicyDecision::deny(
                PolicyReasonCode::DenylistMatch,
                "command is explicitly denied by policy",
            );
        }

        if matches!(self.mode, PolicyMode::Allow) {
            return PolicyDecision::allow(
                PolicyReasonCode::AlwaysAllowPolicy,
                "policy mode allow permits command",
            );
        }

        let is_allowlisted =
            self.matches_shell_rule(&self.allowlist, &cmd_lower, RuleListKind::Allow);
        let risky = is_risky_shell_command(&cmd_lower);

        if matches!(self.mode, PolicyMode::Deny) {
            if risky {
                return PolicyDecision::deny(
                    PolicyReasonCode::ReadOnlyBlocked,
                    "deny mode blocks risky shell commands",
                );
            }
            return if is_allowlisted {
                PolicyDecision::allow(
                    PolicyReasonCode::ReadOnlyAllowlisted,
                    "deny mode allowlisted command",
                )
            } else {
                PolicyDecision::deny(
                    PolicyReasonCode::ReadOnlyBlocked,
                    "deny mode blocks non-allowlisted command",
                )
            };
        }

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

    pub fn evaluate_tool(
        &self,
        tool_name: &str,
        tool_args: &[String],
        risky: bool,
    ) -> PolicyDecision {
        let tool_name_lower = normalize_tool_name(tool_name);
        if tool_name_lower.is_empty() {
            return PolicyDecision::deny(
                PolicyReasonCode::EmptyCommand,
                "empty tool name is not allowed",
            );
        }

        let specifier = normalize_tool_specifier(&tool_name_lower, tool_args);

        if self.matches_tool_rule(&self.denylist, &tool_name_lower, &specifier) {
            return PolicyDecision::deny(
                PolicyReasonCode::DenylistMatch,
                "tool is explicitly denied by policy",
            );
        }

        if matches!(self.mode, PolicyMode::Allow) {
            return PolicyDecision::allow(
                PolicyReasonCode::AlwaysAllowPolicy,
                "policy mode allow permits tool execution",
            );
        }

        let is_allowlisted = self.matches_tool_rule(&self.allowlist, &tool_name_lower, &specifier);

        if matches!(self.mode, PolicyMode::Deny) {
            if risky {
                return PolicyDecision::deny(
                    PolicyReasonCode::ReadOnlyBlocked,
                    "deny mode blocks risky tools",
                );
            }

            return if is_allowlisted {
                PolicyDecision::allow(
                    PolicyReasonCode::ReadOnlyAllowlisted,
                    "deny mode allowlisted tool",
                )
            } else {
                PolicyDecision::deny(
                    PolicyReasonCode::ReadOnlyBlocked,
                    "deny mode blocks non-allowlisted tools",
                )
            };
        }

        if risky {
            return PolicyDecision::approve_required(
                PolicyReasonCode::RiskyTool,
                "risky tool requires approval",
            );
        }

        PolicyDecision::allow(PolicyReasonCode::ToolAllowed, "tool allowed by ask mode")
    }

    fn matches_shell_rule(
        &self,
        rules: &[PolicyRule],
        command: &str,
        list_kind: RuleListKind,
    ) -> bool {
        rules
            .iter()
            .any(|rule| rule_matches_shell(rule, command, list_kind))
    }

    fn matches_tool_rule(&self, rules: &[PolicyRule], name: &str, specifier: &str) -> bool {
        rules
            .iter()
            .any(|rule| rule_matches_tool(rule, name, specifier))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PolicyMode {
    Allow,
    Ask,
    Deny,
}

impl PolicyMode {
    fn from_raw(raw: &str) -> Self {
        match normalize_approval_policy(raw) {
            Some(APPROVAL_POLICY_ALLOW) => Self::Allow,
            Some(APPROVAL_POLICY_DENY) => Self::Deny,
            Some(APPROVAL_POLICY_ASK) | None => Self::Ask,
            Some(other) => {
                debug_assert!(matches!(
                    other,
                    APPROVAL_POLICY_ALLOW | APPROVAL_POLICY_ASK | APPROVAL_POLICY_DENY
                ));
                Self::Ask
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleScope {
    Any,
    Shell,
    Tool,
}

#[derive(Debug, Clone)]
struct PolicyRule {
    scope: RuleScope,
    pattern: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleListKind {
    Allow,
    Deny,
}

fn parse_rules(raw_rules: &[String]) -> Vec<PolicyRule> {
    raw_rules
        .iter()
        .filter_map(|rule| parse_rule(rule))
        .collect()
}

fn parse_rule(raw_rule: &str) -> Option<PolicyRule> {
    let normalized = normalize_shell_command(raw_rule);
    if normalized.is_empty() {
        return None;
    }

    let lowered = normalized.to_ascii_lowercase();
    if let Some(rest) = lowered.strip_prefix("shell:") {
        let pattern = normalize_shell_command(rest);
        if pattern.is_empty() {
            return None;
        }
        return Some(PolicyRule {
            scope: RuleScope::Shell,
            pattern,
        });
    }

    if let Some(rest) = lowered.strip_prefix("tool:") {
        let pattern = normalize_shell_command(rest);
        if pattern.is_empty() {
            return None;
        }
        return Some(PolicyRule {
            scope: RuleScope::Tool,
            pattern,
        });
    }

    Some(PolicyRule {
        scope: RuleScope::Any,
        pattern: lowered,
    })
}

fn normalize_tool_name(tool_name: &str) -> String {
    normalize_shell_command(tool_name).to_ascii_lowercase()
}

fn normalize_tool_specifier(tool_name: &str, tool_args: &[String]) -> String {
    let mut parts = Vec::with_capacity(1 + tool_args.len());
    parts.push(tool_name.to_owned());
    parts.extend(
        tool_args
            .iter()
            .map(|arg| normalize_tool_match_value(&normalize_shell_command(arg)))
            .filter(|arg| !arg.is_empty()),
    );
    normalize_tool_match_value(&parts.join(" ")).to_ascii_lowercase()
}

fn rule_matches_shell(rule: &PolicyRule, command: &str, list_kind: RuleListKind) -> bool {
    if matches!(rule.scope, RuleScope::Tool) {
        return false;
    }

    if rule.pattern.is_empty() {
        return false;
    }

    match list_kind {
        RuleListKind::Allow => command.starts_with(&rule.pattern),
        RuleListKind::Deny => command.contains(&rule.pattern),
    }
}

fn rule_matches_tool(rule: &PolicyRule, tool_name: &str, specifier: &str) -> bool {
    if matches!(rule.scope, RuleScope::Shell) || rule.pattern.is_empty() {
        return false;
    }

    let pattern = normalize_tool_match_value(&rule.pattern);
    let normalized_specifier = normalize_tool_match_value(specifier);

    if pattern.contains(' ') {
        normalized_specifier.starts_with(&pattern)
    } else {
        tool_name == pattern
    }
}

fn normalize_tool_match_value(value: &str) -> String {
    value.replace('\\', "/")
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
        args: Option<Vec<String>>,
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
                "tool" => engine.evaluate_tool(
                    &case.input,
                    case.args.as_deref().unwrap_or(&[]),
                    case.risky.unwrap_or(false),
                ),
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
        let decision = engine.evaluate_tool("fs.write", &[], true);
        assert_eq!(decision.severity, PolicySeverity::ApproveRequired);
        assert_eq!(decision.reason_code, PolicyReasonCode::RiskyTool);
    }

    #[test]
    fn denylist_tool_is_denied() {
        let engine = PolicyEngine::new(&config("permission_gate"));
        let decision = engine.evaluate_tool("blocked-tool", &[], false);
        assert_eq!(decision.severity, PolicySeverity::Deny);
        assert_eq!(decision.reason_code, PolicyReasonCode::DenylistMatch);
    }

    #[test]
    fn canonical_policy_aliases_map_to_expected_modes() {
        let ask_alias = PolicyEngine::new(&config("permission_gate"));
        let ask_canonical = PolicyEngine::new(&config("ask"));
        let allow_alias = PolicyEngine::new(&config("always_allow"));
        let allow_canonical = PolicyEngine::new(&config("allow"));
        let deny_alias = PolicyEngine::new(&config("read_only"));
        let deny_canonical = PolicyEngine::new(&config("deny"));

        let risky_args = vec!["tmp/file.txt".to_owned(), "hi".to_owned()];
        assert_eq!(
            ask_alias
                .evaluate_tool("fs.write", &risky_args, true)
                .severity,
            ask_canonical
                .evaluate_tool("fs.write", &risky_args, true)
                .severity
        );
        assert_eq!(
            allow_alias
                .evaluate_tool("fs.write", &risky_args, true)
                .severity,
            allow_canonical
                .evaluate_tool("fs.write", &risky_args, true)
                .severity
        );
        assert_eq!(
            deny_alias
                .evaluate_tool("fs.write", &risky_args, true)
                .severity,
            deny_canonical
                .evaluate_tool("fs.write", &risky_args, true)
                .severity
        );
    }

    #[test]
    fn tool_specifier_rule_matches_path_prefix() {
        let cfg = SecurityConfig {
            approval_policy: "ask".to_owned(),
            allowlist: vec!["tool:echo".to_owned()],
            denylist: vec!["tool:fs.write tmp/protected".to_owned()],
        };
        let engine = PolicyEngine::new(&cfg);

        let denied = engine.evaluate_tool(
            "fs.write",
            &["tmp/protected.txt".to_owned(), "hello".to_owned()],
            true,
        );
        assert_eq!(denied.severity, PolicySeverity::Deny);
        assert_eq!(denied.reason_code, PolicyReasonCode::DenylistMatch);

        let approval_required = engine.evaluate_tool(
            "fs.write",
            &["tmp/other.txt".to_owned(), "hello".to_owned()],
            true,
        );
        assert_eq!(approval_required.severity, PolicySeverity::ApproveRequired);
        assert_eq!(approval_required.reason_code, PolicyReasonCode::RiskyTool);
    }

    #[test]
    fn tool_specifier_matching_normalizes_windows_path_separators() {
        let cfg = SecurityConfig {
            approval_policy: "ask".to_owned(),
            allowlist: vec!["tool:echo".to_owned()],
            denylist: vec!["tool:fs.write c:/tmp/protected".to_owned()],
        };
        let engine = PolicyEngine::new(&cfg);

        let denied = engine.evaluate_tool(
            "fs.write",
            &[
                "C:\\tmp\\protected\\report.txt".to_owned(),
                "hello".to_owned(),
            ],
            true,
        );
        assert_eq!(denied.severity, PolicySeverity::Deny);
        assert_eq!(denied.reason_code, PolicyReasonCode::DenylistMatch);
    }

    #[test]
    fn deny_mode_allows_only_safe_allowlisted_actions() {
        let cfg = SecurityConfig {
            approval_policy: "deny".to_owned(),
            allowlist: vec!["tool:echo".to_owned(), "ls".to_owned()],
            denylist: vec![],
        };
        let engine = PolicyEngine::new(&cfg);

        let allowed_tool = engine.evaluate_tool("echo", &["ok".to_owned()], false);
        assert_eq!(allowed_tool.severity, PolicySeverity::Allow);
        assert_eq!(
            allowed_tool.reason_code,
            PolicyReasonCode::ReadOnlyAllowlisted
        );

        let denied_tool = engine.evaluate_tool("fs.read", &["Cargo.toml".to_owned()], false);
        assert_eq!(denied_tool.severity, PolicySeverity::Deny);
        assert_eq!(denied_tool.reason_code, PolicyReasonCode::ReadOnlyBlocked);

        let denied_risky = engine.evaluate_tool(
            "fs.write",
            &["tmp/file.txt".to_owned(), "x".to_owned()],
            true,
        );
        assert_eq!(denied_risky.severity, PolicySeverity::Deny);
        assert_eq!(denied_risky.reason_code, PolicyReasonCode::ReadOnlyBlocked);
    }
}
