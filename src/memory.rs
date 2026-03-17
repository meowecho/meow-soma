use std::fmt;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::{self, MeowConfig};

pub const PROJECT_MEMORY_RELATIVE_PATH: &str = ".meow-soma/instructions.md";
pub const LOCAL_MEMORY_RELATIVE_PATH: &str = ".meow-soma/instructions.local.md";
pub const USER_MEMORY_FILE_NAME: &str = "instructions.md";

const PROJECT_INIT_TEMPLATE: &str = "# Meow Soma Project Instructions\n\
\n\
Add project-specific runtime instructions here.\n\
This file has highest precedence over local and user instruction scopes.\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionScope {
    User,
    Local,
    Project,
}

impl InstructionScope {
    pub fn as_str(self) -> &'static str {
        match self {
            InstructionScope::User => "user",
            InstructionScope::Local => "local",
            InstructionScope::Project => "project",
        }
    }
}

impl fmt::Display for InstructionScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstructionPaths {
    pub user: PathBuf,
    pub local: PathBuf,
    pub project: PathBuf,
    pub project_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeSnapshot {
    pub scope: InstructionScope,
    pub path: PathBuf,
    pub exists: bool,
    pub content: Option<String>,
}

impl ScopeSnapshot {
    pub fn has_content(&self) -> bool {
        self.content
            .as_ref()
            .is_some_and(|content| !content.is_empty())
    }

    pub fn status_label(&self) -> &'static str {
        match (self.exists, self.has_content()) {
            (false, _) => "missing",
            (true, false) => "empty",
            (true, true) => "loaded",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstructionSnapshot {
    pub scopes: Vec<ScopeSnapshot>,
    pub effective_instructions: String,
}

impl InstructionSnapshot {
    pub fn loaded_scope_count(&self) -> usize {
        self.scopes
            .iter()
            .filter(|scope| scope.has_content())
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitProjectOutcome {
    Created,
    Overwritten,
    AlreadyExists,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitProjectResult {
    pub outcome: InitProjectOutcome,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstructionMemory {
    paths: InstructionPaths,
    snapshot: InstructionSnapshot,
}

impl InstructionMemory {
    pub fn load(config: &MeowConfig, cwd: &Path) -> Result<Self> {
        let paths = resolve_instruction_paths(config, cwd)?;
        Self::from_paths(paths)
    }

    pub fn from_paths(paths: InstructionPaths) -> Result<Self> {
        let snapshot = load_snapshot(&paths)?;
        Ok(Self { paths, snapshot })
    }

    pub fn paths(&self) -> &InstructionPaths {
        &self.paths
    }

    pub fn snapshot(&self) -> &InstructionSnapshot {
        &self.snapshot
    }

    pub fn effective_context_block(&self) -> Option<String> {
        if self.snapshot.effective_instructions.is_empty() {
            None
        } else {
            Some(format!(
                "Instruction memory hierarchy (precedence: user < local < project):\n{}",
                self.snapshot.effective_instructions
            ))
        }
    }

    pub fn reload(&mut self) -> Result<()> {
        self.snapshot = load_snapshot(&self.paths)?;
        Ok(())
    }

    pub fn init_project_file(&mut self, force: bool) -> Result<InitProjectResult> {
        let path = self.paths.project.clone();
        reject_symlink_path(&path, "write to")?;
        validate_project_scoped_path(&path, &self.paths.project_root, "write to")?;
        let existed = path.exists();

        if existed && !force {
            return Ok(InitProjectResult {
                outcome: InitProjectOutcome::AlreadyExists,
                path,
            });
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed creating project memory directory: {}",
                    parent.display()
                )
            })?;
        }

        fs::write(&path, PROJECT_INIT_TEMPLATE).with_context(|| {
            format!(
                "failed writing project instruction file: {}",
                path.display()
            )
        })?;

        self.reload()?;

        Ok(InitProjectResult {
            outcome: if existed {
                InitProjectOutcome::Overwritten
            } else {
                InitProjectOutcome::Created
            },
            path,
        })
    }
}

pub fn resolve_instruction_paths(config: &MeowConfig, cwd: &Path) -> Result<InstructionPaths> {
    let user_memory_dir = config::resolve_path(&config.storage.memory_dir)?;
    let project_root = discover_project_root(cwd);

    Ok(InstructionPaths {
        user: user_memory_dir.join(USER_MEMORY_FILE_NAME),
        local: project_root.join(LOCAL_MEMORY_RELATIVE_PATH),
        project: project_root.join(PROJECT_MEMORY_RELATIVE_PATH),
        project_root,
    })
}

fn load_snapshot(paths: &InstructionPaths) -> Result<InstructionSnapshot> {
    let mut scopes = Vec::with_capacity(3);
    scopes.push(load_scope_snapshot(
        InstructionScope::User,
        &paths.user,
        None,
    )?);
    scopes.push(load_scope_snapshot(
        InstructionScope::Local,
        &paths.local,
        Some(&paths.project_root),
    )?);
    scopes.push(load_scope_snapshot(
        InstructionScope::Project,
        &paths.project,
        Some(&paths.project_root),
    )?);

    let effective_instructions = render_effective_instructions(&scopes);

    Ok(InstructionSnapshot {
        scopes,
        effective_instructions,
    })
}

fn load_scope_snapshot(
    scope: InstructionScope,
    path: &Path,
    project_root: Option<&Path>,
) -> Result<ScopeSnapshot> {
    reject_symlink_path(path, "read")?;
    if let Some(project_root) = project_root {
        validate_project_scoped_path(path, project_root, "read")?;
    }

    if !path.exists() {
        return Ok(ScopeSnapshot {
            scope,
            path: path.to_path_buf(),
            exists: false,
            content: None,
        });
    }

    let content = fs::read_to_string(path).with_context(|| {
        format!(
            "failed reading {scope} instruction file: {}",
            path.display()
        )
    })?;
    let normalized = normalize_instruction_content(&content);

    Ok(ScopeSnapshot {
        scope,
        path: path.to_path_buf(),
        exists: true,
        content: if normalized.is_empty() {
            None
        } else {
            Some(normalized)
        },
    })
}

fn render_effective_instructions(scopes: &[ScopeSnapshot]) -> String {
    scopes
        .iter()
        .filter_map(|scope| {
            scope
                .content
                .as_ref()
                .map(|content| format!("{} instructions:\n{}", scope.scope, content))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn normalize_instruction_content(raw: &str) -> String {
    raw.replace("\r\n", "\n").trim().to_owned()
}

fn reject_symlink_path(path: &Path, action: &str) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "failed to inspect instruction file metadata: {}",
                    path.display()
                )
            });
        }
    };
    if metadata.file_type().is_symlink() {
        bail!(
            "refusing to {action} symlinked instruction file: {}",
            path.display()
        );
    }
    Ok(())
}

fn validate_project_scoped_path(path: &Path, project_root: &Path, action: &str) -> Result<()> {
    if !path.starts_with(project_root) {
        bail!(
            "refusing to {action} instruction path outside project root: {}",
            path.display()
        );
    }

    let canonical_root = match fs::canonicalize(project_root) {
        Ok(path) => Some(path),
        Err(err) if err.kind() == ErrorKind::NotFound => None,
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "failed to canonicalize project root while validating instruction path: {}",
                    project_root.display()
                )
            });
        }
    };

    let mut cursor = Some(path.to_path_buf());
    while let Some(current) = cursor {
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => Some(metadata),
            Err(err) if err.kind() == ErrorKind::NotFound => None,
            Err(err) => {
                return Err(err).with_context(|| {
                    format!(
                        "failed to inspect instruction path component metadata: {}",
                        current.display()
                    )
                });
            }
        };

        if let Some(metadata) = metadata {
            if metadata.file_type().is_symlink() {
                bail!(
                    "refusing to {action} instruction path through symlinked directory: {}",
                    current.display()
                );
            }
            if let Some(canonical_root) = canonical_root.as_ref() {
                let canonical_current = fs::canonicalize(&current).with_context(|| {
                    format!(
                        "failed to canonicalize instruction path component: {}",
                        current.display()
                    )
                })?;
                if !canonical_current.starts_with(canonical_root) {
                    bail!(
                        "refusing to {action} instruction path outside project root: {}",
                        path.display()
                    );
                }
            }
        }

        if current == project_root {
            break;
        }
        cursor = current.parent().map(Path::to_path_buf);
    }

    Ok(())
}

fn discover_project_root(cwd: &Path) -> PathBuf {
    for ancestor in cwd.ancestors() {
        if ancestor.join(PROJECT_MEMORY_RELATIVE_PATH).exists() {
            return ancestor.to_path_buf();
        }
    }

    for ancestor in cwd.ancestors() {
        if ancestor.join(".git").exists() {
            return ancestor.to_path_buf();
        }
    }

    cwd.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    use uuid::Uuid;

    fn temp_root(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{nanos}-{}", Uuid::new_v4()))
    }

    fn test_config(memory_dir: &Path) -> MeowConfig {
        let mut cfg = MeowConfig::default();
        cfg.storage.memory_dir = memory_dir.display().to_string();
        cfg
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent directory should be created");
        }
        fs::write(path, content).expect("file should be written");
    }

    #[test]
    fn precedence_merges_user_local_then_project() {
        let root = temp_root("meow-memory-precedence");
        let workspace = root.join("workspace");
        let user_memory = root.join("user-memory");

        fs::create_dir_all(workspace.join(".git")).expect("git dir should exist");

        write_file(
            &user_memory.join(USER_MEMORY_FILE_NAME),
            "Use snake_case variables.",
        );
        write_file(
            &workspace.join(LOCAL_MEMORY_RELATIVE_PATH),
            "Prefer descriptive error messages.",
        );
        write_file(
            &workspace.join(PROJECT_MEMORY_RELATIVE_PATH),
            "Never add TODO placeholders.",
        );

        let cfg = test_config(&user_memory);
        let memory = InstructionMemory::load(&cfg, &workspace).expect("memory should load");
        let effective = memory
            .effective_context_block()
            .expect("effective memory should be present");

        let user_idx = effective
            .find("user instructions")
            .expect("user scope should be included");
        let local_idx = effective
            .find("local instructions")
            .expect("local scope should be included");
        let project_idx = effective
            .find("project instructions")
            .expect("project scope should be included");

        assert!(user_idx < local_idx, "user scope should come before local");
        assert!(
            local_idx < project_idx,
            "local scope should come before project"
        );
    }

    #[test]
    fn resolve_paths_uses_git_root_when_no_project_file_exists() {
        let root = temp_root("meow-memory-git-root");
        let workspace = root.join("workspace");
        let nested = workspace.join("src/nested");
        let user_memory = root.join("user-memory");

        fs::create_dir_all(workspace.join(".git")).expect("git dir should be created");
        fs::create_dir_all(&nested).expect("nested workspace should exist");

        let cfg = test_config(&user_memory);
        let paths = resolve_instruction_paths(&cfg, &nested).expect("paths should resolve");

        assert_eq!(paths.project_root, workspace);
        assert_eq!(
            paths.project,
            paths.project_root.join(PROJECT_MEMORY_RELATIVE_PATH)
        );
        assert_eq!(
            paths.local,
            paths.project_root.join(LOCAL_MEMORY_RELATIVE_PATH)
        );
    }

    #[test]
    fn reload_picks_up_instruction_file_changes() {
        let root = temp_root("meow-memory-reload");
        let workspace = root.join("workspace");
        let user_memory = root.join("user-memory");

        fs::create_dir_all(workspace.join(".git")).expect("git dir should exist");
        write_file(
            &user_memory.join(USER_MEMORY_FILE_NAME),
            "Initial user instruction.",
        );

        let cfg = test_config(&user_memory);
        let mut memory = InstructionMemory::load(&cfg, &workspace).expect("memory should load");

        let initial = memory
            .effective_context_block()
            .expect("initial effective instructions should exist");
        assert!(initial.contains("Initial user instruction."));
        assert!(!initial.contains("Project instruction after reload."));

        write_file(
            &workspace.join(PROJECT_MEMORY_RELATIVE_PATH),
            "Project instruction after reload.",
        );

        memory.reload().expect("memory should reload");
        let reloaded = memory
            .effective_context_block()
            .expect("effective instructions should exist after reload");

        assert!(reloaded.contains("Initial user instruction."));
        assert!(reloaded.contains("Project instruction after reload."));
    }

    #[test]
    fn init_project_file_supports_force_overwrite() {
        let root = temp_root("meow-memory-init");
        let workspace = root.join("workspace");
        let user_memory = root.join("user-memory");

        fs::create_dir_all(workspace.join(".git")).expect("git dir should exist");

        let cfg = test_config(&user_memory);
        let mut memory = InstructionMemory::load(&cfg, &workspace).expect("memory should load");

        let created = memory
            .init_project_file(false)
            .expect("initial project file creation should succeed");
        assert_eq!(created.outcome, InitProjectOutcome::Created);
        assert!(created.path.exists());

        fs::write(&created.path, "custom project instructions")
            .expect("custom project instructions should be written");

        let exists = memory
            .init_project_file(false)
            .expect("second init without force should succeed with already-exists outcome");
        assert_eq!(exists.outcome, InitProjectOutcome::AlreadyExists);

        let unchanged = fs::read_to_string(&created.path).expect("project file should be readable");
        assert_eq!(unchanged, "custom project instructions");

        let overwritten = memory
            .init_project_file(true)
            .expect("forced init should overwrite project instructions");
        assert_eq!(overwritten.outcome, InitProjectOutcome::Overwritten);

        let refreshed = fs::read_to_string(&created.path).expect("project file should be readable");
        assert!(refreshed.contains("Meow Soma Project Instructions"));
    }

    #[cfg(unix)]
    #[test]
    fn load_rejects_symlinked_instruction_files() {
        use std::os::unix::fs::symlink;

        let root = temp_root("meow-memory-symlink-read");
        let workspace = root.join("workspace");
        let user_memory = root.join("user-memory");
        let target = root.join("sensitive.txt");

        fs::create_dir_all(workspace.join(".git")).expect("git dir should exist");
        write_file(&target, "secret");
        if let Some(parent) = workspace.join(PROJECT_MEMORY_RELATIVE_PATH).parent() {
            fs::create_dir_all(parent).expect("project memory dir should exist");
        }
        symlink(&target, workspace.join(PROJECT_MEMORY_RELATIVE_PATH))
            .expect("symlink should be created");

        let cfg = test_config(&user_memory);
        let err = InstructionMemory::load(&cfg, &workspace)
            .expect_err("symlinked instruction path should be rejected");
        assert!(err.to_string().contains("symlinked instruction file"));
    }

    #[cfg(unix)]
    #[test]
    fn init_force_rejects_symlinked_project_file() {
        use std::os::unix::fs::symlink;

        let root = temp_root("meow-memory-symlink-write");
        let workspace = root.join("workspace");
        let user_memory = root.join("user-memory");
        let target = root.join("target.txt");

        fs::create_dir_all(workspace.join(".git")).expect("git dir should exist");
        let cfg = test_config(&user_memory);
        let mut memory = InstructionMemory::load(&cfg, &workspace).expect("memory should load");

        write_file(&target, "do-not-overwrite");
        if let Some(parent) = workspace.join(PROJECT_MEMORY_RELATIVE_PATH).parent() {
            fs::create_dir_all(parent).expect("project memory dir should exist");
        }
        symlink(&target, workspace.join(PROJECT_MEMORY_RELATIVE_PATH))
            .expect("symlink should be created");

        let err = memory
            .init_project_file(true)
            .expect_err("force init should reject symlink target");
        assert!(err.to_string().contains("symlinked instruction file"));
        let content = fs::read_to_string(&target).expect("target should remain readable");
        assert_eq!(content, "do-not-overwrite");
    }

    #[cfg(unix)]
    #[test]
    fn init_force_rejects_broken_symlinked_project_file() {
        use std::os::unix::fs::symlink;

        let root = temp_root("meow-memory-broken-symlink");
        let workspace = root.join("workspace");
        let user_memory = root.join("user-memory");
        let missing_target = root.join("missing-target.txt");

        fs::create_dir_all(workspace.join(".git")).expect("git dir should exist");
        let cfg = test_config(&user_memory);
        let mut memory = InstructionMemory::load(&cfg, &workspace).expect("memory should load");

        if let Some(parent) = workspace.join(PROJECT_MEMORY_RELATIVE_PATH).parent() {
            fs::create_dir_all(parent).expect("project memory dir should exist");
        }
        symlink(
            &missing_target,
            workspace.join(PROJECT_MEMORY_RELATIVE_PATH),
        )
        .expect("broken symlink should be created");

        let err = memory
            .init_project_file(true)
            .expect_err("force init should reject broken symlink");
        assert!(err.to_string().contains("symlinked instruction file"));
    }

    #[test]
    fn init_force_on_missing_file_reports_created() {
        let root = temp_root("meow-memory-init-force-create");
        let workspace = root.join("workspace");
        let user_memory = root.join("user-memory");

        fs::create_dir_all(workspace.join(".git")).expect("git dir should exist");

        let cfg = test_config(&user_memory);
        let mut memory = InstructionMemory::load(&cfg, &workspace).expect("memory should load");
        let result = memory
            .init_project_file(true)
            .expect("force init should create when file is absent");

        assert_eq!(result.outcome, InitProjectOutcome::Created);
        assert!(result.path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn load_rejects_project_scope_via_symlinked_parent_directory() {
        use std::os::unix::fs::symlink;

        let root = temp_root("meow-memory-parent-symlink-read");
        let workspace = root.join("workspace");
        let user_memory = root.join("user-memory");
        let redirected = root.join("redirected-memory");

        fs::create_dir_all(workspace.join(".git")).expect("git dir should exist");
        fs::create_dir_all(&redirected).expect("redirected directory should exist");
        write_file(
            &redirected.join("instructions.md"),
            "outside-of-project-root instructions",
        );
        symlink(&redirected, workspace.join(".meow-soma"))
            .expect("project memory directory symlink should be created");

        let cfg = test_config(&user_memory);
        let err = InstructionMemory::load(&cfg, &workspace)
            .expect_err("symlinked parent directory should be rejected");
        assert!(err.to_string().contains("symlinked directory"));
    }

    #[cfg(unix)]
    #[test]
    fn init_force_rejects_project_scope_via_symlinked_parent_directory() {
        use std::os::unix::fs::symlink;

        let root = temp_root("meow-memory-parent-symlink-write");
        let workspace = root.join("workspace");
        let user_memory = root.join("user-memory");
        let redirected = root.join("redirected-memory");
        let redirected_instruction = redirected.join("instructions.md");

        fs::create_dir_all(workspace.join(".git")).expect("git dir should exist");
        let cfg = test_config(&user_memory);
        let mut memory = InstructionMemory::load(&cfg, &workspace).expect("memory should load");

        fs::create_dir_all(&redirected).expect("redirected directory should exist");
        write_file(&redirected_instruction, "do-not-overwrite");
        symlink(&redirected, workspace.join(".meow-soma"))
            .expect("project memory directory symlink should be created");

        let err = memory
            .init_project_file(true)
            .expect_err("force init should reject symlinked parent directory");
        let err_text = err.to_string();
        assert!(
            err_text.contains("symlinked directory") || err_text.contains("outside project root"),
            "unexpected init error: {err_text}"
        );
        let content = fs::read_to_string(&redirected_instruction)
            .expect("redirected instruction should stay");
        assert_eq!(content, "do-not-overwrite");
    }
}
