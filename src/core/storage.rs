use crate::core::model::{
    AccountRecord, ImageRecord, MigrationSummary, RequestData, SiteDocumentRecord, TaskRecord,
    TaskState, TaskStatus, WorkRecord,
};
use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use std::path::Path;
use std::time::Duration;

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path).context("failed to open sqlite database")?;
        conn.busy_timeout(Duration::from_secs(5))
            .context("failed to configure sqlite busy timeout")?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn =
            Connection::open_in_memory().context("failed to open in-memory sqlite database")?;
        conn.busy_timeout(Duration::from_secs(5))
            .context("failed to configure sqlite busy timeout")?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS tasks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                request_id TEXT NOT NULL,
                action TEXT NOT NULL,
                param TEXT NOT NULL,
                request_json TEXT NOT NULL,
                status TEXT NOT NULL,
                error TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS accounts (
                site TEXT NOT NULL,
                name TEXT NOT NULL,
                display_name TEXT,
                account_json TEXT NOT NULL,
                active INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(site, name)
            );

            CREATE TABLE IF NOT EXISTS works (
                site TEXT NOT NULL,
                work_key TEXT NOT NULL,
                title TEXT NOT NULL,
                author TEXT NOT NULL,
                author_id TEXT,
                author_url TEXT,
                type TEXT NOT NULL,
                serialization TEXT NOT NULL,
                caption TEXT NOT NULL,
                create_date TEXT NOT NULL,
                update_date TEXT NOT NULL,
                raw_json TEXT NOT NULL,
                PRIMARY KEY(site, work_key)
            );

            CREATE TABLE IF NOT EXISTS images (
                logical_name TEXT PRIMARY KEY,
                hash TEXT NOT NULL,
                ext TEXT NOT NULL,
                kind TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS site_documents (
                site TEXT NOT NULL,
                key TEXT NOT NULL,
                document_json TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(site, key)
            );

            CREATE TABLE IF NOT EXISTS migrations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_root TEXT NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                summary_json TEXT NOT NULL
            );
        "#,
        )?;
        Ok(())
    }

    pub fn enqueue_task(&self, task: &TaskRecord) -> Result<i64> {
        self.conn.execute(
            r#"INSERT INTO tasks (request_id, action, param, request_json, status, error, created_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
            params![
                task.request_id,
                task.action,
                task.param,
                serde_json::to_string(&task.request)?,
                task.status.to_string(),
                task.error,
                task.created_at,
                task.updated_at,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_tasks(&self) -> Result<Vec<TaskRecord>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, request_id, action, param, request_json, status, error, created_at, updated_at
               FROM tasks ORDER BY id ASC"#,
        )?;
        let rows = stmt.query_map([], task_record_from_row)?;
        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(row?);
        }
        Ok(tasks)
    }

    pub fn upsert_account(&self, account: &AccountRecord) -> Result<()> {
        self.conn.execute(
            r#"INSERT INTO accounts (site, name, display_name, account_json, active, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6)
               ON CONFLICT(site, name) DO UPDATE SET
                   display_name=excluded.display_name,
                   account_json=excluded.account_json,
                   active=excluded.active,
                   updated_at=excluded.updated_at"#,
            params![
                account.site,
                account.name,
                account.display_name,
                serde_json::to_string(&account.account)?,
                i64::from(account.active),
                account.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_accounts(&self, site: &str) -> Result<Vec<AccountRecord>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT site, name, display_name, account_json, active, updated_at
               FROM accounts WHERE site = ?1 ORDER BY active DESC, name ASC"#,
        )?;
        let rows = stmt.query_map([site], |row| {
            let account_json: String = row.get(3)?;
            let account: crate::core::model::AccountFile =
                serde_json::from_str(&account_json).unwrap_or_default();
            Ok(AccountRecord {
                site: row.get(0)?,
                name: row.get(1)?,
                display_name: row.get(2)?,
                account,
                active: row.get::<_, i64>(4)? != 0,
                updated_at: row.get(5)?,
            })
        })?;
        let mut accounts = Vec::new();
        for row in rows {
            accounts.push(row?);
        }
        Ok(accounts)
    }

    pub fn upsert_work(&self, work: &WorkRecord) -> Result<()> {
        self.conn.execute(
            r#"INSERT INTO works (site, work_key, title, author, author_id, author_url, type, serialization, caption, create_date, update_date, raw_json)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
               ON CONFLICT(site, work_key) DO UPDATE SET
                   title=excluded.title,
                   author=excluded.author,
                   author_id=excluded.author_id,
                   author_url=excluded.author_url,
                   type=excluded.type,
                   serialization=excluded.serialization,
                   caption=excluded.caption,
                   create_date=excluded.create_date,
                   update_date=excluded.update_date,
                   raw_json=excluded.raw_json"#,
            params![
                work.site,
                work.work_key,
                work.title,
                work.author,
                work.author_id,
                work.author_url,
                work.r#type,
                work.serialization,
                work.caption,
                work.create_date,
                work.update_date,
                serde_json::to_string(&work.raw_json)?,
            ],
        )?;
        Ok(())
    }

    pub fn list_works(&self, site: Option<&str>) -> Result<Vec<WorkRecord>> {
        let mut works = Vec::new();
        match site {
            Some(site) => {
                let mut stmt = self.conn.prepare(
                    r#"SELECT site, work_key, title, author, author_id, author_url, type, serialization, caption, create_date, update_date, raw_json
                       FROM works WHERE site = ?1 ORDER BY author, title"#,
                )?;
                let rows = stmt.query_map([site], |row| {
                    let raw_json: String = row.get(11)?;
                    Ok(WorkRecord {
                        site: row.get(0)?,
                        work_key: row.get(1)?,
                        title: row.get(2)?,
                        author: row.get(3)?,
                        author_id: row.get(4)?,
                        author_url: row.get(5)?,
                        r#type: row.get(6)?,
                        serialization: row.get(7)?,
                        caption: row.get(8)?,
                        create_date: row.get(9)?,
                        update_date: row.get(10)?,
                        raw_json: serde_json::from_str(&raw_json).unwrap_or_default(),
                    })
                })?;
                for row in rows {
                    works.push(row?);
                }
            }
            None => {
                let mut stmt = self.conn.prepare(
                    r#"SELECT site, work_key, title, author, author_id, author_url, type, serialization, caption, create_date, update_date, raw_json
                       FROM works ORDER BY site, author, title"#,
                )?;
                let rows = stmt.query_map([], |row| {
                    let raw_json: String = row.get(11)?;
                    Ok(WorkRecord {
                        site: row.get(0)?,
                        work_key: row.get(1)?,
                        title: row.get(2)?,
                        author: row.get(3)?,
                        author_id: row.get(4)?,
                        author_url: row.get(5)?,
                        r#type: row.get(6)?,
                        serialization: row.get(7)?,
                        caption: row.get(8)?,
                        create_date: row.get(9)?,
                        update_date: row.get(10)?,
                        raw_json: serde_json::from_str(&raw_json).unwrap_or_default(),
                    })
                })?;
                for row in rows {
                    works.push(row?);
                }
            }
        }
        Ok(works)
    }

    pub fn upsert_image(&self, image: &ImageRecord) -> Result<()> {
        self.conn.execute(
            r#"INSERT INTO images (logical_name, hash, ext, kind)
               VALUES (?1, ?2, ?3, ?4)
               ON CONFLICT(logical_name) DO UPDATE SET
                   hash=excluded.hash,
                   ext=excluded.ext,
                   kind=excluded.kind"#,
            params![image.logical_name, image.hash, image.ext, image.kind],
        )?;
        Ok(())
    }

    pub fn list_images(&self) -> Result<Vec<ImageRecord>> {
        let mut stmt = self
            .conn
            .prepare(r#"SELECT logical_name, hash, ext, kind FROM images ORDER BY logical_name"#)?;
        let rows = stmt.query_map([], |row| {
            Ok(ImageRecord {
                logical_name: row.get(0)?,
                hash: row.get(1)?,
                ext: row.get(2)?,
                kind: row.get(3)?,
            })
        })?;
        let mut images = Vec::new();
        for row in rows {
            images.push(row?);
        }
        Ok(images)
    }

    pub fn upsert_site_document_json(&self, record: &SiteDocumentRecord) -> Result<()> {
        self.conn.execute(
            r#"INSERT INTO site_documents (site, key, document_json, updated_at)
               VALUES (?1, ?2, ?3, ?4)
               ON CONFLICT(site, key) DO UPDATE SET
                   document_json=excluded.document_json,
                   updated_at=excluded.updated_at"#,
            params![
                record.site,
                record.key,
                serde_json::to_string(&record.document)?,
                record.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn upsert_site_document<T>(
        &self,
        site: &str,
        key: &str,
        document: &T,
        updated_at: &str,
    ) -> Result<()>
    where
        T: Serialize,
    {
        self.upsert_site_document_json(&SiteDocumentRecord {
            site: site.to_string(),
            key: key.to_string(),
            document: serde_json::to_value(document)?,
            updated_at: updated_at.to_string(),
        })
    }

    pub fn get_site_document_record(
        &self,
        site: &str,
        key: &str,
    ) -> Result<Option<SiteDocumentRecord>> {
        self.conn
            .query_row(
                r#"SELECT site, key, document_json, updated_at
                   FROM site_documents WHERE site = ?1 AND key = ?2"#,
                params![site, key],
                |row| {
                    let document_json: String = row.get(2)?;
                    Ok(SiteDocumentRecord {
                        site: row.get(0)?,
                        key: row.get(1)?,
                        document: serde_json::from_str(&document_json).unwrap_or_default(),
                        updated_at: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn get_site_document<T>(&self, site: &str, key: &str) -> Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        self.get_site_document_record(site, key)?
            .map(|record| {
                serde_json::from_value(record.document)
                    .context("failed to deserialize site document")
            })
            .transpose()
    }

    pub fn list_site_document_records(
        &self,
        site: &str,
        key_prefix: Option<&str>,
    ) -> Result<Vec<SiteDocumentRecord>> {
        let mut records = Vec::new();
        match key_prefix {
            Some(prefix) => {
                let mut stmt = self.conn.prepare(
                    r#"SELECT site, key, document_json, updated_at
                       FROM site_documents
                       WHERE site = ?1 AND key LIKE ?2
                       ORDER BY key ASC"#,
                )?;
                let rows = stmt.query_map(params![site, format!("{prefix}%")], |row| {
                    let document_json: String = row.get(2)?;
                    Ok(SiteDocumentRecord {
                        site: row.get(0)?,
                        key: row.get(1)?,
                        document: serde_json::from_str(&document_json).unwrap_or_default(),
                        updated_at: row.get(3)?,
                    })
                })?;
                for row in rows {
                    records.push(row?);
                }
            }
            None => {
                let mut stmt = self.conn.prepare(
                    r#"SELECT site, key, document_json, updated_at
                       FROM site_documents
                       WHERE site = ?1
                       ORDER BY key ASC"#,
                )?;
                let rows = stmt.query_map([site], |row| {
                    let document_json: String = row.get(2)?;
                    Ok(SiteDocumentRecord {
                        site: row.get(0)?,
                        key: row.get(1)?,
                        document: serde_json::from_str(&document_json).unwrap_or_default(),
                        updated_at: row.get(3)?,
                    })
                })?;
                for row in rows {
                    records.push(row?);
                }
            }
        }
        Ok(records)
    }

    pub fn list_site_documents<T>(
        &self,
        site: &str,
        key_prefix: Option<&str>,
    ) -> Result<Vec<(String, T)>>
    where
        T: DeserializeOwned,
    {
        self.list_site_document_records(site, key_prefix)?
            .into_iter()
            .map(|record| {
                let key = record.key;
                let document = serde_json::from_value(record.document)
                    .context("failed to deserialize site document while listing")?;
                Ok((key, document))
            })
            .collect()
    }

    pub fn record_migration(
        &self,
        source_root: &str,
        started_at: &str,
        summary: &MigrationSummary,
    ) -> Result<()> {
        self.conn.execute(
            r#"INSERT INTO migrations (source_root, started_at, summary_json)
               VALUES (?1, ?2, ?3)"#,
            params![source_root, started_at, serde_json::to_string(summary)?],
        )?;
        Ok(())
    }

    pub fn mark_task_status(
        &self,
        task_id: i64,
        status: TaskStatus,
        error: Option<String>,
        updated_at: &str,
    ) -> Result<()> {
        self.conn.execute(
            r#"UPDATE tasks SET status = ?1, error = ?2, updated_at = ?3 WHERE id = ?4"#,
            params![status.to_string(), error, updated_at, task_id],
        )?;
        Ok(())
    }

    pub fn next_queued_task(&self) -> Result<Option<TaskRecord>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, request_id, action, param, request_json, status, error, created_at, updated_at
               FROM tasks WHERE status = 'queued' ORDER BY id ASC LIMIT 1"#,
        )?;
        let mut rows = stmt.query([])?;
        match rows.next()? {
            Some(row) => Ok(Some(task_record_from_row(row)?)),
            None => Ok(None),
        }
    }

    pub fn current_running_task(&self) -> Result<Option<TaskRecord>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, request_id, action, param, request_json, status, error, created_at, updated_at
               FROM tasks WHERE status = 'running' ORDER BY id ASC LIMIT 1"#,
        )?;
        let mut rows = stmt.query([])?;
        match rows.next()? {
            Some(row) => Ok(Some(task_record_from_row(row)?)),
            None => Ok(None),
        }
    }

    pub fn claim_next_queued_task(&self, updated_at: &str) -> Result<Option<TaskRecord>> {
        if self.current_running_task()?.is_some() {
            return Ok(None);
        }

        let Some(mut task) = self.next_queued_task()? else {
            return Ok(None);
        };

        self.mark_task_status(task.id, TaskStatus::Running, None, updated_at)?;
        task.status = TaskStatus::Running;
        task.error = None;
        task.updated_at = updated_at.to_string();
        Ok(Some(task))
    }

    pub fn requeue_running_tasks(&self, updated_at: &str) -> Result<usize> {
        let changed = self.conn.execute(
            r#"UPDATE tasks
               SET status = 'queued', error = NULL, updated_at = ?1
               WHERE status = 'running'"#,
            params![updated_at],
        )?;
        Ok(changed)
    }

    pub fn task_state(&self) -> Result<TaskState> {
        let mut state = TaskState::default();
        for task in self.list_tasks()? {
            match task.status {
                TaskStatus::Running => {
                    if state.current_task.is_none() {
                        state.current_task = Some(task.request);
                    }
                }
                TaskStatus::Queued => state.queue.push(task.request),
                TaskStatus::Succeeded | TaskStatus::Failed | TaskStatus::Skipped => {}
            }
        }
        Ok(state)
    }

    pub fn count_tasks(&self) -> Result<i64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn summary(&self) -> Result<MigrationSummary> {
        Ok(MigrationSummary {
            accounts: self
                .conn
                .query_row("SELECT COUNT(*) FROM accounts", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0) as usize,
            tasks: self
                .conn
                .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get::<_, i64>(0))
                .unwrap_or(0) as usize,
            works: self
                .conn
                .query_row("SELECT COUNT(*) FROM works", [], |row| row.get::<_, i64>(0))
                .unwrap_or(0) as usize,
            images: self
                .conn
                .query_row("SELECT COUNT(*) FROM images", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0) as usize,
            site_documents: self
                .conn
                .query_row("SELECT COUNT(*) FROM site_documents", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0) as usize,
            archived_files: 0,
        })
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TaskStatus::Queued => "queued",
            TaskStatus::Running => "running",
            TaskStatus::Succeeded => "succeeded",
            TaskStatus::Failed => "failed",
            TaskStatus::Skipped => "skipped",
        };
        f.write_str(s)
    }
}

fn parse_status(value: &str) -> TaskStatus {
    match value {
        "running" => TaskStatus::Running,
        "succeeded" => TaskStatus::Succeeded,
        "failed" => TaskStatus::Failed,
        "skipped" => TaskStatus::Skipped,
        _ => TaskStatus::Queued,
    }
}

fn task_record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRecord> {
    let request_json: String = row.get(4)?;
    let request: RequestData = serde_json::from_str(&request_json).unwrap_or_default();
    let status_str: String = row.get(5)?;
    Ok(TaskRecord {
        id: row.get(0)?,
        request_id: row.get(1)?,
        action: row.get(2)?,
        param: row.get(3)?,
        request,
        status: parse_status(&status_str),
        error: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_task(request_id: &str) -> TaskRecord {
        TaskRecord {
            id: 0,
            request_id: request_id.to_string(),
            action: "download".to_string(),
            param: "https://example.com/work".to_string(),
            request: RequestData {
                request_id: request_id.to_string(),
                add: Some("https://example.com/work".to_string()),
                ..RequestData::default()
            },
            status: TaskStatus::Queued,
            error: None,
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn claim_next_queued_task_marks_single_running_task() {
        let store = Store::open_in_memory().expect("store");
        store
            .enqueue_task(&sample_task("req-1"))
            .expect("enqueue first");
        store
            .enqueue_task(&sample_task("req-2"))
            .expect("enqueue second");

        let claimed = store
            .claim_next_queued_task("2025-01-01T00:00:01Z")
            .expect("claim task")
            .expect("task");
        assert_eq!(claimed.request_id, "req-1");
        assert_eq!(claimed.status, TaskStatus::Running);

        let next = store
            .claim_next_queued_task("2025-01-01T00:00:02Z")
            .expect("claim again");
        assert!(next.is_none());
    }

    #[test]
    fn task_state_mirrors_running_and_queued_requests() {
        let store = Store::open_in_memory().expect("store");
        store
            .enqueue_task(&sample_task("req-1"))
            .expect("enqueue first");
        store
            .enqueue_task(&sample_task("req-2"))
            .expect("enqueue second");
        let claimed = store
            .claim_next_queued_task("2025-01-01T00:00:01Z")
            .expect("claim task");
        assert!(claimed.is_some());

        let state = store.task_state().expect("task state");
        assert_eq!(
            state
                .current_task
                .as_ref()
                .map(|task| task.request_id.as_str()),
            Some("req-1")
        );
        assert_eq!(state.queue.len(), 1);
        assert_eq!(state.queue[0].request_id, "req-2");
    }

    #[test]
    fn requeue_running_tasks_returns_task_to_queue() {
        let store = Store::open_in_memory().expect("store");
        store
            .enqueue_task(&sample_task("req-1"))
            .expect("enqueue task");
        let claimed = store
            .claim_next_queued_task("2025-01-01T00:00:01Z")
            .expect("claim task");
        assert!(claimed.is_some());

        let changed = store
            .requeue_running_tasks("2025-01-01T00:00:02Z")
            .expect("requeue running");
        assert_eq!(changed, 1);

        let state = store.task_state().expect("task state");
        assert!(state.current_task.is_none());
        assert_eq!(state.queue.len(), 1);
        assert_eq!(state.queue[0].request_id, "req-1");
    }

    #[test]
    fn site_documents_support_roundtrip_and_prefix_listing() {
        let store = Store::open_in_memory().expect("store");
        store
            .upsert_site_document(
                "pixiv",
                "tracked_users/123/config",
                &serde_json::json!({"novel": "enable"}),
                "2025-01-01T00:00:00Z",
            )
            .expect("insert config");
        store
            .upsert_site_document(
                "pixiv",
                "tracked_users/123/illust_ids",
                &serde_json::json!(["1", "2"]),
                "2025-01-01T00:00:01Z",
            )
            .expect("insert snapshot");
        store
            .upsert_site_document(
                "narou",
                "documents/example",
                &serde_json::json!({"value": true}),
                "2025-01-01T00:00:02Z",
            )
            .expect("insert other site");

        let config: serde_json::Value = store
            .get_site_document("pixiv", "tracked_users/123/config")
            .expect("load config")
            .expect("config exists");
        assert_eq!(config["novel"], "enable");

        let records = store
            .list_site_document_records("pixiv", Some("tracked_users/123/"))
            .expect("list pixiv docs");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].key, "tracked_users/123/config");
        assert_eq!(records[1].key, "tracked_users/123/illust_ids");
    }
}
