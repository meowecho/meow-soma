use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::{Duration, Utc};
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const LATEST_SCHEMA_VERSION: i64 = 4;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub id: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct MessageRecord {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct RunRecord {
    pub id: String,
    pub goal: String,
    pub output: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ToolCallRecord {
    pub id: i64,
    pub tool_name: String,
    pub args: String,
    pub status: String,
    pub output: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ApprovalRecord {
    pub id: i64,
    pub action: String,
    pub decision: String,
    #[serde(default)]
    pub reason_code: String,
    pub reason: String,
    pub approved: bool,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ArtifactRecord {
    pub id: String,
    pub artifact_type: String,
    pub path: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TelemetryEventRecord {
    pub id: i64,
    pub metric: String,
    pub operation: String,
    pub provider: String,
    pub model: String,
    pub latency_ms: i64,
    pub success: bool,
    pub error_kind: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TelemetryLatencySummary {
    pub count: u64,
    pub avg_ms: u64,
    pub p50_ms: u64,
    pub p95_ms: u64,
    pub min_ms: u64,
    pub max_ms: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TelemetryErrorSummary {
    pub kind: String,
    pub count: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TelemetrySummary {
    pub window_days: u32,
    pub from: String,
    pub to: String,
    pub total_events: u64,
    pub startup: TelemetryLatencySummary,
    pub response: TelemetryLatencySummary,
    pub response_failures: u64,
    pub error_categories: Vec<TelemetryErrorSummary>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct StateSnapshot {
    pub schema_version: i64,
    pub exported_at: String,
    pub sessions: Vec<SessionRecord>,
    pub messages: Vec<MessageRecord>,
    #[serde(default)]
    pub runs: Vec<RunRecord>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCallRecord>,
    #[serde(default)]
    pub approvals: Vec<ApprovalRecord>,
    #[serde(default)]
    pub artifacts: Vec<ArtifactRecord>,
    #[serde(default)]
    pub telemetry_events: Vec<TelemetryEventRecord>,
}

#[derive(Debug)]
pub struct StateStore {
    conn: Connection,
}

impl StateStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed creating state directory: {}", parent.display())
            })?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("failed opening sqlite db: {}", path.display()))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .context("failed enabling sqlite foreign_keys")?;
        verify_integrity(&conn, path)?;

        let store = Self { conn };
        store.run_migrations()?;
        Ok(store)
    }

    pub fn create_session(&self, title: Option<&str>) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = now_rfc3339();

        self.conn.execute(
            "INSERT INTO sessions (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, title, now, now],
        )?;

        Ok(id)
    }

    pub fn add_message(&self, session_id: &str, role: &str, content: &str) -> Result<()> {
        let now = now_rfc3339();

        self.conn.execute(
            "INSERT INTO messages (session_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![session_id, role, content, now],
        )?;

        self.conn.execute(
            "UPDATE sessions SET updated_at = ?2 WHERE id = ?1",
            params![session_id, now],
        )?;

        Ok(())
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, created_at, updated_at FROM sessions ORDER BY updated_at DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(SessionRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?;

        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row?);
        }

        Ok(sessions)
    }

    pub fn get_messages(&self, session_id: &str) -> Result<Vec<MessageRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, role, content, created_at FROM messages WHERE session_id = ?1 ORDER BY id ASC",
        )?;

        let rows = stmt.query_map(params![session_id], |row| {
            Ok(MessageRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }

        Ok(messages)
    }

    pub fn get_recent_messages(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<MessageRecord>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, role, content, created_at FROM messages WHERE session_id = ?1 ORDER BY id DESC LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![session_id, limit as i64], |row| {
            Ok(MessageRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }

        messages.reverse();
        Ok(messages)
    }

    pub fn record_run(&self, goal: &str, output: &str, status: &str) -> Result<String> {
        let run_id = Uuid::new_v4().to_string();
        let now = now_rfc3339();
        self.conn.execute(
            "INSERT INTO runs (id, goal, output, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![run_id, goal, output, status, now],
        )?;
        Ok(run_id)
    }

    pub fn record_tool_call(
        &self,
        tool_name: &str,
        args: &str,
        status: &str,
        output: &str,
    ) -> Result<()> {
        let now = now_rfc3339();
        self.conn.execute(
            "INSERT INTO tool_calls (tool_name, args, status, output, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![tool_name, args, status, output, now],
        )?;
        Ok(())
    }

    pub fn record_approval(
        &self,
        action: &str,
        decision: &str,
        reason_code: &str,
        reason: &str,
        approved: bool,
    ) -> Result<()> {
        let now = now_rfc3339();
        self.conn.execute(
            "INSERT INTO approvals (action, decision, reason_code, reason, approved, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![action, decision, reason_code, reason, approved, now],
        )?;
        Ok(())
    }

    pub fn record_startup_latency(&self, mode: &str, latency_ms: u128) -> Result<()> {
        self.record_telemetry_event("startup", mode, "", "", latency_ms, true, "")
    }

    pub fn record_response_latency(
        &self,
        operation: &str,
        provider: &str,
        model: &str,
        latency_ms: u128,
        success: bool,
        error_kind: Option<&str>,
    ) -> Result<()> {
        self.record_telemetry_event(
            "response",
            operation,
            provider,
            model,
            latency_ms,
            success,
            error_kind.unwrap_or(""),
        )
    }

    pub fn telemetry_summary(&self, days: u32) -> Result<TelemetrySummary> {
        let window_days = days.max(1);
        let now = Utc::now();
        let since = now - Duration::days(window_days as i64);
        let from = since.to_rfc3339();
        let to = now.to_rfc3339();

        let mut stmt = self.conn.prepare(
            "SELECT id, metric, operation, provider, model, latency_ms, success, error_kind, created_at \
             FROM telemetry_events \
             WHERE created_at >= ?1 \
             ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![from], |row| {
            Ok(TelemetryEventRecord {
                id: row.get(0)?,
                metric: row.get(1)?,
                operation: row.get(2)?,
                provider: row.get(3)?,
                model: row.get(4)?,
                latency_ms: row.get(5)?,
                success: row.get(6)?,
                error_kind: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }

        let mut startup_latencies = Vec::new();
        let mut response_latencies = Vec::new();
        let mut response_failures = 0_u64;
        let mut error_counts: BTreeMap<String, u64> = BTreeMap::new();

        for event in &events {
            if event.metric == "startup" {
                startup_latencies.push(event.latency_ms);
            }
            if event.metric == "response" {
                response_latencies.push(event.latency_ms);
                if !event.success {
                    response_failures += 1;
                }
            }

            if !event.error_kind.is_empty() {
                *error_counts.entry(event.error_kind.clone()).or_insert(0) += 1;
            }
        }

        let error_categories = error_counts
            .into_iter()
            .map(|(kind, count)| TelemetryErrorSummary { kind, count })
            .collect();

        Ok(TelemetrySummary {
            window_days,
            from,
            to,
            total_events: events.len() as u64,
            startup: compute_latency_summary(&startup_latencies),
            response: compute_latency_summary(&response_latencies),
            response_failures,
            error_categories,
        })
    }

    pub fn export_snapshot(&self) -> Result<StateSnapshot> {
        Ok(StateSnapshot {
            schema_version: LATEST_SCHEMA_VERSION,
            exported_at: now_rfc3339(),
            sessions: self.list_all_sessions()?,
            messages: self.list_all_messages()?,
            runs: self.list_runs()?,
            tool_calls: self.list_tool_calls()?,
            approvals: self.list_approvals()?,
            artifacts: self.list_artifacts()?,
            telemetry_events: self.list_telemetry_events()?,
        })
    }

    pub fn export_snapshot_json(&self) -> Result<String> {
        let snapshot = self.export_snapshot()?;
        serde_json::to_string_pretty(&snapshot).context("failed serializing state snapshot")
    }

    pub fn import_snapshot_json(&self, payload: &str) -> Result<()> {
        let snapshot: StateSnapshot = serde_json::from_str(payload)
            .context("invalid backup payload; expected JSON from `meow session export --all`")?;
        self.import_snapshot(&snapshot)
    }

    pub fn import_snapshot(&self, snapshot: &StateSnapshot) -> Result<()> {
        if snapshot.schema_version > LATEST_SCHEMA_VERSION {
            bail!(
                "backup schema version {} is newer than this binary (supported: {}); upgrade meow-soma before importing",
                snapshot.schema_version,
                LATEST_SCHEMA_VERSION
            );
        }

        let tx = self.conn.unchecked_transaction()?;
        tx.execute_batch(
            r#"
            DELETE FROM messages;
            DELETE FROM sessions;
            DELETE FROM runs;
            DELETE FROM tool_calls;
            DELETE FROM approvals;
            DELETE FROM artifacts;
            DELETE FROM telemetry_events;
            "#,
        )?;

        for session in &snapshot.sessions {
            tx.execute(
                "INSERT INTO sessions (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
                params![
                    session.id,
                    session.title,
                    session.created_at,
                    session.updated_at
                ],
            )?;
        }

        for message in &snapshot.messages {
            tx.execute(
                "INSERT INTO messages (id, session_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    message.id,
                    message.session_id,
                    message.role,
                    message.content,
                    message.created_at
                ],
            )?;
        }

        for run in &snapshot.runs {
            tx.execute(
                "INSERT INTO runs (id, goal, output, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![run.id, run.goal, run.output, run.status, run.created_at],
            )?;
        }

        for tool_call in &snapshot.tool_calls {
            tx.execute(
                "INSERT INTO tool_calls (id, tool_name, args, status, output, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    tool_call.id,
                    tool_call.tool_name,
                    tool_call.args,
                    tool_call.status,
                    tool_call.output,
                    tool_call.created_at
                ],
            )?;
        }

        for approval in &snapshot.approvals {
            tx.execute(
                "INSERT INTO approvals (id, action, decision, reason_code, reason, approved, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    approval.id,
                    approval.action,
                    approval.decision,
                    approval.reason_code,
                    approval.reason,
                    approval.approved,
                    approval.created_at
                ],
            )?;
        }

        for artifact in &snapshot.artifacts {
            tx.execute(
                "INSERT INTO artifacts (id, artifact_type, path, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![
                    artifact.id,
                    artifact.artifact_type,
                    artifact.path,
                    artifact.created_at
                ],
            )?;
        }

        for event in &snapshot.telemetry_events {
            tx.execute(
                "INSERT INTO telemetry_events (id, metric, operation, provider, model, latency_ms, success, error_kind, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    event.id,
                    event.metric,
                    event.operation,
                    event.provider,
                    event.model,
                    event.latency_ms,
                    event.success,
                    event.error_kind,
                    event.created_at
                ],
            )?;
        }

        tx.commit().context("failed importing state snapshot")
    }

    fn record_telemetry_event(
        &self,
        metric: &str,
        operation: &str,
        provider: &str,
        model: &str,
        latency_ms: u128,
        success: bool,
        error_kind: &str,
    ) -> Result<()> {
        let now = now_rfc3339();
        self.conn.execute(
            "INSERT INTO telemetry_events (metric, operation, provider, model, latency_ms, success, error_kind, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                metric,
                operation,
                provider,
                model,
                clamp_latency_to_i64(latency_ms),
                success,
                error_kind,
                now
            ],
        )?;
        Ok(())
    }

    fn run_migrations(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );
            "#,
        )?;

        let current = self.current_schema_version()?;
        if current > LATEST_SCHEMA_VERSION {
            bail!(
                "database schema version {} is newer than this binary (supported: {}); upgrade meow-soma",
                current,
                LATEST_SCHEMA_VERSION
            );
        }

        for migration in schema_migrations() {
            if migration.version <= current {
                continue;
            }

            let tx = self.conn.unchecked_transaction()?;
            (migration.apply)(&tx).with_context(|| {
                format!("failed running schema migration v{}", migration.version)
            })?;
            tx.execute(
                "INSERT OR IGNORE INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                params![migration.version, now_rfc3339()],
            )?;
            tx.commit().with_context(|| {
                format!("failed committing schema migration v{}", migration.version)
            })?;
        }

        let applied = self.current_schema_version()?;
        if applied < LATEST_SCHEMA_VERSION {
            bail!(
                "schema migration incomplete: expected version {}, got {}",
                LATEST_SCHEMA_VERSION,
                applied
            );
        }

        Ok(())
    }

    fn current_schema_version(&self) -> Result<i64> {
        let version = self.conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )?;
        Ok(version)
    }

    fn list_all_sessions(&self) -> Result<Vec<SessionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, created_at, updated_at FROM sessions ORDER BY created_at ASC, id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SessionRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?;

        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row?);
        }
        Ok(sessions)
    }

    fn list_all_messages(&self) -> Result<Vec<MessageRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, role, content, created_at FROM messages ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(MessageRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }
        Ok(messages)
    }

    fn list_runs(&self) -> Result<Vec<RunRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, goal, output, status, created_at FROM runs ORDER BY created_at ASC, id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(RunRecord {
                id: row.get(0)?,
                goal: row.get(1)?,
                output: row.get(2)?,
                status: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;

        let mut runs = Vec::new();
        for row in rows {
            runs.push(row?);
        }
        Ok(runs)
    }

    fn list_tool_calls(&self) -> Result<Vec<ToolCallRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, tool_name, args, status, output, created_at FROM tool_calls ORDER BY id ASC")?;
        let rows = stmt.query_map([], |row| {
            Ok(ToolCallRecord {
                id: row.get(0)?,
                tool_name: row.get(1)?,
                args: row.get(2)?,
                status: row.get(3)?,
                output: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;

        let mut calls = Vec::new();
        for row in rows {
            calls.push(row?);
        }
        Ok(calls)
    }

    fn list_approvals(&self) -> Result<Vec<ApprovalRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, action, decision, reason_code, reason, approved, created_at FROM approvals ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ApprovalRecord {
                id: row.get(0)?,
                action: row.get(1)?,
                decision: row.get(2)?,
                reason_code: row.get(3)?,
                reason: row.get(4)?,
                approved: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;

        let mut approvals = Vec::new();
        for row in rows {
            approvals.push(row?);
        }
        Ok(approvals)
    }

    fn list_artifacts(&self) -> Result<Vec<ArtifactRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, artifact_type, path, created_at FROM artifacts ORDER BY created_at ASC, id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ArtifactRecord {
                id: row.get(0)?,
                artifact_type: row.get(1)?,
                path: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        let mut artifacts = Vec::new();
        for row in rows {
            artifacts.push(row?);
        }
        Ok(artifacts)
    }

    fn list_telemetry_events(&self) -> Result<Vec<TelemetryEventRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, metric, operation, provider, model, latency_ms, success, error_kind, created_at \
             FROM telemetry_events ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(TelemetryEventRecord {
                id: row.get(0)?,
                metric: row.get(1)?,
                operation: row.get(2)?,
                provider: row.get(3)?,
                model: row.get(4)?,
                latency_ms: row.get(5)?,
                success: row.get(6)?,
                error_kind: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }
}

#[derive(Clone, Copy)]
struct Migration {
    version: i64,
    apply: fn(&Transaction<'_>) -> Result<()>,
}

fn schema_migrations() -> [Migration; LATEST_SCHEMA_VERSION as usize] {
    [
        Migration {
            version: 1,
            apply: migration_v1,
        },
        Migration {
            version: 2,
            apply: migration_v2,
        },
        Migration {
            version: 3,
            apply: migration_v3,
        },
        Migration {
            version: 4,
            apply: migration_v4,
        },
    ]
}

fn migration_v1(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            title TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(session_id) REFERENCES sessions(id)
        );

        CREATE TABLE IF NOT EXISTS runs (
            id TEXT PRIMARY KEY,
            goal TEXT NOT NULL,
            output TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS tool_calls (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_name TEXT NOT NULL,
            args TEXT NOT NULL,
            status TEXT NOT NULL,
            output TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS approvals (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            action TEXT NOT NULL,
            decision TEXT NOT NULL,
            reason_code TEXT NOT NULL DEFAULT '',
            reason TEXT NOT NULL,
            approved INTEGER NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS artifacts (
            id TEXT PRIMARY KEY,
            artifact_type TEXT NOT NULL,
            path TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        "#,
    )?;
    Ok(())
}

fn migration_v2(tx: &Transaction<'_>) -> Result<()> {
    ensure_column(tx, "approvals", "reason_code", "TEXT NOT NULL DEFAULT ''")
}

fn migration_v3(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_sessions_updated_at ON sessions(updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_messages_session_id_id ON messages(session_id, id DESC);
        CREATE INDEX IF NOT EXISTS idx_runs_created_at ON runs(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_tool_calls_created_at ON tool_calls(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_approvals_created_at ON approvals(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_artifacts_created_at ON artifacts(created_at DESC);
        "#,
    )?;
    Ok(())
}

fn migration_v4(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS telemetry_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            metric TEXT NOT NULL,
            operation TEXT NOT NULL,
            provider TEXT NOT NULL,
            model TEXT NOT NULL,
            latency_ms INTEGER NOT NULL,
            success INTEGER NOT NULL,
            error_kind TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_telemetry_created_at ON telemetry_events(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_telemetry_metric_created_at ON telemetry_events(metric, created_at DESC);
        "#,
    )?;
    Ok(())
}

fn ensure_column(tx: &Transaction<'_>, table: &str, column: &str, definition: &str) -> Result<()> {
    let statement = format!("ALTER TABLE {table} ADD COLUMN {column} {definition}");
    match tx.execute(&statement, []) {
        Ok(_) => Ok(()),
        Err(err) => {
            let message = err.to_string().to_ascii_lowercase();
            if message.contains("duplicate column name") {
                Ok(())
            } else {
                Err(err.into())
            }
        }
    }
}

fn verify_integrity(conn: &Connection, path: &Path) -> Result<()> {
    match conn.query_row("PRAGMA quick_check(1)", [], |row| row.get::<_, String>(0)) {
        Ok(result) if result.eq_ignore_ascii_case("ok") => Ok(()),
        Ok(result) => bail!("{}", corruption_recovery_message(path, &result)),
        Err(err) => bail!(
            "{}",
            corruption_recovery_message(path, &format!("quick_check failed: {err}"))
        ),
    }
}

fn corruption_recovery_message(path: &Path, detail: &str) -> String {
    format!(
        "sqlite integrity check failed for {} ({detail}). The state DB may be corrupted.\n\
         Recovery steps:\n\
           1) Move the corrupted DB aside: mv {} {}.corrupt\n\
           2) Restore from backup JSON: meow session import <backup.json>\n\
           3) If no backup exists, delete the DB file and let meow recreate it on next run.",
        path.display(),
        path.display(),
        path.display()
    )
}

fn clamp_latency_to_i64(value: u128) -> i64 {
    value.min(i64::MAX as u128) as i64
}

fn compute_latency_summary(latencies: &[i64]) -> TelemetryLatencySummary {
    if latencies.is_empty() {
        return TelemetryLatencySummary {
            count: 0,
            avg_ms: 0,
            p50_ms: 0,
            p95_ms: 0,
            min_ms: 0,
            max_ms: 0,
        };
    }

    let mut values = latencies
        .iter()
        .map(|value| (*value).max(0) as u64)
        .collect::<Vec<_>>();
    values.sort_unstable();

    let count = values.len() as u64;
    let sum = values.iter().sum::<u64>();
    let min = values.first().copied().unwrap_or(0);
    let max = values.last().copied().unwrap_or(0);
    let p50 = percentile_value(&values, 50);
    let p95 = percentile_value(&values, 95);

    TelemetryLatencySummary {
        count,
        avg_ms: if count == 0 { 0 } else { sum / count },
        p50_ms: p50,
        p95_ms: p95,
        min_ms: min,
        max_ms: max,
    }
}

fn percentile_value(sorted_values: &[u64], percentile: usize) -> u64 {
    if sorted_values.is_empty() {
        return 0;
    }

    let count = sorted_values.len();
    let rank = (count * percentile).div_ceil(100);
    let index = rank.saturating_sub(1).min(count.saturating_sub(1));
    sorted_values[index]
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_db_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}.db", Uuid::new_v4()))
    }

    fn create_legacy_schema(path: &Path) {
        let conn = Connection::open(path).expect("legacy db should open");
        conn.execute_batch(
            r#"
            CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                title TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE approvals (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                action TEXT NOT NULL,
                decision TEXT NOT NULL,
                reason TEXT NOT NULL,
                approved INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );
            "#,
        )
        .expect("legacy schema should be created");
    }

    fn index_names(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='index'")
            .expect("index query should prepare");
        let rows = stmt
            .query_map([], |row| row.get(0))
            .expect("index query should run");

        let mut names = Vec::new();
        for row in rows {
            names.push(row.expect("index row should parse"));
        }
        names
    }

    #[test]
    fn migrations_apply_on_fresh_database() {
        let db_path = temp_db_path("meow-state-fresh");
        let store = StateStore::open(&db_path).expect("state store should open");

        let version = store
            .current_schema_version()
            .expect("schema version should be readable");
        assert_eq!(version, LATEST_SCHEMA_VERSION);

        let count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("migration rows should be queryable");
        assert_eq!(count, LATEST_SCHEMA_VERSION);
    }

    #[test]
    fn migrates_legacy_database_and_adds_reason_code_column() {
        let db_path = temp_db_path("meow-state-legacy");
        create_legacy_schema(&db_path);

        let store = StateStore::open(&db_path).expect("legacy db should migrate");
        let version = store
            .current_schema_version()
            .expect("schema version should be queryable");
        assert_eq!(version, LATEST_SCHEMA_VERSION);

        store
            .record_approval(
                "shell",
                "approved",
                "outside_allowlist",
                "approved manually",
                true,
            )
            .expect("insert should include reason_code");

        let mut stmt = store
            .conn
            .prepare("SELECT reason_code FROM approvals ORDER BY id DESC LIMIT 1")
            .expect("query should prepare");
        let code: String = stmt
            .query_row([], |row| row.get(0))
            .expect("row should exist");
        assert_eq!(code, "outside_allowlist");
    }

    #[test]
    fn migrations_are_idempotent() {
        let db_path = temp_db_path("meow-state-idempotent");
        create_legacy_schema(&db_path);

        StateStore::open(&db_path).expect("first migration pass should succeed");
        StateStore::open(&db_path).expect("second migration pass should succeed");

        let conn = Connection::open(&db_path).expect("db should open");
        let (count, distinct): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COUNT(DISTINCT version) FROM schema_migrations",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("migration stats should be queryable");

        assert_eq!(count, LATEST_SCHEMA_VERSION);
        assert_eq!(distinct, LATEST_SCHEMA_VERSION);
    }

    #[test]
    fn rejects_database_newer_than_supported_schema() {
        let db_path = temp_db_path("meow-state-future-schema");
        let conn = Connection::open(&db_path).expect("db should open");
        conn.execute_batch(
            r#"
            CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );
            INSERT INTO schema_migrations (version, applied_at) VALUES (99, '2026-01-01T00:00:00Z');
            "#,
        )
        .expect("future schema marker should be inserted");

        let err = StateStore::open(&db_path).expect_err("open should reject future schema");
        let message = err.to_string();
        assert!(message.contains("newer than this binary"));
        assert!(message.contains("supported"));
    }

    #[test]
    fn creates_indexes_for_hot_paths() {
        let db_path = temp_db_path("meow-state-indexes");
        let store = StateStore::open(&db_path).expect("state store should open");
        let names = index_names(&store.conn);

        for required in [
            "idx_sessions_updated_at",
            "idx_messages_session_id_id",
            "idx_runs_created_at",
            "idx_tool_calls_created_at",
            "idx_approvals_created_at",
            "idx_artifacts_created_at",
            "idx_telemetry_created_at",
        ] {
            assert!(
                names.contains(&required.to_owned()),
                "missing expected index: {required}"
            );
        }
    }

    #[test]
    fn corruption_errors_include_recovery_steps() {
        let db_path = temp_db_path("meow-state-corrupt");
        fs::write(&db_path, b"this is not sqlite").expect("corrupt file should be created");

        let err = StateStore::open(&db_path).expect_err("open should fail");
        let message = err.to_string();
        assert!(message.contains("quick_check"));
        assert!(message.contains("meow session import"));
    }

    #[test]
    fn snapshot_export_import_roundtrip() {
        let source_db = temp_db_path("meow-state-source");
        let target_db = temp_db_path("meow-state-target");

        let source = StateStore::open(&source_db).expect("source state store should open");
        let session_id = source
            .create_session(Some("snapshot test"))
            .expect("session should be created");
        source
            .add_message(&session_id, "user", "hello")
            .expect("user message should be inserted");
        source
            .add_message(&session_id, "assistant", "world")
            .expect("assistant message should be inserted");
        source
            .record_run("goal", "run output", "ok")
            .expect("run should be recorded");
        source
            .record_tool_call("echo", "--hello", "ok", "hello")
            .expect("tool call should be recorded");
        source
            .record_approval("shell", "approved", "manual", "approved manually", true)
            .expect("approval should be recorded");
        source
            .conn
            .execute(
                "INSERT INTO artifacts (id, artifact_type, path, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![
                    Uuid::new_v4().to_string(),
                    "log",
                    "/tmp/artifact.log",
                    now_rfc3339()
                ],
            )
            .expect("artifact should be inserted");
        source
            .record_startup_latency("ask", 120)
            .expect("startup telemetry should be recorded");
        source
            .record_response_latency("ask", "openai", "gpt-4.1", 430, false, Some("timeout"))
            .expect("response telemetry should be recorded");

        let exported_json = source
            .export_snapshot_json()
            .expect("snapshot should export to json");
        let expected: StateSnapshot =
            serde_json::from_str(&exported_json).expect("snapshot json should parse");

        let target = StateStore::open(&target_db).expect("target state store should open");
        target
            .import_snapshot_json(&exported_json)
            .expect("snapshot should import");

        let actual = target
            .export_snapshot()
            .expect("snapshot should export after import");

        assert_eq!(actual.schema_version, expected.schema_version);
        assert_eq!(actual.sessions, expected.sessions);
        assert_eq!(actual.messages, expected.messages);
        assert_eq!(actual.runs, expected.runs);
        assert_eq!(actual.tool_calls, expected.tool_calls);
        assert_eq!(actual.approvals, expected.approvals);
        assert_eq!(actual.artifacts, expected.artifacts);
        assert_eq!(actual.telemetry_events, expected.telemetry_events);
    }

    #[test]
    fn import_snapshot_accepts_legacy_shape_with_missing_new_fields() {
        let db_path = temp_db_path("meow-state-legacy-snapshot");
        let store = StateStore::open(&db_path).expect("state store should open");

        let legacy_json = r#"
        {
          "schema_version": 1,
          "exported_at": "2026-03-03T00:00:00Z",
          "sessions": [
            {
              "id": "session-1",
              "title": "legacy",
              "created_at": "2026-03-03T00:00:00Z",
              "updated_at": "2026-03-03T00:00:00Z"
            }
          ],
          "messages": [
            {
              "id": 1,
              "session_id": "session-1",
              "role": "user",
              "content": "hello from legacy backup",
              "created_at": "2026-03-03T00:00:00Z"
            }
          ]
        }
        "#;

        store
            .import_snapshot_json(legacy_json)
            .expect("legacy snapshot should import");

        let snapshot = store
            .export_snapshot()
            .expect("snapshot should export after import");
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.messages.len(), 1);
        assert!(snapshot.runs.is_empty());
        assert!(snapshot.tool_calls.is_empty());
        assert!(snapshot.approvals.is_empty());
        assert!(snapshot.artifacts.is_empty());
        assert!(snapshot.telemetry_events.is_empty());
    }

    #[test]
    fn telemetry_summary_aggregates_latencies_and_error_categories() {
        let db_path = temp_db_path("meow-state-telemetry-summary");
        let store = StateStore::open(&db_path).expect("state store should open");

        store
            .record_startup_latency("tui", 90)
            .expect("startup telemetry should be recorded");
        store
            .record_startup_latency("ask", 110)
            .expect("startup telemetry should be recorded");
        store
            .record_response_latency("ask", "openai", "gpt-4.1", 300, true, None)
            .expect("response telemetry should be recorded");
        store
            .record_response_latency("run", "openai", "gpt-4.1", 450, false, Some("timeout"))
            .expect("response telemetry should be recorded");
        store
            .record_response_latency("chat", "anthropic", "claude", 210, true, None)
            .expect("response telemetry should be recorded");

        let summary = store
            .telemetry_summary(7)
            .expect("telemetry summary should be computed");

        assert_eq!(summary.window_days, 7);
        assert_eq!(summary.total_events, 5);
        assert_eq!(summary.startup.count, 2);
        assert_eq!(summary.startup.p95_ms, 110);
        assert_eq!(summary.response.count, 3);
        assert_eq!(summary.response_failures, 1);
        assert_eq!(summary.response.p95_ms, 450);
        assert_eq!(summary.error_categories.len(), 1);
        assert_eq!(summary.error_categories[0].kind, "timeout");
        assert_eq!(summary.error_categories[0].count, 1);
    }

    #[test]
    fn import_snapshot_rejects_structurally_invalid_payload_without_data_loss() {
        let db_path = temp_db_path("meow-state-invalid-shape");
        let store = StateStore::open(&db_path).expect("state store should open");

        let session_id = store
            .create_session(Some("baseline"))
            .expect("session should be created");
        store
            .add_message(&session_id, "user", "keep me")
            .expect("message should be stored");

        let baseline = store
            .export_snapshot()
            .expect("baseline snapshot should export");

        let err = store
            .import_snapshot_json("{}")
            .expect_err("invalid object shape should be rejected");
        assert!(err.to_string().contains("invalid backup payload"));

        let after = store
            .export_snapshot()
            .expect("post-failure snapshot should export");
        assert_eq!(after.sessions, baseline.sessions);
        assert_eq!(after.messages, baseline.messages);
    }

    #[test]
    fn import_rollback_preserves_existing_data_on_invalid_payload() {
        let db_path = temp_db_path("meow-state-invalid-import");
        let store = StateStore::open(&db_path).expect("state store should open");

        let session_id = store
            .create_session(Some("rollback"))
            .expect("session should be created");
        store
            .add_message(&session_id, "user", "hello")
            .expect("message should be stored");

        let baseline = store
            .export_snapshot()
            .expect("baseline snapshot should export");
        let mut invalid = baseline.clone();
        invalid.sessions.clear();
        let invalid_json = serde_json::to_string(&invalid).expect("invalid json should serialize");

        let err = store
            .import_snapshot_json(&invalid_json)
            .expect_err("import should fail");
        assert!(!err.to_string().is_empty());

        let after = store
            .export_snapshot()
            .expect("post-failure snapshot should export");
        assert_eq!(after.sessions, baseline.sessions);
        assert_eq!(after.messages, baseline.messages);
    }
}
