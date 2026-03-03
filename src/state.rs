use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{Connection, params};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct SessionRecord {
    pub id: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct MessageRecord {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

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

        let store = Self { conn };
        store.init_schema()?;
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

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
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

        self.ensure_column("approvals", "reason_code", "TEXT NOT NULL DEFAULT ''")?;

        Ok(())
    }

    fn ensure_column(&self, table: &str, column: &str, definition: &str) -> Result<()> {
        let statement = format!("ALTER TABLE {table} ADD COLUMN {column} {definition}");
        match self.conn.execute(&statement, []) {
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
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_approval_persists_reason_code() {
        let db_path = std::env::temp_dir().join(format!("meow-state-{}.db", Uuid::new_v4()));
        let store = StateStore::open(&db_path).expect("state store should open");

        store
            .record_approval(
                "fs.write",
                "required",
                "risky_tool",
                "risky tool requires approval",
                false,
            )
            .expect("approval should be persisted");

        let mut stmt = store
            .conn
            .prepare("SELECT reason_code FROM approvals ORDER BY id DESC LIMIT 1")
            .expect("query should prepare");
        let code: String = stmt
            .query_row([], |row| row.get(0))
            .expect("row should exist");
        assert_eq!(code, "risky_tool");
    }

    #[test]
    fn migrates_existing_approvals_table_with_reason_code_column() {
        let db_path =
            std::env::temp_dir().join(format!("meow-state-migrate-{}.db", Uuid::new_v4()));
        let conn = Connection::open(&db_path).expect("db should open");
        conn.execute_batch(
            r#"
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
        .expect("legacy approvals schema should be created");
        drop(conn);

        let store = StateStore::open(&db_path).expect("state store should migrate schema");
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
}
