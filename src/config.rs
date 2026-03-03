use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MeowConfig {
    pub project: ProjectConfig,
    pub runtime: RuntimeConfig,
    pub security: SecurityConfig,
    pub storage: StorageConfig,
    pub mcp: McpConfig,
    pub providers: ProvidersConfig,
    pub profiles: Vec<ProfileConfig>,
}

impl Default for MeowConfig {
    fn default() -> Self {
        Self {
            project: ProjectConfig::default(),
            runtime: RuntimeConfig::default(),
            security: SecurityConfig::default(),
            storage: StorageConfig::default(),
            mcp: McpConfig::default(),
            providers: ProvidersConfig::default(),
            profiles: vec![ProfileConfig::default()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectConfig {
    pub name: String,
    pub default_profile: String,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: "meow-soma".to_owned(),
            default_profile: "default".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeConfig {
    pub default_provider: String,
    pub max_steps: u32,
    pub concurrency: u8,
    pub retry_budget: u8,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            default_provider: "ollama".to_owned(),
            max_steps: 20,
            concurrency: 2,
            retry_budget: 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    pub approval_policy: String,
    pub allowlist: Vec<String>,
    pub denylist: Vec<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            approval_policy: "permission_gate".to_owned(),
            allowlist: vec![
                "ls".to_owned(),
                "pwd".to_owned(),
                "cat".to_owned(),
                "rg".to_owned(),
                "git status".to_owned(),
                "git diff".to_owned(),
            ],
            denylist: vec![
                "rm -rf /".to_owned(),
                "mkfs".to_owned(),
                "dd if=".to_owned(),
                "git reset --hard".to_owned(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub root_dir: String,
    pub sqlite_path: String,
    pub memory_dir: String,
    pub artifacts_dir: String,
    pub logs_dir: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            root_dir: "~/.meow-soma".to_owned(),
            sqlite_path: "~/.meow-soma/state.db".to_owned(),
            memory_dir: "~/.meow-soma/memory".to_owned(),
            artifacts_dir: "~/.meow-soma/artifacts".to_owned(),
            logs_dir: "~/.meow-soma/logs".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct McpConfig {
    pub enabled: bool,
    pub transport: String,
    pub bind: String,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            transport: "stdio".to_owned(),
            bind: "127.0.0.1:7777".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProvidersConfig {
    pub openai: Option<ProviderConfig>,
    pub anthropic: Option<ProviderConfig>,
    pub ollama: Option<ProviderConfig>,
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            openai: Some(ProviderConfig {
                model: "gpt-4.1".to_owned(),
                endpoint: Some("https://api.openai.com/v1".to_owned()),
                api_key_env: Some("OPENAI_API_KEY".to_owned()),
                timeout_secs: 60,
            }),
            anthropic: Some(ProviderConfig {
                model: "claude-3-7-sonnet-latest".to_owned(),
                endpoint: Some("https://api.anthropic.com".to_owned()),
                api_key_env: Some("ANTHROPIC_API_KEY".to_owned()),
                timeout_secs: 60,
            }),
            ollama: Some(ProviderConfig {
                model: "llama3.1:8b".to_owned(),
                endpoint: Some("http://127.0.0.1:11434".to_owned()),
                api_key_env: None,
                timeout_secs: 60,
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    pub model: String,
    pub endpoint: Option<String>,
    pub api_key_env: Option<String>,
    pub timeout_secs: u64,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            model: "unknown".to_owned(),
            endpoint: None,
            api_key_env: None,
            timeout_secs: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileConfig {
    pub name: String,
    pub model: Option<String>,
    pub tools: Vec<String>,
    pub temperature: f32,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            name: "default".to_owned(),
            model: None,
            tools: vec!["shell".to_owned(), "fs.read".to_owned(), "echo".to_owned()],
            temperature: 0.2,
        }
    }
}

pub fn canonical_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("unable to resolve home directory")?;
    Ok(home.join(".meow-soma").join("config.toml"))
}

pub fn load(path_override: Option<&PathBuf>) -> Result<MeowConfig> {
    let path = resolve_config_path(path_override)?;
    if !path.exists() {
        return Ok(MeowConfig::default());
    }

    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed reading config file: {}", path.display()))?;

    let config: MeowConfig = toml::from_str(&raw)
        .with_context(|| format!("failed parsing TOML config: {}", path.display()))?;

    Ok(config)
}

pub fn write_default(path_override: Option<&PathBuf>, force: bool) -> Result<PathBuf> {
    let path = resolve_config_path(path_override)?;
    if path.exists() && !force {
        bail!(
            "config already exists at {} (pass --force to overwrite)",
            path.display()
        );
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent directory: {}", parent.display()))?;
    }

    let template = toml::to_string_pretty(&MeowConfig::default())
        .context("failed to serialize default config")?;
    fs::write(&path, template)
        .with_context(|| format!("failed writing config file: {}", path.display()))?;

    Ok(path)
}

pub fn validate(config: &MeowConfig) -> Result<()> {
    let mut errors = Vec::new();

    if config.project.name.trim().is_empty() {
        errors.push("[project].name must not be empty".to_owned());
    }

    if config.project.default_profile.trim().is_empty() {
        errors.push("[project].default_profile must not be empty".to_owned());
    }

    if config.runtime.max_steps == 0 {
        errors.push("[runtime].max_steps must be > 0".to_owned());
    }

    if config.runtime.concurrency == 0 {
        errors.push("[runtime].concurrency must be > 0".to_owned());
    }

    let policy = config.security.approval_policy.as_str();
    if !matches!(policy, "permission_gate" | "always_allow" | "read_only") {
        errors.push(
            "[security].approval_policy must be one of: permission_gate, always_allow, read_only"
                .to_owned(),
        );
    }

    if config.providers.openai.is_none()
        && config.providers.anthropic.is_none()
        && config.providers.ollama.is_none()
    {
        errors.push("at least one provider must be configured".to_owned());
    }

    if config
        .profiles
        .iter()
        .all(|profile| profile.name != config.project.default_profile)
    {
        errors.push(format!(
            "default profile '{}' not found in [[profiles]]",
            config.project.default_profile
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(errors.join("\n")))
    }
}

pub fn resolve_path(raw: &str) -> Result<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("path is empty"));
    }

    if trimmed == "~" || trimmed.starts_with("~/") {
        let home = dirs::home_dir().context("unable to resolve home directory")?;
        if trimmed == "~" {
            return Ok(home);
        }
        return Ok(home.join(trimmed.trim_start_matches("~/")));
    }

    Ok(Path::new(trimmed).to_path_buf())
}

fn resolve_config_path(path_override: Option<&PathBuf>) -> Result<PathBuf> {
    match path_override {
        Some(path) => Ok(path.clone()),
        None => canonical_config_path(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("config")
            .join(name)
    }

    #[test]
    fn valid_fixture_loads_and_validates() {
        let path = fixture_path("valid.toml");
        let cfg = load(Some(&path)).expect("fixture config should load");
        validate(&cfg).expect("fixture config should be valid");

        assert_eq!(cfg.project.name, "fixture-project");
        assert_eq!(cfg.project.default_profile, "default");
        assert_eq!(cfg.runtime.default_provider, "ollama");
    }

    #[test]
    fn invalid_fixture_reports_validation_errors() {
        let path = fixture_path("invalid_validation.toml");
        let cfg = load(Some(&path)).expect("fixture config should load");
        let err = validate(&cfg).expect_err("validation should fail");
        let message = err.to_string();

        for expected in [
            "[project].name must not be empty",
            "[runtime].max_steps must be > 0",
            "[runtime].concurrency must be > 0",
            "[security].approval_policy must be one of: permission_gate, always_allow, read_only",
            "default profile 'missing-profile' not found in [[profiles]]",
        ] {
            assert!(
                message.contains(expected),
                "validation error should contain `{expected}`, got: {message}"
            );
        }
    }
}
