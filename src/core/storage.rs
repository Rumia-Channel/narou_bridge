use crate::core::model::{
    AccountRecord, ImageRecord, MigrationSummary, RequestData, SiteDocumentRecord, TaskRecord,
    TaskState, TaskStatus, WorkListPage, WorkRecord, WorkSummary,
};
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use serde::{Serialize, de::DeserializeOwned};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

pub struct Store {
    inner: Arc<StoreInner>,
}

struct StoreInner {
    conn: Mutex<Connection>,
    in_memory: bool,
    failed_tasks: Mutex<HashMap<i64, FailedTaskState>>,
    last_stale_recovery: Mutex<Option<Instant>>,
    #[cfg(test)]
    mark_task_status_failures: Mutex<usize>,
}

const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const MARK_TASK_STATUS_RETRY_ATTEMPTS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkListSort {
    UpdatedDesc,
    UpdatedAsc,
    TitleAsc,
    TitleDesc,
    AuthorAsc,
    AuthorDesc,
}

impl WorkListSort {
    fn order_by(self) -> &'static str {
        match self {
            Self::UpdatedDesc => "update_date DESC, title COLLATE NOCASE ASC, work_key ASC",
            Self::UpdatedAsc => "update_date ASC, title COLLATE NOCASE ASC, work_key ASC",
            Self::TitleAsc => "title COLLATE NOCASE ASC, work_key ASC",
            Self::TitleDesc => "title COLLATE NOCASE DESC, work_key ASC",
            Self::AuthorAsc => "author COLLATE NOCASE ASC, title COLLATE NOCASE ASC, work_key ASC",
            Self::AuthorDesc => {
                "author COLLATE NOCASE DESC, title COLLATE NOCASE ASC, work_key ASC"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WorkListFilters<'a> {
    pub site: Option<&'a str>,
    pub search: Option<&'a str>,
    pub author_id: Option<&'a str>,
    pub author: Option<&'a str>,
    pub work_type: Option<&'a str>,
    pub serialization: Option<&'a str>,
}

const MARK_TASK_STATUS_RETRY_DELAY: Duration = Duration::from_millis(100);
const STALE_RUNNING_THRESHOLD: ChronoDuration = ChronoDuration::hours(1);
const STALE_RUNNING_RECOVERY_INTERVAL: Duration = Duration::from_secs(60);
const STALE_RUNNING_ERROR: &str = "worker recovered stale running task";

#[derive(Clone)]
struct FailedTaskState {
    error: Option<String>,
    updated_at: String,
}

fn store_cache() -> &'static Mutex<HashMap<PathBuf, Arc<StoreInner>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<StoreInner>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let cache = store_cache();
        let mut cache = cache
            .lock()
            .map_err(|_| anyhow!("sqlite store cache mutex poisoned"))?;
        if let Some(inner) = cache.get(&path) {
            return Ok(Self {
                inner: Arc::clone(inner),
            });
        }

        let store = Self::from_connection(
            Connection::open(&path).context("failed to open sqlite database")?,
            false,
        )?;
        cache.insert(path, Arc::clone(&store.inner));
        Ok(store)
    }

    fn from_connection(conn: Connection, in_memory: bool) -> Result<Self> {
        conn.busy_timeout(SQLITE_BUSY_TIMEOUT)
            .context("failed to configure sqlite busy timeout")?;
        let store = Self {
            inner: Arc::new(StoreInner {
                conn: Mutex::new(conn),
                in_memory,
                failed_tasks: Mutex::new(HashMap::new()),
                last_stale_recovery: Mutex::new(None),
                #[cfg(test)]
                mark_task_status_failures: Mutex::new(0),
            }),
        };
        store.init_schema()?;
        Ok(store)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(
            Connection::open_in_memory().context("failed to open in-memory sqlite database")?,
            true,
        )
    }

    fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = self
            .inner
            .conn
            .lock()
            .map_err(|_| anyhow!("sqlite connection mutex poisoned"))?;
        f(&conn)
    }

    fn init_schema(&self) -> Result<()> {
        self.with_conn(|conn| {
            if !self.inner.in_memory {
                conn.pragma_update(None, "journal_mode", "WAL")
                    .context("failed to enable sqlite WAL mode")?;
                verify_runtime_journal_mode(conn)?;
            }
            conn.pragma_update(None, "synchronous", "NORMAL")
                .context("failed to configure sqlite synchronous mode")?;
            conn.pragma_update(None, "foreign_keys", "ON")
                .context("failed to enable sqlite foreign keys")?;
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS tasks (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    request_id TEXT NOT NULL,
                    action TEXT NOT NULL,
                    param TEXT NOT NULL,
                    request_json TEXT NOT NULL,
                    status TEXT NOT NULL,
                    error TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    started_at TEXT
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
            conn.execute(
                "CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_request_id ON tasks(request_id)",
                [],
            )?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_works_site_update_date ON works(site, update_date DESC, work_key)",
                [],
            )?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_works_site_title ON works(site, title COLLATE NOCASE, work_key)",
                [],
            )?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_works_site_author ON works(site, author COLLATE NOCASE, title COLLATE NOCASE, work_key)",
                [],
            )?;
            ensure_column_exists(conn, "tasks", "started_at", "TEXT")?;
            Ok(())
        })
    }

    fn persist_task_status(
        &self,
        task_id: i64,
        status: TaskStatus,
        error: Option<String>,
        updated_at: &str,
    ) -> Result<()> {
        self.maybe_fail_mark_task_status()?;
        self.with_conn(|conn| {
            let changed = conn.execute(
                r#"UPDATE tasks
                   SET status = ?1,
                       error = ?2,
                       updated_at = ?3,
                       started_at = CASE
                           WHEN ?1 = 'running' THEN COALESCE(started_at, ?3)
                           WHEN ?1 = 'queued' THEN NULL
                           ELSE started_at
                       END
                   WHERE id = ?4"#,
                params![status.to_string(), error, updated_at, task_id],
            )?;
            if changed == 0 {
                anyhow::bail!("task {task_id} not found");
            }
            Ok(())
        })
    }

    fn clear_failed_task_state(&self, task_id: i64) {
        if let Ok(mut failed_tasks) = self.inner.failed_tasks.lock() {
            failed_tasks.remove(&task_id);
        }
    }

    fn remember_failed_task_state(&self, task_id: i64, error: Option<String>, updated_at: &str) {
        if let Ok(mut failed_tasks) = self.inner.failed_tasks.lock() {
            failed_tasks.insert(
                task_id,
                FailedTaskState {
                    error,
                    updated_at: updated_at.to_string(),
                },
            );
        }
    }

    fn failed_task_state(&self, task_id: i64) -> Option<FailedTaskState> {
        self.inner
            .failed_tasks
            .lock()
            .ok()
            .and_then(|failed_tasks| failed_tasks.get(&task_id).cloned())
    }

    fn apply_failed_task_state(&self, task: &mut TaskRecord) {
        if let Some(failed_state) = self.failed_task_state(task.id) {
            task.status = TaskStatus::Failed;
            task.error = failed_state.error;
            task.updated_at = failed_state.updated_at;
        }
    }

    fn list_tasks_with_status(
        &self,
        status: &str,
        include_started_at: bool,
    ) -> Result<Vec<(TaskRecord, Option<String>)>> {
        self.with_conn(|conn| {
            let select = if include_started_at {
                r#"SELECT id, request_id, action, param, request_json, status, error, created_at, updated_at, started_at
                   FROM tasks WHERE status = ?1 ORDER BY id ASC"#
            } else {
                r#"SELECT id, request_id, action, param, request_json, status, error, created_at, updated_at, NULL AS started_at
                   FROM tasks WHERE status = ?1 ORDER BY id ASC"#
            };
            let mut stmt = conn.prepare(select)?;
            let rows = stmt.query_map([status], task_with_started_at_from_row)?;
            let mut tasks = Vec::new();
            for row in rows {
                let (mut task, started_at) = row?;
                self.apply_failed_task_state(&mut task);
                tasks.push((task, started_at));
            }
            Ok(tasks)
        })
    }

    fn maybe_recover_stale_running_tasks(&self, updated_at: &str) -> Result<()> {
        let should_recover = {
            let mut last_recovery = self
                .inner
                .last_stale_recovery
                .lock()
                .map_err(|_| anyhow!("stale recovery mutex poisoned"))?;
            if last_recovery
                .map(|instant| instant.elapsed() >= STALE_RUNNING_RECOVERY_INTERVAL)
                .unwrap_or(true)
            {
                *last_recovery = Some(Instant::now());
                true
            } else {
                false
            }
        };

        if should_recover {
            let _ = self.recover_stale_running_tasks(updated_at)?;
        }
        Ok(())
    }

    fn recover_stale_running_tasks(&self, updated_at: &str) -> Result<usize> {
        let cutoff = parse_timestamp(updated_at)? - STALE_RUNNING_THRESHOLD;
        let mut recovered = 0;
        for (task, started_at) in self.list_tasks_with_status("running", true)? {
            if task.status != TaskStatus::Running {
                continue;
            }

            let started_at = started_at.unwrap_or_else(|| task.updated_at.clone());
            if parse_timestamp(&started_at)? > cutoff {
                continue;
            }

            let stale_error = Some(format!("{STALE_RUNNING_ERROR}: running since {started_at}"));
            match self.mark_task_status(task.id, TaskStatus::Failed, stale_error, updated_at) {
                Ok(()) => recovered += 1,
                Err(err) => error!(
                    task_id = task.id,
                    request_id = %task.request_id,
                    error = %err,
                    "failed to recover stale running task"
                ),
            }
        }
        Ok(recovered)
    }

    #[cfg(test)]
    fn set_mark_task_status_failures(&self, failures: usize) {
        let mut remaining = self
            .inner
            .mark_task_status_failures
            .lock()
            .expect("mark_task_status failure injector");
        *remaining = failures;
    }

    #[cfg(test)]
    fn maybe_fail_mark_task_status(&self) -> Result<()> {
        let mut remaining = self
            .inner
            .mark_task_status_failures
            .lock()
            .map_err(|_| anyhow!("mark_task_status failure injector poisoned"))?;
        if *remaining > 0 {
            *remaining -= 1;
            anyhow::bail!("injected mark_task_status failure");
        }
        Ok(())
    }

    #[cfg(not(test))]
    fn maybe_fail_mark_task_status(&self) -> Result<()> {
        Ok(())
    }

    pub fn enqueue_task(&self, task: &TaskRecord) -> Result<i64> {
        self.with_conn(|conn| {
            let changed = conn.execute(
                r#"INSERT OR IGNORE INTO tasks (request_id, action, param, request_json, status, error, created_at, updated_at, started_at)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)"#,
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
            Ok(if changed == 0 {
                0
            } else {
                conn.last_insert_rowid()
            })
        })
    }

    pub fn list_tasks(&self) -> Result<Vec<TaskRecord>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"SELECT id, request_id, action, param, request_json, status, error, created_at, updated_at
                   FROM tasks ORDER BY id ASC"#,
            )?;
            let rows = stmt.query_map([], task_record_from_row)?;
            let mut tasks = Vec::new();
            for row in rows {
                let mut task = row?;
                self.apply_failed_task_state(&mut task);
                tasks.push(task);
            }
            Ok(tasks)
        })
    }

    pub fn has_incomplete_task(&self, action: &str, param: &str) -> Result<bool> {
        Ok(self.list_tasks()?.into_iter().any(|task| {
            task.action == action
                && task.param == param
                && matches!(task.status, TaskStatus::Queued | TaskStatus::Running)
        }))
    }

    pub fn upsert_account(&self, account: &AccountRecord) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
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
            if account.active {
                conn.execute(
                    r#"UPDATE accounts
                       SET active = CASE WHEN name = ?2 THEN 1 ELSE 0 END,
                           updated_at = CASE
                               WHEN name = ?2 THEN ?3
                               WHEN active != 0 THEN ?3
                               ELSE updated_at
                           END
                       WHERE site = ?1"#,
                    params![account.site, account.name, account.updated_at],
                )?;
            }
            Ok(())
        })
    }

    pub fn list_accounts(&self, site: &str) -> Result<Vec<AccountRecord>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"SELECT site, name, display_name, account_json, active, updated_at
                   FROM accounts WHERE site = ?1 ORDER BY active DESC, name ASC"#,
            )?;
            let rows = stmt.query_map([site], account_record_from_row)?;
            let mut accounts = Vec::new();
            for row in rows {
                accounts.push(row?);
            }
            Ok(accounts)
        })
    }

    pub fn get_account(&self, site: &str, name: &str) -> Result<Option<AccountRecord>> {
        self.with_conn(|conn| {
            conn.query_row(
                r#"SELECT site, name, display_name, account_json, active, updated_at
                   FROM accounts WHERE site = ?1 AND name = ?2"#,
                params![site, name],
                account_record_from_row,
            )
            .optional()
            .map_err(Into::into)
        })
    }

    pub fn delete_account(&self, site: &str, name: &str) -> Result<bool> {
        self.with_conn(|conn| {
            let changed = conn.execute(
                r#"DELETE FROM accounts WHERE site = ?1 AND name = ?2"#,
                params![site, name],
            )?;
            Ok(changed > 0)
        })
    }

    pub fn rename_account(
        &self,
        site: &str,
        old_name: &str,
        new_name: &str,
        updated_at: &str,
    ) -> Result<Option<AccountRecord>> {
        if old_name == new_name {
            return self.get_account(site, old_name);
        }
        let changed = self.with_conn(|conn| {
            conn.execute(
                r#"UPDATE accounts
                   SET name = ?1, updated_at = ?2
                   WHERE site = ?3 AND name = ?4"#,
                params![new_name, updated_at, site, old_name],
            )
            .map_err(Into::into)
        })?;
        if changed == 0 {
            return Ok(None);
        }
        self.get_account(site, new_name)
    }

    pub fn set_active_account(
        &self,
        site: &str,
        name: &str,
        updated_at: &str,
    ) -> Result<Option<AccountRecord>> {
        if self.get_account(site, name)?.is_none() {
            return Ok(None);
        }
        self.with_conn(|conn| {
            conn.execute(
                r#"UPDATE accounts
                   SET active = CASE WHEN name = ?2 THEN 1 ELSE 0 END,
                       updated_at = CASE
                           WHEN name = ?2 THEN ?3
                           WHEN active != 0 THEN ?3
                           ELSE updated_at
                       END
                   WHERE site = ?1"#,
                params![site, name, updated_at],
            )?;
            Ok(())
        })?;
        self.get_account(site, name)
    }

    pub fn get_active_account(&self, site: &str) -> Result<Option<AccountRecord>> {
        self.with_conn(|conn| {
            conn.query_row(
                r#"SELECT site, name, display_name, account_json, active, updated_at
                   FROM accounts
                   WHERE site = ?1 AND active != 0
                   ORDER BY updated_at DESC, name ASC
                   LIMIT 1"#,
                [site],
                account_record_from_row,
            )
            .optional()
            .map_err(Into::into)
        })
    }

    pub fn upsert_work(&self, work: &WorkRecord) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
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
        })
    }

    pub fn list_works(&self, site: Option<&str>) -> Result<Vec<WorkRecord>> {
        self.with_conn(|conn| {
            let mut works = Vec::new();
            match site {
                Some(site) => {
                    let mut stmt = conn.prepare(
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
                            raw_json: serde_json::from_str(&raw_json).map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    11,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })?,
                        })
                    })?;
                    for row in rows {
                        works.push(row?);
                    }
                }
                None => {
                    let mut stmt = conn.prepare(
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
                            raw_json: serde_json::from_str(&raw_json).map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    11,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })?,
                        })
                    })?;
                    for row in rows {
                        works.push(row?);
                    }
                }
            }
            Ok(works)
        })
    }

    pub fn list_work_summaries(
        &self,
        filters: WorkListFilters<'_>,
        limit: usize,
        offset: usize,
        sort: WorkListSort,
    ) -> Result<WorkListPage> {
        let limit = limit.clamp(1, i64::MAX as usize);
        let offset = offset.min(i64::MAX as usize);
        let site = trimmed_filter(filters.site);
        let search = trimmed_filter(filters.search);
        let author_id = trimmed_filter(filters.author_id);
        let author = trimmed_filter(filters.author);
        let work_type = trimmed_filter(filters.work_type);
        let serialization = trimmed_filter(filters.serialization);

        self.with_conn(|conn| {
            let mut where_parts = Vec::new();
            let mut params = Vec::new();

            if let Some(site) = site {
                where_parts.push("site = ?");
                params.push(SqlValue::Text(site.to_string()));
            }

            if let Some(search) = search {
                where_parts.push(
                    "(title LIKE ? ESCAPE '\\' OR author LIKE ? ESCAPE '\\' OR work_key LIKE ? ESCAPE '\\')",
                );
                let pattern = format!("%{}%", escape_like_pattern(search));
                params.push(SqlValue::Text(pattern.clone()));
                params.push(SqlValue::Text(pattern.clone()));
                params.push(SqlValue::Text(pattern));
            }

            if let Some(author_id) = author_id {
                where_parts.push("author_id = ?");
                params.push(SqlValue::Text(author_id.to_string()));
            }

            if let Some(author) = author {
                where_parts.push("author = ?");
                params.push(SqlValue::Text(author.to_string()));
            }

            if let Some(work_type) = work_type {
                where_parts.push("type = ?");
                params.push(SqlValue::Text(work_type.to_string()));
            }

            if let Some(serialization) = serialization {
                where_parts.push("serialization = ?");
                params.push(SqlValue::Text(serialization.to_string()));
            }

            let where_sql = if where_parts.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", where_parts.join(" AND "))
            };

            let count_sql = format!("SELECT COUNT(*) FROM works{where_sql}");
            let total = conn.query_row(&count_sql, params_from_iter(params.iter()), |row| {
                row.get::<_, i64>(0)
            })? as usize;

            let mut page_params = params;
            page_params.push(SqlValue::Integer(limit as i64));
            page_params.push(SqlValue::Integer(offset as i64));
            let sql = format!(
                r#"SELECT site, work_key, title, author, author_id, author_url, type, serialization, caption, create_date, update_date
                   FROM works{where_sql}
                   ORDER BY {}
                   LIMIT ? OFFSET ?"#,
                sort.order_by()
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(page_params.iter()), |row| {
                Ok(WorkSummary {
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
                })
            })?;

            let mut works = Vec::new();
            for row in rows {
                works.push(row?);
            }

            Ok(WorkListPage {
                site: site.map(str::to_string),
                limit,
                offset,
                total,
                works,
            })
        })
    }

    pub fn get_work(&self, site: &str, work_key: &str) -> Result<Option<WorkRecord>> {
        self.with_conn(|conn| {
            conn.query_row(
                r#"SELECT site, work_key, title, author, author_id, author_url, type, serialization, caption, create_date, update_date, raw_json
                   FROM works WHERE site = ?1 AND work_key = ?2"#,
                params![site, work_key],
                |row| {
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
                        raw_json: serde_json::from_str(&raw_json).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                11,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
    }

    pub fn upsert_image(&self, image: &ImageRecord) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"INSERT INTO images (logical_name, hash, ext, kind)
                   VALUES (?1, ?2, ?3, ?4)
                   ON CONFLICT(logical_name) DO UPDATE SET
                       hash=excluded.hash,
                       ext=excluded.ext,
                       kind=excluded.kind"#,
                params![image.logical_name, image.hash, image.ext, image.kind],
            )?;
            Ok(())
        })
    }

    pub fn bulk_upsert_images<F>(&self, images: &[ImageRecord], mut on_progress: F) -> Result<()>
    where
        F: FnMut(usize),
    {
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            {
                let mut stmt = tx.prepare(
                    r#"INSERT INTO images (logical_name, hash, ext, kind)
                       VALUES (?1, ?2, ?3, ?4)
                       ON CONFLICT(logical_name) DO UPDATE SET
                           hash=excluded.hash,
                           ext=excluded.ext,
                           kind=excluded.kind"#,
                )?;
                for (idx, image) in images.iter().enumerate() {
                    stmt.execute(params![
                        image.logical_name,
                        image.hash,
                        image.ext,
                        image.kind
                    ])?;
                    on_progress(idx + 1);
                }
            }
            tx.commit()?;
            Ok(())
        })
    }

    pub fn list_images(&self) -> Result<Vec<ImageRecord>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"SELECT logical_name, hash, ext, kind FROM images ORDER BY logical_name"#,
            )?;
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
        })
    }

    pub fn get_image(&self, logical_name: &str) -> Result<Option<ImageRecord>> {
        self.with_conn(|conn| {
            conn.query_row(
                r#"SELECT logical_name, hash, ext, kind
                   FROM images WHERE logical_name = ?1"#,
                params![logical_name],
                |row| {
                    Ok(ImageRecord {
                        logical_name: row.get(0)?,
                        hash: row.get(1)?,
                        ext: row.get(2)?,
                        kind: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
    }

    pub fn find_image_by_logical_prefix(&self, prefix: &str) -> Result<Option<ImageRecord>> {
        self.with_conn(|conn| {
            conn.query_row(
                r#"SELECT logical_name, hash, ext, kind
                   FROM images
                   WHERE logical_name LIKE ?1
                   ORDER BY logical_name
                   LIMIT 1"#,
                params![format!("{prefix}%")],
                |row| {
                    Ok(ImageRecord {
                        logical_name: row.get(0)?,
                        hash: row.get(1)?,
                        ext: row.get(2)?,
                        kind: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
    }

    pub fn upsert_site_document_json(&self, record: &SiteDocumentRecord) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
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
        })
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
        self.with_conn(|conn| {
            conn.query_row(
                r#"SELECT site, key, document_json, updated_at
                   FROM site_documents WHERE site = ?1 AND key = ?2"#,
                params![site, key],
                |row| {
                    let document_json: String = row.get(2)?;
                    Ok(SiteDocumentRecord {
                        site: row.get(0)?,
                        key: row.get(1)?,
                        document: serde_json::from_str(&document_json).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                2,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?,
                        updated_at: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
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
        self.with_conn(|conn| {
            let mut records = Vec::new();
            match key_prefix {
                Some(prefix) => {
                    let mut stmt = conn.prepare(
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
                            document: serde_json::from_str(&document_json).map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    2,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })?,
                            updated_at: row.get(3)?,
                        })
                    })?;
                    for row in rows {
                        records.push(row?);
                    }
                }
                None => {
                    let mut stmt = conn.prepare(
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
                            document: serde_json::from_str(&document_json).map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    2,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })?,
                            updated_at: row.get(3)?,
                        })
                    })?;
                    for row in rows {
                        records.push(row?);
                    }
                }
            }
            Ok(records)
        })
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
        self.with_conn(|conn| {
            conn.execute(
                r#"INSERT INTO migrations (source_root, started_at, summary_json)
                   VALUES (?1, ?2, ?3)"#,
                params![source_root, started_at, serde_json::to_string(summary)?],
            )?;
            Ok(())
        })
    }

    pub fn mark_task_status(
        &self,
        task_id: i64,
        status: TaskStatus,
        error: Option<String>,
        updated_at: &str,
    ) -> Result<()> {
        let mut last_error = None;
        for attempt in 0..MARK_TASK_STATUS_RETRY_ATTEMPTS {
            match self.persist_task_status(task_id, status.clone(), error.clone(), updated_at) {
                Ok(()) => {
                    self.clear_failed_task_state(task_id);
                    return Ok(());
                }
                Err(err) => {
                    last_error = Some(err);
                    if attempt + 1 < MARK_TASK_STATUS_RETRY_ATTEMPTS {
                        thread::sleep(MARK_TASK_STATUS_RETRY_DELAY);
                    }
                }
            }
        }

        let err = last_error.unwrap_or_else(|| anyhow!("mark_task_status failed unexpectedly"));
        let failed_error = format!(
            "failed to persist task status after {MARK_TASK_STATUS_RETRY_ATTEMPTS} attempts: {err}"
        );
        error!(
            task_id,
            target_status = %status,
            error = %err,
            "failed to persist task status"
        );
        self.remember_failed_task_state(task_id, Some(failed_error.clone()), updated_at);
        Err(anyhow!(failed_error))
    }

    pub fn next_queued_task(&self) -> Result<Option<TaskRecord>> {
        Ok(self
            .list_tasks_with_status("queued", false)?
            .into_iter()
            .map(|(task, _)| task)
            .find(|task| task.status == TaskStatus::Queued))
    }

    pub fn current_running_task(&self) -> Result<Option<TaskRecord>> {
        Ok(self
            .list_tasks_with_status("running", false)?
            .into_iter()
            .map(|(task, _)| task)
            .find(|task| task.status == TaskStatus::Running))
    }

    pub fn claim_next_queued_task(&self, updated_at: &str) -> Result<Option<TaskRecord>> {
        self.maybe_recover_stale_running_tasks(updated_at)?;
        if self.current_running_task()?.is_some() {
            return Ok(None);
        }

        for (mut task, _) in self.list_tasks_with_status("queued", false)? {
            if task.status != TaskStatus::Queued {
                continue;
            }

            match self.mark_task_status(task.id, TaskStatus::Running, None, updated_at) {
                Ok(()) => {
                    task.status = TaskStatus::Running;
                    task.error = None;
                    task.updated_at = updated_at.to_string();
                    return Ok(Some(task));
                }
                Err(err) => {
                    error!(
                        task_id = task.id,
                        request_id = %task.request_id,
                        error = %err,
                        "failed to claim queued task, treating it as failed in memory"
                    );
                }
            }
        }

        Ok(None)
    }

    pub fn requeue_running_tasks(&self, updated_at: &str) -> Result<usize> {
        let cutoff = parse_timestamp(updated_at)? - STALE_RUNNING_THRESHOLD;
        let mut requeued = 0;

        for (task, started_at) in self.list_tasks_with_status("running", true)? {
            if task.status != TaskStatus::Running {
                continue;
            }

            let started_at = started_at.unwrap_or_else(|| task.updated_at.clone());
            if parse_timestamp(&started_at)? <= cutoff {
                let stale_error =
                    Some(format!("{STALE_RUNNING_ERROR}: running since {started_at}"));
                if let Err(err) =
                    self.mark_task_status(task.id, TaskStatus::Failed, stale_error, updated_at)
                {
                    error!(
                        task_id = task.id,
                        request_id = %task.request_id,
                        error = %err,
                        "failed to persist stale running task failure"
                    );
                }
                continue;
            }

            if let Err(err) = self.mark_task_status(task.id, TaskStatus::Queued, None, updated_at) {
                error!(
                    task_id = task.id,
                    request_id = %task.request_id,
                    error = %err,
                    "failed to requeue running task"
                );
                continue;
            }
            requeued += 1;
        }

        Ok(requeued)
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
        self.with_conn(|conn| {
            let count: i64 = conn.query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))?;
            Ok(count)
        })
    }

    pub fn summary(&self) -> Result<MigrationSummary> {
        self.with_conn(|conn| {
            Ok(MigrationSummary {
                accounts: conn
                    .query_row("SELECT COUNT(*) FROM accounts", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap_or(0) as usize,
                tasks: conn
                    .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get::<_, i64>(0))
                    .unwrap_or(0) as usize,
                works: conn
                    .query_row("SELECT COUNT(*) FROM works", [], |row| row.get::<_, i64>(0))
                    .unwrap_or(0) as usize,
                images: conn
                    .query_row("SELECT COUNT(*) FROM images", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap_or(0) as usize,
                site_documents: conn
                    .query_row("SELECT COUNT(*) FROM site_documents", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap_or(0) as usize,
                archived_files: 0,
            })
        })
    }
}

fn verify_runtime_journal_mode(conn: &Connection) -> Result<()> {
    let journal_mode: String = conn
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .context("failed to read sqlite journal_mode")?;
    if journal_mode.eq_ignore_ascii_case("wal") {
        info!("sqlite journal_mode=WAL");
    } else {
        warn!(journal_mode = %journal_mode, "sqlite journal_mode is not WAL");
    }
    Ok(())
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

fn ensure_column_exists(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for existing in columns {
        if existing? == column {
            return Ok(());
        }
    }
    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )?;
    Ok(())
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("failed to parse timestamp: {value}"))?
        .with_timezone(&Utc))
}

fn account_record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AccountRecord> {
    let account_json: String = row.get(3)?;
    let account: crate::core::model::AccountFile =
        serde_json::from_str(&account_json).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
        })?;
    Ok(AccountRecord {
        site: row.get(0)?,
        name: row.get(1)?,
        display_name: row.get(2)?,
        account,
        active: row.get::<_, i64>(4)? != 0,
        updated_at: row.get(5)?,
    })
}

fn task_record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRecord> {
    let request_json: String = row.get(4)?;
    let request: RequestData = serde_json::from_str(&request_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
    })?;
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

fn task_with_started_at_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(TaskRecord, Option<String>)> {
    Ok((task_record_from_row(row)?, row.get(9)?))
}

fn trimmed_filter(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn escape_like_pattern(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

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

    fn sample_work(site: &str, work_key: &str, title: &str, author: &str) -> WorkRecord {
        WorkRecord {
            site: site.to_string(),
            work_key: work_key.to_string(),
            title: title.to_string(),
            author: author.to_string(),
            author_id: Some(format!("{author}-id")),
            author_url: Some(format!("https://example.invalid/users/{author}")),
            r#type: "novel".to_string(),
            serialization: "短編".to_string(),
            caption: format!("{title} caption"),
            create_date: "2025-01-01T00:00:00Z".to_string(),
            update_date: format!("2025-01-0{}T00:00:00Z", &work_key[1..]),
            raw_json: serde_json::json!({
                "title": title,
                "id": work_key,
                "nid": work_key,
                "author": author,
                "type": "novel",
                "serialization": "短編",
                "episodes": {}
            }),
        }
    }

    #[test]
    fn list_work_summaries_pages_sorts_and_searches_without_raw_json() {
        let store = Store::open_in_memory().expect("store");
        store
            .upsert_work(&sample_work("pixiv", "n1", "Alpha Work", "Zeta"))
            .expect("upsert first work");
        store
            .upsert_work(&sample_work("pixiv", "n2", "Beta Match", "Alpha"))
            .expect("upsert second work");
        store
            .upsert_work(&sample_work("pixiv", "n3", "Gamma Match", "Beta"))
            .expect("upsert third work");
        store
            .upsert_work(&sample_work("narou", "n4", "Other Match", "Other"))
            .expect("upsert other site work");

        let page = store
            .list_work_summaries(
                WorkListFilters {
                    site: Some("pixiv"),
                    search: Some("Match"),
                    ..WorkListFilters::default()
                },
                1,
                1,
                WorkListSort::TitleAsc,
            )
            .expect("list work summaries");

        assert_eq!(page.site.as_deref(), Some("pixiv"));
        assert_eq!(page.total, 2);
        assert_eq!(page.limit, 1);
        assert_eq!(page.offset, 1);
        assert_eq!(page.works.len(), 1);
        assert_eq!(page.works[0].work_key, "n3");
        assert_eq!(page.works[0].title, "Gamma Match");

        let author_page = store
            .list_work_summaries(
                WorkListFilters {
                    site: Some("pixiv"),
                    author_id: Some("Alpha-id"),
                    work_type: Some("novel"),
                    serialization: Some("短編"),
                    ..WorkListFilters::default()
                },
                10,
                0,
                WorkListSort::UpdatedDesc,
            )
            .expect("list filtered work summaries");
        assert_eq!(author_page.total, 1);
        assert_eq!(author_page.works[0].work_key, "n2");
    }

    fn sample_account(site: &str, name: &str, active: bool) -> AccountRecord {
        AccountRecord {
            site: site.to_string(),
            name: name.to_string(),
            display_name: Some(format!("{name} display")),
            account: crate::core::model::AccountFile {
                cookies: serde_json::json!({"session": name}),
                user_agent: Some(format!("{name}-ua")),
                display_name: Some(format!("{name} display")),
            },
            active,
            updated_at: "2025-01-01T00:00:00Z".to_string(),
        }
    }

    fn unique_test_dir(label: &str) -> PathBuf {
        let mut path = std::env::current_dir().expect("current dir");
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let counter = TEST_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
        path.push("target");
        path.push("storage-tests");
        path.push(format!(
            "{label}-{}-{timestamp}-{counter}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test directory");
        path
    }

    #[test]
    fn open_in_memory_uses_non_wal_journal_mode() {
        let store = Store::open_in_memory().expect("store");
        let journal_mode: String = store
            .with_conn(|conn| {
                conn.pragma_query_value(None, "journal_mode", |row| row.get(0))
                    .map_err(Into::into)
            })
            .expect("journal mode");
        assert_ne!(journal_mode.to_lowercase(), "wal");
    }

    #[test]
    fn open_disk_store_enables_wal_files() {
        let test_dir = unique_test_dir("wal");
        let db_path = test_dir.join("queue.sqlite3");
        let store = Store::open(&db_path).expect("store");
        store
            .enqueue_task(&sample_task("req-wal"))
            .expect("enqueue task");

        let journal_mode: String = store
            .with_conn(|conn| {
                conn.pragma_query_value(None, "journal_mode", |row| row.get(0))
                    .map_err(Into::into)
            })
            .expect("journal mode");
        assert_eq!(journal_mode.to_lowercase(), "wal");

        let synchronous: String = store
            .with_conn(|conn| {
                conn.pragma_query_value(None, "synchronous", |row| {
                    let synchronous = match row.get_ref(0)? {
                        rusqlite::types::ValueRef::Text(value) => {
                            String::from_utf8_lossy(value).into_owned()
                        }
                        rusqlite::types::ValueRef::Integer(0) => "OFF".to_string(),
                        rusqlite::types::ValueRef::Integer(1) => "NORMAL".to_string(),
                        rusqlite::types::ValueRef::Integer(2) => "FULL".to_string(),
                        rusqlite::types::ValueRef::Integer(3) => "EXTRA".to_string(),
                        other => {
                            return Err(rusqlite::Error::FromSqlConversionFailure(
                                0,
                                other.data_type(),
                                Box::new(std::io::Error::other(format!(
                                    "unexpected sqlite synchronous value: {other:?}"
                                ))),
                            ));
                        }
                    };
                    Ok(synchronous)
                })
                .map_err(Into::into)
            })
            .expect("synchronous mode");
        assert_eq!(synchronous.to_lowercase(), "normal");

        let wal_path = PathBuf::from(format!("{}-wal", db_path.display()));
        assert!(
            wal_path.exists(),
            "expected sqlite WAL file at {}",
            wal_path.display()
        );

        let _ = fs::remove_file(&wal_path);
        let _ = fs::remove_file(&db_path);
        let _ = fs::remove_dir_all(&test_dir);
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
    fn mark_task_status_retries_transient_failures() {
        let store = Store::open_in_memory().expect("store");
        store
            .enqueue_task(&sample_task("req-1"))
            .expect("enqueue task");
        let claimed = store
            .claim_next_queued_task("2025-01-01T00:00:01Z")
            .expect("claim task");
        assert!(claimed.is_some());

        store.set_mark_task_status_failures(2);
        store
            .mark_task_status(1, TaskStatus::Succeeded, None, "2025-01-01T00:00:02Z")
            .expect("retry should eventually succeed");

        let tasks = store.list_tasks().expect("list tasks");
        assert_eq!(tasks[0].status, TaskStatus::Succeeded);
        assert!(
            store
                .current_running_task()
                .expect("running task")
                .is_none()
        );
    }

    #[test]
    fn mark_task_status_failure_marks_task_failed_in_memory() {
        let store = Store::open_in_memory().expect("store");
        store
            .enqueue_task(&sample_task("req-1"))
            .expect("enqueue task");
        let claimed = store
            .claim_next_queued_task("2025-01-01T00:00:01Z")
            .expect("claim task");
        assert!(claimed.is_some());

        store.set_mark_task_status_failures(MARK_TASK_STATUS_RETRY_ATTEMPTS);
        let result = store.mark_task_status(1, TaskStatus::Succeeded, None, "2025-01-01T00:00:02Z");
        assert!(
            result.is_err(),
            "persistent failures should surface as errors"
        );

        let tasks = store.list_tasks().expect("list tasks");
        assert_eq!(tasks[0].status, TaskStatus::Failed);
        assert!(
            tasks[0]
                .error
                .as_deref()
                .is_some_and(|error| error.contains("failed to persist task status"))
        );
        assert!(
            store
                .current_running_task()
                .expect("running task")
                .is_none()
        );
        assert!(
            store
                .task_state()
                .expect("task state")
                .current_task
                .is_none()
        );
    }

    #[test]
    fn requeue_running_tasks_marks_stale_running_tasks_failed() {
        let store = Store::open_in_memory().expect("store");
        store
            .enqueue_task(&sample_task("req-1"))
            .expect("enqueue task");
        let claimed = store
            .claim_next_queued_task("2025-01-01T00:00:01Z")
            .expect("claim task");
        assert!(claimed.is_some());

        let changed = store
            .requeue_running_tasks("2025-01-01T01:00:02Z")
            .expect("recover stale running task");
        assert_eq!(changed, 0);

        let tasks = store.list_tasks().expect("list tasks");
        assert_eq!(tasks[0].status, TaskStatus::Failed);
        assert!(
            tasks[0]
                .error
                .as_deref()
                .is_some_and(|error| error.contains(STALE_RUNNING_ERROR))
        );
        assert!(
            store
                .current_running_task()
                .expect("running task")
                .is_none()
        );
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

    #[test]
    fn account_helpers_support_lookup_switch_rename_and_delete() {
        let store = Store::open_in_memory().expect("store");
        store
            .upsert_account(&sample_account("pixiv", "alpha", true))
            .expect("insert alpha");
        store
            .upsert_account(&sample_account("pixiv", "beta", false))
            .expect("insert beta");

        let active = store
            .get_active_account("pixiv")
            .expect("get active")
            .expect("active exists");
        assert_eq!(active.name, "alpha");

        let fetched = store
            .get_account("pixiv", "beta")
            .expect("get beta")
            .expect("beta exists");
        assert_eq!(fetched.display_name.as_deref(), Some("beta display"));

        let switched = store
            .set_active_account("pixiv", "beta", "2025-01-01T00:00:01Z")
            .expect("switch active")
            .expect("beta switched");
        assert_eq!(switched.name, "beta");
        assert!(switched.active);
        assert_eq!(
            store
                .get_active_account("pixiv")
                .expect("get switched active")
                .map(|account| account.name),
            Some("beta".to_string())
        );

        let renamed = store
            .rename_account("pixiv", "beta", "gamma", "2025-01-01T00:00:02Z")
            .expect("rename beta")
            .expect("gamma exists");
        assert_eq!(renamed.name, "gamma");
        assert!(renamed.active);
        assert!(
            store
                .get_account("pixiv", "beta")
                .expect("old beta lookup")
                .is_none()
        );
        assert_eq!(
            store
                .get_active_account("pixiv")
                .expect("active after rename")
                .map(|account| account.name),
            Some("gamma".to_string())
        );

        assert!(
            store
                .delete_account("pixiv", "alpha")
                .expect("delete alpha account")
        );
        assert!(
            store
                .get_account("pixiv", "alpha")
                .expect("alpha lookup after delete")
                .is_none()
        );
    }

    #[test]
    fn has_incomplete_task_checks_only_queued_and_running_matches() {
        let store = Store::open_in_memory().expect("store");
        let queued = TaskRecord {
            action: "update".to_string(),
            param: "all".to_string(),
            request: RequestData {
                request_id: "req-update".to_string(),
                update: Some("all".to_string()),
                ..RequestData::default()
            },
            ..sample_task("req-update")
        };
        store.enqueue_task(&queued).expect("enqueue queued update");
        assert!(
            store
                .has_incomplete_task("update", "all")
                .expect("queued task should match")
        );

        let claimed = store
            .claim_next_queued_task("2025-01-01T00:00:01Z")
            .expect("claim update task");
        assert!(claimed.is_some());
        assert!(
            store
                .has_incomplete_task("update", "all")
                .expect("running task should match")
        );

        store
            .mark_task_status(1, TaskStatus::Succeeded, None, "2025-01-01T00:00:02Z")
            .expect("mark succeeded");
        assert!(
            !store
                .has_incomplete_task("update", "all")
                .expect("completed task should not match")
        );
    }

    #[test]
    fn enqueue_task_ignores_duplicate_request_id() {
        let store = Store::open_in_memory().expect("store");

        let first_id = store
            .enqueue_task(&sample_task("req-dup"))
            .expect("enqueue first");
        let second_id = store
            .enqueue_task(&sample_task("req-dup"))
            .expect("enqueue duplicate");

        assert!(first_id > 0);
        assert_eq!(second_id, 0);
        let tasks = store.list_tasks().expect("list tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].request_id, "req-dup");
    }

    #[test]
    fn malformed_site_document_json_is_not_silently_overwritten_with_default() {
        let store = Store::open_in_memory().expect("store");
        store
            .with_conn(|conn| {
                conn.execute(
                    r#"INSERT INTO site_documents (site, key, document_json, updated_at)
                       VALUES (?1, ?2, ?3, ?4)"#,
                    params![
                        "pixiv",
                        "tracked_users/123/config",
                        "{not valid json",
                        "2025-01-01T00:00:00Z",
                    ],
                )?;
                Ok(())
            })
            .expect("insert malformed row");

        let read_result = store.get_site_document_record("pixiv", "tracked_users/123/config");
        assert!(
            read_result.is_err(),
            "reading malformed JSON row should propagate an error"
        );

        let list_result = store.list_site_document_records("pixiv", None);
        assert!(
            list_result.is_err(),
            "listing rows containing malformed JSON should propagate an error"
        );

        let stored: String = store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT document_json FROM site_documents WHERE site = ?1 AND key = ?2",
                    params!["pixiv", "tracked_users/123/config"],
                    |row| row.get(0),
                )
                .map_err(Into::into)
            })
            .expect("row still exists");
        assert_eq!(stored, "{not valid json");
    }
}
