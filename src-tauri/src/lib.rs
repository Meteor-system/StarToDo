use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(windows)]
use std::{
    io::{Read, Write},
    os::windows::process::CommandExt,
    process::{Child, Command, ExitStatus, Stdio},
    time::Instant,
};

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use rusqlite::{backup::Backup, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sysinfo::{
    get_current_pid, Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System,
    MINIMUM_CPU_UPDATE_INTERVAL,
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Listener, LogicalSize, Manager, State, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use tauri_plugin_deep_link::DeepLinkExt;
use url::Url;

mod parser;
mod projects;
mod reminder_jobs;
mod reminder_orchestrator;
mod reminders;
mod tasks;

const MAIN_WINDOW_LABEL: &str = "main";
const DATABASE_FILE_NAME: &str = "star-to-do.sqlite3";
const NOTIFICATION_HOST_FILE_NAME: &str = "StarToDo.NotificationHost.exe";
const STAGE0_NOTIFICATION_ID: &str = "stage0-scheduled-probe";
const STAGE0_ACTIVATION_URI: &str =
    "startodo://reminder/open?delivery=stage0-probe&token=AAAAAAAAAAAAAAAA";
const STAGE0_MIGRATION_VERSION: i64 = 1;
const TASKS_MIGRATION_VERSION: i64 = 2;
const TASK_FIELDS_MIGRATION_VERSION: i64 = 3;
const TASK_ORGANIZATION_MIGRATION_VERSION: i64 = 4;
const TASK_RECURRENCE_MIGRATION_VERSION: i64 = 5;
const PROJECTS_AND_RELIABILITY_MIGRATION_VERSION: i64 = 6;
const REMINDER_DELIVERY_RETRY_MIGRATION_VERSION: i64 = 7;
const WINDOW_PREFERENCES_FILE_NAME: &str = "window-preferences.json";
const REMINDER_WARNING_CLAIM_LEASE_MS: u128 = 15_000;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const NOTIFICATION_HOST_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(windows)]
const NOTIFICATION_HOST_POLL_INTERVAL: Duration = Duration::from_millis(25);
#[cfg(windows)]
const NOTIFICATION_HOST_MAX_OUTPUT_BYTES: usize = 64 * 1_024;
#[cfg(windows)]
static NOTIFICATION_HOST_LOCK: Mutex<()> = Mutex::new(());

struct AppState {
    database: Mutex<Connection>,
    database_path: PathBuf,
    reminder_orchestration_lock: Mutex<()>,
    reminder_host: Arc<dyn reminder_orchestrator::ReminderHost>,
    reminder_warning_state: Mutex<ReminderWarningState>,
    next_reminder_warning_listener_token: AtomicU64,
    ui_rebuild_in_progress: AtomicBool,
    next_reminder_warning_id: AtomicU64,
    scheduled_reminder_retry_at_unix_ms: AtomicI64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeSnapshot {
    process_id: u32,
    main_window_exists: bool,
    main_window_visible: bool,
    ui_ready: bool,
    ui_rebuild_in_progress: bool,
    database_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessMetric {
    process_id: u32,
    parent_process_id: Option<u32>,
    name: String,
    memory_bytes: u64,
    cpu_percent: f32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessMetricsSnapshot {
    sampled_at_unix_ms: u128,
    process_count: usize,
    total_memory_bytes: u64,
    total_cpu_percent: f32,
    processes: Vec<ProcessMetric>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DatabaseProbe {
    database_path: String,
    migration_count: i64,
    run_count: i64,
    last_probe_at_unix_ms: u128,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NotificationDiagnostics {
    immediate_notifications_available: bool,
    scheduled_notifications_available: bool,
    scheduled_notifications_status: String,
    scheduled_notifications_detail: String,
    pending_count: Option<u64>,
    notification_host_path: Option<String>,
    activation_delivery_available: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskMutation {
    task: tasks::Task,
    reminder_warning: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskCompletionCommandResult {
    task: tasks::Task,
    next_task: Option<tasks::Task>,
    reminder_warning: Option<String>,
    next_reminder_warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueuedReminderWarning {
    id: u64,
    task_id: i64,
    message: String,
    #[serde(skip_serializing)]
    claim: Option<ReminderWarningClaim>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReminderWarningClaim {
    listener_token: u64,
    expires_at_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReminderWarningListener {
    run_id: String,
    generation: u64,
    token: u64,
    event_listener_ready: bool,
}

#[derive(Debug, Default)]
struct ReminderWarningState {
    listener: Option<ReminderWarningListener>,
    highest_listener_generation: u64,
    pending: Vec<QueuedReminderWarning>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiReadyRegistration {
    runtime_snapshot: RuntimeSnapshot,
    reminder_warning_listener_token: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReconcileReport {
    scheduled: usize,
    cancelled: usize,
    missed_count: usize,
    missed_task_ids: Vec<i64>,
    capability: reminders::DeliveryCapability,
    warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowPreferences {
    mode: String,
    width: u32,
    height: u32,
    always_on_top: bool,
}

impl Default for WindowPreferences {
    fn default() -> Self {
        Self {
            mode: "full".to_string(),
            width: 800,
            height: 600,
            always_on_top: false,
        }
    }
}

fn string_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn configure_database_connection(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA busy_timeout = 5000;
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = FULL;
            ",
        )
        .map_err(string_error)?;

    let foreign_keys: i64 = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .map_err(string_error)?;
    let busy_timeout: i64 = connection
        .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
        .map_err(string_error)?;
    let journal_mode: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .map_err(string_error)?;
    let synchronous: i64 = connection
        .query_row("PRAGMA synchronous", [], |row| row.get(0))
        .map_err(string_error)?;
    if foreign_keys != 1
        || busy_timeout != 5_000
        || !journal_mode.eq_ignore_ascii_case("wal")
        || synchronous != 2
    {
        return Err("database connection safety configuration could not be verified".to_string());
    }
    Ok(())
}

fn migration_snapshot_path(database_path: &Path) -> Result<PathBuf, String> {
    let parent = database_path
        .parent()
        .ok_or_else(|| "database path has no parent directory".to_string())?;
    let file_name = database_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "database path has no valid file name".to_string())?;
    let timestamp = now_unix_ms();
    for sequence in 0..1_000 {
        let candidate = parent.join(format!(
            "{file_name}.pre-v6-{timestamp}-{}-{sequence}.sqlite3",
            std::process::id()
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("could not allocate a unique v6 migration snapshot path".to_string())
}

fn migration_v6_is_pending(connection: &Connection) -> Result<bool, String> {
    let has_migrations_table: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations')",
            [],
            |row| row.get(0),
        )
        .map_err(string_error)?;
    if !has_migrations_table {
        return Ok(false);
    }
    let has_prior_migration: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations)",
            [],
            |row| row.get(0),
        )
        .map_err(string_error)?;
    let v6_applied: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
            [PROJECTS_AND_RELIABILITY_MIGRATION_VERSION],
            |row| row.get(0),
        )
        .map_err(string_error)?;
    Ok(has_prior_migration && !v6_applied)
}

fn create_v6_migration_snapshot(
    connection: &Connection,
    database_path: &Path,
) -> Result<(), String> {
    let snapshot_path = migration_snapshot_path(database_path)?;
    let result = (|| {
        let mut snapshot = Connection::open(&snapshot_path).map_err(string_error)?;
        let backup = Backup::new(connection, &mut snapshot).map_err(string_error)?;
        backup
            .run_to_completion(128, Duration::from_millis(10), None)
            .map_err(string_error)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&snapshot_path);
    }
    result
}

fn open_database(app: &AppHandle) -> Result<(Connection, PathBuf), String> {
    let data_dir = app.path().app_data_dir().map_err(string_error)?;
    fs::create_dir_all(&data_dir).map_err(string_error)?;

    let database_path = data_dir.join(DATABASE_FILE_NAME);
    let mut connection = Connection::open(&database_path).map_err(string_error)?;
    configure_database_connection(&connection)?;
    if migration_v6_is_pending(&connection)? {
        create_v6_migration_snapshot(&connection, &database_path)?;
    }
    apply_migrations(&mut connection).map_err(string_error)?;
    Ok((connection, database_path))
}

fn apply_migrations(connection: &mut Connection) -> rusqlite::Result<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at_unix_ms INTEGER NOT NULL
        );
        ",
    )?;

    let migration_applied = |version| -> rusqlite::Result<bool> {
        transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
            [version],
            |row| row.get(0),
        )
    };

    if !migration_applied(STAGE0_MIGRATION_VERSION)? {
        transaction.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS stage0_probe (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                run_count INTEGER NOT NULL DEFAULT 0,
                last_probe_at_unix_ms INTEGER NOT NULL
            );
            ",
        )?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_unix_ms) VALUES (?1, ?2)",
            (STAGE0_MIGRATION_VERSION, now_unix_ms() as i64),
        )?;
    }

    if !migration_applied(TASKS_MIGRATION_VERSION)? {
        transaction.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS tasks (
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                notes TEXT NOT NULL DEFAULT '',
                due_at_unix_ms INTEGER,
                completed_at_unix_ms INTEGER,
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                CHECK (length(trim(title)) BETWEEN 1 AND 200),
                CHECK (length(notes) <= 10000),
                CHECK (due_at_unix_ms IS NULL OR due_at_unix_ms BETWEEN 0 AND 8640000000000000),
                CHECK (completed_at_unix_ms IS NULL OR completed_at_unix_ms >= 0)
            );
            ",
        )?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_unix_ms) VALUES (?1, ?2)",
            (TASKS_MIGRATION_VERSION, now_unix_ms() as i64),
        )?;
    }

    if !migration_applied(TASK_FIELDS_MIGRATION_VERSION)? {
        let has_column = |column: &str| -> rusqlite::Result<bool> {
            transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('tasks') WHERE name = ?1)",
                [column],
                |row| row.get(0),
            )
        };
        if !has_column("planned_date")? {
            transaction.execute("ALTER TABLE tasks ADD COLUMN planned_date TEXT", [])?;
        }
        if !has_column("reminder_at_unix_ms")? {
            transaction.execute(
                "ALTER TABLE tasks ADD COLUMN reminder_at_unix_ms INTEGER",
                [],
            )?;
        }
        if !has_column("reminder_fired_at_unix_ms")? {
            transaction.execute(
                "ALTER TABLE tasks ADD COLUMN reminder_fired_at_unix_ms INTEGER",
                [],
            )?;
        }
        if !has_column("priority")? {
            transaction.execute(
                "ALTER TABLE tasks ADD COLUMN priority INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }

        // SQLite cannot add CHECK constraints to an existing table. Rebuild it so
        // upgraded v2 databases have the same invariants as newly created ones.
        transaction.execute_batch(
            "
            DROP INDEX IF EXISTS idx_tasks_list;
            DROP TABLE IF EXISTS tasks_v3_rebuild;
            CREATE TABLE tasks_v3_rebuild (
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                notes TEXT NOT NULL DEFAULT '',
                planned_date TEXT,
                due_at_unix_ms INTEGER,
                reminder_at_unix_ms INTEGER,
                reminder_fired_at_unix_ms INTEGER,
                priority INTEGER NOT NULL DEFAULT 0,
                completed_at_unix_ms INTEGER,
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                CHECK (length(trim(title)) BETWEEN 1 AND 200),
                CHECK (length(notes) <= 10000),
                CHECK (
                    planned_date IS NULL OR (
                        length(planned_date) = 10 AND
                        planned_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
                    )
                ),
                CHECK (due_at_unix_ms IS NULL OR due_at_unix_ms BETWEEN 0 AND 8640000000000000),
                CHECK (reminder_at_unix_ms IS NULL OR reminder_at_unix_ms BETWEEN 0 AND 8640000000000000),
                CHECK (reminder_fired_at_unix_ms IS NULL OR reminder_fired_at_unix_ms BETWEEN 0 AND 8640000000000000),
                CHECK (priority BETWEEN 0 AND 3),
                CHECK (completed_at_unix_ms IS NULL OR completed_at_unix_ms >= 0)
            );
            INSERT INTO tasks_v3_rebuild (
                id, title, notes, planned_date, due_at_unix_ms,
                reminder_at_unix_ms, reminder_fired_at_unix_ms, priority,
                completed_at_unix_ms, created_at_unix_ms, updated_at_unix_ms
            )
            SELECT
                id, title, notes, planned_date, due_at_unix_ms,
                reminder_at_unix_ms, reminder_fired_at_unix_ms, priority,
                completed_at_unix_ms, created_at_unix_ms, updated_at_unix_ms
            FROM tasks;
            DROP TABLE tasks;
            ALTER TABLE tasks_v3_rebuild RENAME TO tasks;
            CREATE INDEX idx_tasks_list ON tasks (
                completed_at_unix_ms,
                planned_date,
                due_at_unix_ms,
                priority,
                created_at_unix_ms,
                id
            );
            ",
        )?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_unix_ms) VALUES (?1, ?2)",
            (TASK_FIELDS_MIGRATION_VERSION, now_unix_ms() as i64),
        )?;
    }

    if !migration_applied(TASK_ORGANIZATION_MIGRATION_VERSION)? {
        let has_deleted_at_column: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('tasks') WHERE name = 'deleted_at_unix_ms')",
            [],
            |row| row.get(0),
        )?;
        if !has_deleted_at_column {
            transaction.execute(
                "ALTER TABLE tasks ADD COLUMN deleted_at_unix_ms INTEGER",
                [],
            )?;
        }
        transaction.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_tasks_deleted ON tasks (deleted_at_unix_ms, id);",
        )?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_unix_ms) VALUES (?1, ?2)",
            (TASK_ORGANIZATION_MIGRATION_VERSION, now_unix_ms() as i64),
        )?;
    }

    let recurrence_migration_applied = migration_applied(TASK_RECURRENCE_MIGRATION_VERSION)?;
    let has_column = |column: &str| -> rusqlite::Result<bool> {
        transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('tasks') WHERE name = ?1)",
            [column],
            |row| row.get(0),
        )
    };
    if !has_column("recurrence_kind")? {
        transaction.execute("ALTER TABLE tasks ADD COLUMN recurrence_kind TEXT", [])?;
    }
    if !has_column("recurrence_series_id")? {
        transaction.execute(
            "ALTER TABLE tasks ADD COLUMN recurrence_series_id INTEGER",
            [],
        )?;
    }
    if !has_column("recurrence_source_task_id")? {
        transaction.execute(
            "ALTER TABLE tasks ADD COLUMN recurrence_source_task_id INTEGER",
            [],
        )?;
    }
    if !has_column("recurrence_timezone")? {
        transaction.execute("ALTER TABLE tasks ADD COLUMN recurrence_timezone TEXT", [])?;
    }
    if !has_column("recurrence_dst_policy")? {
        transaction.execute(
            "ALTER TABLE tasks ADD COLUMN recurrence_dst_policy TEXT",
            [],
        )?;
    }
    transaction.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS recurrence_templates (
            series_id INTEGER PRIMARY KEY,
            reminder_at_unix_ms INTEGER
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_recurrence_source
            ON tasks (recurrence_source_task_id)
            WHERE recurrence_source_task_id IS NOT NULL;
        DROP INDEX IF EXISTS idx_tasks_recurrence_occurrence;
        CREATE UNIQUE INDEX idx_tasks_recurrence_occurrence
            ON tasks (recurrence_series_id, planned_date)
            WHERE recurrence_series_id IS NOT NULL
              AND planned_date IS NOT NULL;
        ",
    )?;
    transaction.execute(
        "
        INSERT OR IGNORE INTO recurrence_templates (series_id, reminder_at_unix_ms)
        SELECT COALESCE(recurrence_series_id, id), reminder_at_unix_ms
        FROM tasks
        WHERE recurrence_kind IN ('daily', 'weekly')
        ",
        [],
    )?;
    if !recurrence_migration_applied {
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_unix_ms) VALUES (?1, ?2)",
            (TASK_RECURRENCE_MIGRATION_VERSION, now_unix_ms() as i64),
        )?;
    }

    if !migration_applied(PROJECTS_AND_RELIABILITY_MIGRATION_VERSION)? {
        transaction.execute_batch(
            "
            CREATE TABLE projects (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                archived_at_unix_ms INTEGER,
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                CHECK (length(trim(name)) BETWEEN 1 AND 100)
            );
            CREATE UNIQUE INDEX idx_projects_active_name
                ON projects (name COLLATE NOCASE)
                WHERE archived_at_unix_ms IS NULL;

            DROP INDEX IF EXISTS idx_tasks_list;
            DROP INDEX IF EXISTS idx_tasks_deleted;
            DROP INDEX IF EXISTS idx_tasks_recurrence_source;
            DROP INDEX IF EXISTS idx_tasks_recurrence_occurrence;
            DROP INDEX IF EXISTS idx_tasks_project_visibility;
            DROP TABLE IF EXISTS tasks_v6_rebuild;
            CREATE TABLE tasks_v6_rebuild (
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                notes TEXT NOT NULL DEFAULT '',
                planned_date TEXT,
                due_at_unix_ms INTEGER,
                reminder_at_unix_ms INTEGER,
                reminder_fired_at_unix_ms INTEGER,
                deleted_at_unix_ms INTEGER,
                project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL,
                priority INTEGER NOT NULL DEFAULT 0,
                recurrence_kind TEXT,
                recurrence_series_id INTEGER,
                recurrence_source_task_id INTEGER,
                recurrence_timezone TEXT,
                recurrence_dst_policy TEXT,
                completed_at_unix_ms INTEGER,
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                CHECK (length(trim(title)) BETWEEN 1 AND 200),
                CHECK (length(notes) <= 10000),
                CHECK (
                    planned_date IS NULL OR (
                        length(planned_date) = 10 AND
                        planned_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
                    )
                ),
                CHECK (due_at_unix_ms IS NULL OR due_at_unix_ms BETWEEN 0 AND 8640000000000000),
                CHECK (reminder_at_unix_ms IS NULL OR reminder_at_unix_ms BETWEEN 0 AND 8640000000000000),
                CHECK (reminder_fired_at_unix_ms IS NULL OR reminder_fired_at_unix_ms BETWEEN 0 AND 8640000000000000),
                CHECK (deleted_at_unix_ms IS NULL OR deleted_at_unix_ms >= 0),
                CHECK (priority BETWEEN 0 AND 3),
                CHECK (completed_at_unix_ms IS NULL OR completed_at_unix_ms >= 0)
            );
            INSERT INTO tasks_v6_rebuild (
                id, title, notes, planned_date, due_at_unix_ms,
                reminder_at_unix_ms, reminder_fired_at_unix_ms, deleted_at_unix_ms,
                project_id, priority, recurrence_kind, recurrence_series_id,
                recurrence_source_task_id, recurrence_timezone, recurrence_dst_policy,
                completed_at_unix_ms, created_at_unix_ms, updated_at_unix_ms
            )
            SELECT
                id, title, notes, planned_date, due_at_unix_ms,
                reminder_at_unix_ms, reminder_fired_at_unix_ms, deleted_at_unix_ms,
                NULL, priority, recurrence_kind, recurrence_series_id,
                recurrence_source_task_id, recurrence_timezone, recurrence_dst_policy,
                completed_at_unix_ms, created_at_unix_ms, updated_at_unix_ms
            FROM tasks;
            DROP TABLE tasks;
            ALTER TABLE tasks_v6_rebuild RENAME TO tasks;
            CREATE INDEX idx_tasks_list ON tasks (
                completed_at_unix_ms,
                planned_date,
                due_at_unix_ms,
                priority,
                created_at_unix_ms,
                id
            );
            CREATE INDEX idx_tasks_deleted ON tasks (deleted_at_unix_ms, id);
            CREATE INDEX idx_tasks_project_visibility
                ON tasks (
                    project_id,
                    deleted_at_unix_ms,
                    completed_at_unix_ms,
                    planned_date,
                    due_at_unix_ms,
                    priority,
                    created_at_unix_ms,
                    id
                );
            CREATE UNIQUE INDEX idx_tasks_recurrence_source
                ON tasks (recurrence_source_task_id)
                WHERE recurrence_source_task_id IS NOT NULL;
            CREATE UNIQUE INDEX idx_tasks_recurrence_occurrence
                ON tasks (recurrence_series_id, planned_date)
                WHERE recurrence_series_id IS NOT NULL
                  AND planned_date IS NOT NULL;

            CREATE TABLE reminder_sync_jobs (
                task_id INTEGER PRIMARY KEY,
                intent TEXT NOT NULL CHECK (intent IN ('sync', 'cancel')),
                generation INTEGER NOT NULL,
                attempt_count INTEGER NOT NULL DEFAULT 0,
                next_attempt_at_unix_ms INTEGER NOT NULL,
                last_error TEXT,
                updated_at_unix_ms INTEGER NOT NULL
            );
            CREATE INDEX idx_reminder_sync_jobs_ready
                ON reminder_sync_jobs (next_attempt_at_unix_ms, updated_at_unix_ms);

            CREATE TABLE reminder_deliveries (
                id TEXT PRIMARY KEY,
                task_id INTEGER NOT NULL,
                reminder_at_unix_ms INTEGER NOT NULL,
                activation_token_hash BLOB NOT NULL,
                state TEXT NOT NULL CHECK (state IN (
                    'pending_schedule', 'scheduled', 'pending_cancel',
                    'cancelled', 'activated', 'expired', 'superseded'
                )),
                created_at_unix_ms INTEGER NOT NULL,
                scheduled_at_unix_ms INTEGER,
                activated_at_unix_ms INTEGER
            );
            CREATE UNIQUE INDEX idx_reminder_deliveries_live_task
                ON reminder_deliveries (task_id)
                WHERE state IN ('pending_schedule', 'scheduled', 'pending_cancel');

            CREATE TABLE activation_inbox (
                id INTEGER PRIMARY KEY,
                delivery_id TEXT NOT NULL UNIQUE,
                task_id INTEGER NOT NULL,
                received_at_unix_ms INTEGER NOT NULL,
                claimed_by TEXT,
                claim_expires_at_unix_ms INTEGER,
                acknowledged_at_unix_ms INTEGER
            );
            CREATE INDEX idx_activation_inbox_claimable
                ON activation_inbox (
                    acknowledged_at_unix_ms,
                    claim_expires_at_unix_ms,
                    received_at_unix_ms
                );

            CREATE TABLE reliability_incidents (
                id INTEGER PRIMARY KEY,
                kind TEXT NOT NULL CHECK (kind IN ('reminder_host', 'database')),
                dedupe_key TEXT NOT NULL UNIQUE,
                task_id INTEGER,
                operation TEXT NOT NULL,
                message TEXT NOT NULL,
                status TEXT NOT NULL CHECK (status IN ('open', 'resolved', 'acknowledged')),
                first_seen_at_unix_ms INTEGER NOT NULL,
                last_seen_at_unix_ms INTEGER NOT NULL,
                occurrence_count INTEGER NOT NULL DEFAULT 1,
                resolved_at_unix_ms INTEGER,
                acknowledged_at_unix_ms INTEGER
            );
            ",
        )?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_unix_ms) VALUES (?1, ?2)",
            (
                PROJECTS_AND_RELIABILITY_MIGRATION_VERSION,
                now_unix_ms() as i64,
            ),
        )?;
    }

    if !migration_applied(REMINDER_DELIVERY_RETRY_MIGRATION_VERSION)? {
        transaction.execute_batch(
            "
            ALTER TABLE reminder_deliveries
                ADD COLUMN generation INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE reminder_deliveries
                ADD COLUMN activation_uri_hash BLOB;
            ",
        )?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_unix_ms) VALUES (?1, ?2)",
            (
                REMINDER_DELIVERY_RETRY_MIGRATION_VERSION,
                now_unix_ms() as i64,
            ),
        )?;
    }

    transaction.commit()
}

fn execute_database_probe(
    connection: &mut Connection,
    database_path: &Path,
) -> rusqlite::Result<DatabaseProbe> {
    apply_migrations(connection)?;

    let sampled_at = now_unix_ms();
    connection.execute(
        "
        INSERT INTO stage0_probe (id, run_count, last_probe_at_unix_ms)
        VALUES (1, 1, ?1)
        ON CONFLICT(id) DO UPDATE SET
            run_count = stage0_probe.run_count + 1,
            last_probe_at_unix_ms = excluded.last_probe_at_unix_ms
        ",
        [sampled_at as i64],
    )?;

    let (run_count, last_probe_at_unix_ms) = connection.query_row(
        "SELECT run_count, last_probe_at_unix_ms FROM stage0_probe WHERE id = 1",
        [],
        |row| Ok((row.get(0)?, row.get::<_, i64>(1)?)),
    )?;
    let migration_count =
        connection.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get(0)
        })?;

    Ok(DatabaseProbe {
        database_path: database_path.display().to_string(),
        migration_count,
        run_count,
        last_probe_at_unix_ms: last_probe_at_unix_ms as u128,
    })
}

fn show_existing_window(window: &WebviewWindow) -> Result<(), String> {
    window.unminimize().map_err(string_error)?;
    window.show().map_err(string_error)?;
    window.set_focus().map_err(string_error)
}

fn install_close_to_tray_behavior(window: &WebviewWindow) {
    let window_for_events = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = window_for_events.hide();
        }
    });
}

fn enqueue_pending_reminder_warning(
    next_id: &AtomicU64,
    warning_state: &Mutex<ReminderWarningState>,
    task_id: i64,
    message: String,
) -> Result<(QueuedReminderWarning, bool), String> {
    let warning = QueuedReminderWarning {
        id: next_id.fetch_add(1, Ordering::Relaxed),
        task_id,
        message,
        claim: None,
    };
    let mut warning_state = warning_state.lock().map_err(string_error)?;
    let event_listener_ready = warning_state
        .listener
        .as_ref()
        .map(|listener| listener.event_listener_ready)
        .unwrap_or(false);
    warning_state.pending.push(warning.clone());
    Ok((warning, event_listener_ready))
}

fn release_reminder_warning_claims_for(
    warning_state: &mut ReminderWarningState,
    listener_token: u64,
) {
    for warning in &mut warning_state.pending {
        if warning
            .claim
            .as_ref()
            .map(|claim| claim.listener_token == listener_token)
            .unwrap_or(false)
        {
            warning.claim = None;
        }
    }
}

fn release_expired_reminder_warning_claims(warning_state: &mut ReminderWarningState, now: u128) {
    for warning in &mut warning_state.pending {
        if warning
            .claim
            .as_ref()
            .map(|claim| claim.expires_at_unix_ms <= now)
            .unwrap_or(false)
        {
            warning.claim = None;
        }
    }
}

fn register_reminder_warning_listener(
    warning_state: &Mutex<ReminderWarningState>,
    next_listener_token: &AtomicU64,
    ui_run_id: String,
    ui_generation: u64,
    event_listener_ready: bool,
) -> Result<u64, String> {
    if ui_run_id.trim().is_empty() {
        return Err("ui run id must not be empty".to_string());
    }
    if ui_generation == 0 {
        return Err("ui generation must be positive".to_string());
    }

    let mut warning_state = warning_state.lock().map_err(string_error)?;
    if let Some(listener) = warning_state.listener.clone() {
        if listener.run_id == ui_run_id && listener.generation == ui_generation {
            let listener = warning_state
                .listener
                .as_mut()
                .expect("listener must remain present while state is locked");
            listener.event_listener_ready |= event_listener_ready;
            return Ok(listener.token);
        }
        if ui_generation <= listener.generation {
            return Err("stale reminder warning listener registration".to_string());
        }
    } else if ui_generation <= warning_state.highest_listener_generation {
        return Err("stale reminder warning listener registration".to_string());
    }

    let listener_token = next_listener_token.fetch_add(1, Ordering::Relaxed);
    warning_state.listener = Some(ReminderWarningListener {
        run_id: ui_run_id,
        generation: ui_generation,
        token: listener_token,
        event_listener_ready,
    });
    warning_state.highest_listener_generation = ui_generation;
    Ok(listener_token)
}

fn claim_pending_reminder_warnings_from(
    warning_state: &Mutex<ReminderWarningState>,
    listener_token: u64,
    now: u128,
) -> Result<Vec<QueuedReminderWarning>, String> {
    let mut warning_state = warning_state.lock().map_err(string_error)?;
    if warning_state
        .listener
        .as_ref()
        .map(|listener| listener.token)
        != Some(listener_token)
    {
        return Err("reminder warning listener is not current".to_string());
    }

    release_expired_reminder_warning_claims(&mut warning_state, now);
    let claim = ReminderWarningClaim {
        listener_token,
        expires_at_unix_ms: now.saturating_add(REMINDER_WARNING_CLAIM_LEASE_MS),
    };
    let mut claimed = Vec::new();
    for warning in &mut warning_state.pending {
        let belongs_to_listener = warning
            .claim
            .as_ref()
            .map(|warning_claim| warning_claim.listener_token == listener_token)
            .unwrap_or(true);
        if belongs_to_listener {
            warning.claim = Some(claim.clone());
            claimed.push(warning.clone());
        }
    }
    Ok(claimed)
}

fn acknowledge_reminder_warnings_from(
    warning_state: &Mutex<ReminderWarningState>,
    listener_token: u64,
    ids: &[u64],
    now: u128,
) -> Result<(), String> {
    if ids.is_empty() {
        return Ok(());
    }
    let ids: HashSet<u64> = ids.iter().copied().collect();
    let mut warning_state = warning_state.lock().map_err(string_error)?;
    warning_state.pending.retain(|warning| {
        !ids.contains(&warning.id)
            || warning
                .claim
                .as_ref()
                .map(|claim| {
                    claim.listener_token != listener_token || claim.expires_at_unix_ms <= now
                })
                .unwrap_or(true)
    });
    Ok(())
}

fn release_reminder_warning_listener_locked(
    warning_state: &mut ReminderWarningState,
    listener_token: u64,
) -> bool {
    release_reminder_warning_claims_for(warning_state, listener_token);
    let is_current = warning_state
        .listener
        .as_ref()
        .map(|listener| listener.token)
        == Some(listener_token);
    if is_current {
        warning_state.listener = None;
    }
    is_current
}

fn release_reminder_warning_listener(
    warning_state: &Mutex<ReminderWarningState>,
    listener_token: u64,
) -> Result<bool, String> {
    let mut warning_state = warning_state.lock().map_err(string_error)?;
    Ok(release_reminder_warning_listener_locked(
        &mut warning_state,
        listener_token,
    ))
}

fn release_current_reminder_warning_listener_locked(warning_state: &mut ReminderWarningState) {
    for warning in &mut warning_state.pending {
        warning.claim = None;
    }
    warning_state.listener = None;
}

fn destroy_main_window_and_release_listener(
    app: &AppHandle,
    warning_state: &Mutex<ReminderWarningState>,
    listener_token: u64,
) -> Result<(), String> {
    let mut warning_state = warning_state.lock().map_err(string_error)?;
    let is_current = warning_state
        .listener
        .as_ref()
        .map(|listener| listener.token)
        == Some(listener_token);
    if !is_current {
        return Err("reminder warning listener is not current".to_string());
    }
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window.destroy().map_err(string_error)?;
    }
    release_reminder_warning_listener_locked(&mut warning_state, listener_token);
    Ok(())
}

fn destroy_main_window_and_release_current(
    app: &AppHandle,
    warning_state: &Mutex<ReminderWarningState>,
) -> Result<(), String> {
    let mut warning_state = warning_state.lock().map_err(string_error)?;
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window.destroy().map_err(string_error)?;
    }
    release_current_reminder_warning_listener_locked(&mut warning_state);
    Ok(())
}

fn emit_reminder_warning(app: &AppHandle, task_id: i64, message: String) {
    let state = app.state::<AppState>();
    match enqueue_pending_reminder_warning(
        &state.next_reminder_warning_id,
        &state.reminder_warning_state,
        task_id,
        message,
    ) {
        Ok((warning, true)) => {
            let _ = app.emit("reminder-warning", warning);
        }
        Ok((_, false)) => {}
        Err(error) => eprintln!("could not queue reminder warning: {error}"),
    }
}

fn queue_notification_activation(app: &AppHandle, url: &Url) {
    let Some(activation) = reminder_jobs::parse_notification_activation(url) else {
        return;
    };

    let task_id = match app.state::<AppState>().database.lock() {
        Ok(mut connection) => match reminder_jobs::consume_notification_activation(
            &mut connection,
            &activation,
            tasks::now_unix_ms(),
        ) {
            Ok(task_id) => task_id,
            Err(error) => {
                eprintln!("notification activation consumption failed: {error}");
                return;
            }
        },
        Err(error) => {
            eprintln!("notification activation database lock failed: {error}");
            return;
        }
    };

    let Some(task_id) = task_id else {
        return;
    };

    let _ = app.emit("notification-activation", ());
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = drain_reminder_jobs_for_app(app_handle).await {
            eprintln!(
                "notification activation reminder cleanup failed for task {task_id}: {error}"
            );
        }
    });
    show_or_create_main_window(app);
}

fn queue_notification_urls(app: &AppHandle, urls: impl IntoIterator<Item = Url>) {
    for url in urls {
        queue_notification_activation(app, &url);
    }
}

fn install_activation_listener(app: &tauri::App) {
    let app_handle = app.handle().clone();
    app.listen("deep-link://new-url", move |event| {
        let urls = serde_json::from_str::<Vec<String>>(event.payload()).unwrap_or_default();
        queue_notification_urls(
            &app_handle,
            urls.into_iter().filter_map(|raw| Url::parse(&raw).ok()),
        );
    });
}

fn queue_current_activation(app: &tauri::App) {
    if let Ok(Some(urls)) = app.deep_link().get_current() {
        queue_notification_urls(app.handle(), urls);
    }
}

fn window_preferences_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join(WINDOW_PREFERENCES_FILE_NAME))
        .map_err(string_error)
}

fn validate_window_preferences(preferences: &WindowPreferences) -> Result<(), String> {
    if !matches!(preferences.mode.as_str(), "full" | "compact") {
        return Err("window mode must be full or compact".to_string());
    }
    if !(260..=2_000).contains(&preferences.width) {
        return Err("window width must be between 260 and 2000".to_string());
    }
    if !(180..=1_600).contains(&preferences.height) {
        return Err("window height must be between 180 and 1600".to_string());
    }
    Ok(())
}

fn read_window_preferences(app: &AppHandle) -> WindowPreferences {
    let Ok(path) = window_preferences_path(app) else {
        return WindowPreferences::default();
    };
    let Ok(content) = fs::read_to_string(path) else {
        return WindowPreferences::default();
    };
    serde_json::from_str::<WindowPreferences>(&content)
        .ok()
        .filter(|preferences| validate_window_preferences(preferences).is_ok())
        .unwrap_or_default()
}

fn save_window_preferences_to_disk(
    app: &AppHandle,
    preferences: &WindowPreferences,
) -> Result<(), String> {
    validate_window_preferences(preferences)?;
    let path = window_preferences_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(string_error)?;
    }
    let content = serde_json::to_vec_pretty(preferences).map_err(string_error)?;
    fs::write(path, content).map_err(string_error)
}

fn apply_window_preferences(
    window: &WebviewWindow,
    preferences: &WindowPreferences,
) -> Result<(), String> {
    validate_window_preferences(preferences)?;
    window
        .set_size(LogicalSize::new(preferences.width, preferences.height))
        .map_err(string_error)?;
    window
        .set_resizable(preferences.mode == "full")
        .map_err(string_error)?;
    window
        .set_always_on_top(preferences.always_on_top)
        .map_err(string_error)
}

fn set_window_mode_internal(app: &AppHandle, mode: &str) -> Result<(), String> {
    if !matches!(mode, "full" | "compact") {
        return Err("window mode must be full or compact".to_string());
    }
    let mut preferences = read_window_preferences(app);
    preferences.mode = mode.to_string();
    if mode == "full" {
        preferences.width = 800;
        preferences.height = 600;
    } else {
        preferences.width = 380;
        preferences.height = 520;
    }
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        apply_window_preferences(&window, &preferences)?;
    }
    save_window_preferences_to_disk(app, &preferences)
}

fn build_main_window_from_config(app: &AppHandle) -> Result<(), String> {
    let preferences = read_window_preferences(app);
    if app.get_webview_window(MAIN_WINDOW_LABEL).is_none() {
        let config = app
            .config()
            .app
            .windows
            .first()
            .ok_or_else(|| "tauri.conf.json does not define an app window".to_string())?;
        let window = WebviewWindowBuilder::from_config(app, config)
            .map_err(string_error)?
            .build()
            .map_err(string_error)?;
        apply_window_preferences(&window, &preferences)?;
        install_close_to_tray_behavior(&window);
    }

    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "main window was not created".to_string())?;
    apply_window_preferences(&window, &preferences)?;
    show_existing_window(&window)
}

/// Defers construction because Windows tray and single-instance callbacks can be synchronous.
fn schedule_main_window_rebuild(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state
        .ui_rebuild_in_progress
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }

    let app = app.clone();
    thread::spawn(move || {
        let _ = build_main_window_from_config(&app);
        app.state::<AppState>()
            .ui_rebuild_in_progress
            .store(false, Ordering::Release);
    });
}

fn show_or_create_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = show_existing_window(&window);
    } else {
        schedule_main_window_rebuild(app);
    }
}

fn hide_main_window(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window.hide().map_err(string_error)?;
    }
    Ok(())
}

fn runtime_snapshot(app: &AppHandle, state: &AppState) -> RuntimeSnapshot {
    let (main_window_exists, main_window_visible) = match app.get_webview_window(MAIN_WINDOW_LABEL)
    {
        Some(window) => (true, window.is_visible().unwrap_or(false)),
        None => (false, false),
    };
    let ui_ready = state
        .reminder_warning_state
        .lock()
        .map(|warning_state| warning_state.listener.is_some())
        .unwrap_or(false);

    RuntimeSnapshot {
        process_id: std::process::id(),
        main_window_exists,
        main_window_visible,
        ui_ready,
        ui_rebuild_in_progress: state.ui_rebuild_in_progress.load(Ordering::Acquire),
        database_path: state.database_path.display().to_string(),
    }
}

#[tauri::command]
fn get_runtime_snapshot(app: AppHandle, state: State<'_, AppState>) -> RuntimeSnapshot {
    runtime_snapshot(&app, &state)
}

#[tauri::command]
fn record_ui_ready(
    app: AppHandle,
    ui_run_id: String,
    ui_generation: u64,
    event_listener_ready: bool,
    state: State<'_, AppState>,
) -> Result<UiReadyRegistration, String> {
    let reminder_warning_listener_token = register_reminder_warning_listener(
        &state.reminder_warning_state,
        &state.next_reminder_warning_listener_token,
        ui_run_id,
        ui_generation,
        event_listener_ready,
    )?;
    Ok(UiReadyRegistration {
        runtime_snapshot: runtime_snapshot(&app, &state),
        reminder_warning_listener_token,
    })
}

#[tauri::command]
fn sample_process_metrics() -> Result<ProcessMetricsSnapshot, String> {
    let current_pid = get_current_pid().map_err(string_error)?;
    let refresh_kind = ProcessRefreshKind::nothing().with_memory().with_cpu();
    let mut system = System::new_with_specifics(RefreshKind::nothing());

    // CPU usage is a delta; these are two command-local samples, not a background poller.
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind);
    thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL);
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind);

    let mut related_pids = HashSet::from([current_pid]);
    loop {
        let before = related_pids.len();
        let child_pids: Vec<Pid> = system
            .processes()
            .iter()
            .filter_map(|(pid, process)| {
                process
                    .parent()
                    .filter(|parent| related_pids.contains(parent))
                    .map(|_| *pid)
            })
            .collect();
        related_pids.extend(child_pids);
        if related_pids.len() == before {
            break;
        }
    }

    let mut processes: Vec<ProcessMetric> = related_pids
        .iter()
        .filter_map(|pid| system.process(*pid))
        .map(|process| ProcessMetric {
            process_id: process.pid().as_u32(),
            parent_process_id: process.parent().map(Pid::as_u32),
            name: process.name().to_string_lossy().into_owned(),
            memory_bytes: process.memory(),
            cpu_percent: process.cpu_usage(),
        })
        .collect();
    processes.sort_by_key(|process| process.process_id);

    let total_memory_bytes = processes.iter().map(|process| process.memory_bytes).sum();
    let total_cpu_percent = processes.iter().map(|process| process.cpu_percent).sum();

    Ok(ProcessMetricsSnapshot {
        sampled_at_unix_ms: now_unix_ms(),
        process_count: processes.len(),
        total_memory_bytes,
        total_cpu_percent,
        processes,
    })
}

#[tauri::command]
fn run_database_probe(state: State<'_, AppState>) -> Result<DatabaseProbe, String> {
    let mut connection = state.database.lock().map_err(string_error)?;
    execute_database_probe(&mut connection, &state.database_path).map_err(string_error)
}

#[tauri::command]
fn list_projects(
    include_archived: bool,
    state: State<'_, AppState>,
) -> Result<Vec<projects::Project>, String> {
    let connection = state.database.lock().map_err(string_error)?;
    projects::list(&connection, include_archived)
}

#[tauri::command]
fn create_project(
    input: projects::CreateProjectInput,
    state: State<'_, AppState>,
) -> Result<projects::Project, String> {
    let mut connection = state.database.lock().map_err(string_error)?;
    projects::create(&mut connection, &input)
}

#[tauri::command]
fn update_project(
    id: i64,
    input: projects::UpdateProjectInput,
    state: State<'_, AppState>,
) -> Result<projects::Project, String> {
    let mut connection = state.database.lock().map_err(string_error)?;
    projects::update(&mut connection, id, &input)
}

#[tauri::command]
fn archive_project(id: i64, state: State<'_, AppState>) -> Result<projects::Project, String> {
    let mut connection = state.database.lock().map_err(string_error)?;
    projects::archive(&mut connection, id)
}

#[tauri::command]
fn restore_project(id: i64, state: State<'_, AppState>) -> Result<projects::Project, String> {
    let mut connection = state.database.lock().map_err(string_error)?;
    projects::restore(&mut connection, id)
}

#[tauri::command]
fn list_tasks(state: State<'_, AppState>) -> Result<Vec<tasks::Task>, String> {
    let connection = state.database.lock().map_err(string_error)?;
    tasks::list(&connection)
}

#[tauri::command]
fn list_deleted_tasks(state: State<'_, AppState>) -> Result<Vec<tasks::Task>, String> {
    let connection = state.database.lock().map_err(string_error)?;
    tasks::list_deleted(&connection)
}

#[tauri::command]
async fn reconcile_reminders(
    app: AppHandle,
    _state: State<'_, AppState>,
) -> Result<ReconcileReport, String> {
    reconcile_reminders_for_app(app).await
}

#[tauri::command]
fn list_missed_reminders(state: State<'_, AppState>) -> Result<Vec<i64>, String> {
    let connection = state.database.lock().map_err(string_error)?;
    tasks::list_missed(&connection, tasks::now_unix_ms())
}

#[tauri::command]
fn list_reliability_incidents(
    include_resolved: bool,
    state: State<'_, AppState>,
) -> Result<Vec<reminder_jobs::ReliabilityIncident>, String> {
    let connection = state.database.lock().map_err(string_error)?;
    reminder_jobs::list_reliability_incidents(&connection, include_resolved)
}

#[tauri::command]
fn acknowledge_reliability_incidents(
    ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut connection = state.database.lock().map_err(string_error)?;
    reminder_jobs::acknowledge_reliability_incidents(&mut connection, &ids, tasks::now_unix_ms())
}

#[tauri::command]
fn claim_pending_activations(
    consumer_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<reminder_jobs::PendingActivation>, String> {
    let mut connection = state.database.lock().map_err(string_error)?;
    reminder_jobs::claim_pending_activations(&mut connection, &consumer_id, 1, tasks::now_unix_ms())
}

#[tauri::command]
fn acknowledge_pending_activations(
    consumer_id: String,
    ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<reminder_jobs::ActivationAckResult, String> {
    let mut connection = state.database.lock().map_err(string_error)?;
    reminder_jobs::acknowledge_pending_activations(
        &mut connection,
        &consumer_id,
        &ids,
        tasks::now_unix_ms(),
    )
}

#[tauri::command]
fn claim_pending_reminder_warnings(
    listener_token: u64,
    state: State<'_, AppState>,
) -> Result<Vec<QueuedReminderWarning>, String> {
    claim_pending_reminder_warnings_from(
        &state.reminder_warning_state,
        listener_token,
        now_unix_ms(),
    )
}

#[tauri::command]
fn acknowledge_reminder_warnings(
    listener_token: u64,
    ids: Vec<u64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    acknowledge_reminder_warnings_from(
        &state.reminder_warning_state,
        listener_token,
        &ids,
        now_unix_ms(),
    )
}

#[tauri::command]
fn record_ui_not_ready(listener_token: u64, state: State<'_, AppState>) -> Result<(), String> {
    release_reminder_warning_listener(&state.reminder_warning_state, listener_token).map(|_| ())
}

#[tauri::command]
fn parse_task_drafts(text: String, today_local: String) -> Result<Vec<parser::TaskDraft>, String> {
    parser::parse_task_drafts(&text, &today_local)
}

async fn task_mutation(app: AppHandle, task: tasks::Task) -> TaskMutation {
    let reminder_warning = sync_latest_task_reminder_for_app(app, task.id)
        .await
        .unwrap_or_else(Some);
    TaskMutation {
        task,
        reminder_warning,
    }
}

#[tauri::command]
async fn create_task(
    app: AppHandle,
    input: tasks::CreateTaskInput,
    state: State<'_, AppState>,
) -> Result<TaskMutation, String> {
    let task = {
        let mut connection = state.database.lock().map_err(string_error)?;
        tasks::create(&mut connection, input)?
    };
    Ok(task_mutation(app, task).await)
}

#[tauri::command]
async fn update_task(
    app: AppHandle,
    id: i64,
    input: tasks::UpdateTaskInput,
    state: State<'_, AppState>,
) -> Result<TaskMutation, String> {
    let task = {
        let mut connection = state.database.lock().map_err(string_error)?;
        tasks::update(&mut connection, id, input)?
    };
    Ok(task_mutation(app, task).await)
}

#[tauri::command]
fn set_task_planned_date(
    id: i64,
    planned_date: Option<String>,
    state: State<'_, AppState>,
) -> Result<tasks::Task, String> {
    let mut connection = state.database.lock().map_err(string_error)?;
    tasks::set_planned_date(&mut connection, id, planned_date)
}

#[tauri::command]
async fn set_task_completed(
    app: AppHandle,
    id: i64,
    completed: bool,
    state: State<'_, AppState>,
) -> Result<TaskCompletionCommandResult, String> {
    let mutation = {
        let mut connection = state.database.lock().map_err(string_error)?;
        tasks::set_completed(&mut connection, id, completed)?
    };
    let reminder_warning = sync_latest_task_reminder_for_app(app.clone(), mutation.task.id)
        .await
        .unwrap_or_else(Some);
    let next_reminder_warning = match mutation.next_task.as_ref() {
        Some(next_task) => sync_latest_task_reminder_for_app(app, next_task.id)
            .await
            .unwrap_or_else(Some),
        None => None,
    };
    Ok(TaskCompletionCommandResult {
        task: mutation.task,
        next_task: mutation.next_task,
        reminder_warning,
        next_reminder_warning,
    })
}

#[tauri::command]
async fn snooze_task(
    app: AppHandle,
    id: i64,
    until_unix_ms: i64,
    state: State<'_, AppState>,
) -> Result<TaskMutation, String> {
    let task = {
        let mut connection = state.database.lock().map_err(string_error)?;
        tasks::snooze(&mut connection, id, until_unix_ms)?
    };
    Ok(task_mutation(app, task).await)
}

#[tauri::command]
async fn defer_task_to_tomorrow(
    app: AppHandle,
    id: i64,
    timezone: String,
    state: State<'_, AppState>,
) -> Result<TaskMutation, String> {
    let task = {
        let mut connection = state.database.lock().map_err(string_error)?;
        tasks::defer_to_tomorrow(&mut connection, id, timezone)?
    };
    Ok(task_mutation(app, task).await)
}

#[tauri::command]
async fn delete_task(
    app: AppHandle,
    id: i64,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    {
        let mut connection = state.database.lock().map_err(string_error)?;
        tasks::delete(&mut connection, id)?;
    }
    Ok(sync_latest_task_reminder_for_app(app, id)
        .await
        .unwrap_or_else(Some))
}

#[tauri::command]
async fn restore_task(
    app: AppHandle,
    id: i64,
    state: State<'_, AppState>,
) -> Result<TaskMutation, String> {
    let task = {
        let mut connection = state.database.lock().map_err(string_error)?;
        tasks::restore(&mut connection, id)?
    };
    Ok(task_mutation(app, task).await)
}

#[tauri::command]
async fn permanently_delete_task(
    app: AppHandle,
    id: i64,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    {
        let mut connection = state.database.lock().map_err(string_error)?;
        tasks::permanently_delete(&mut connection, id)?;
    }
    Ok(sync_latest_task_reminder_for_app(app, id)
        .await
        .unwrap_or_else(Some))
}

#[cfg(windows)]
fn notification_host_path(app: &AppHandle) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();

    #[cfg(debug_assertions)]
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../tools/notification-host/publish")
            .join(NOTIFICATION_HOST_FILE_NAME),
    );

    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(
            resource_dir
                .join("notification-host")
                .join(NOTIFICATION_HOST_FILE_NAME),
        );
    }

    let searched_paths = candidates
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");

    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| format!("notification host was not found; searched: {searched_paths}"))
}

#[cfg(windows)]
fn terminate_notification_host(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(windows)]
fn wait_for_notification_host(child: &mut Child) -> Result<ExitStatus, String> {
    let deadline = Instant::now() + NOTIFICATION_HOST_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => {
                thread::sleep(NOTIFICATION_HOST_POLL_INTERVAL);
            }
            Ok(None) => {
                terminate_notification_host(child);
                return Err("notification host timed out after 10 seconds".to_string());
            }
            Err(error) => {
                terminate_notification_host(child);
                return Err(format!("failed to wait for notification host: {error}"));
            }
        }
    }
}

#[cfg(windows)]
fn read_notification_host_output(mut reader: impl Read) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    reader
        .by_ref()
        .take((NOTIFICATION_HOST_MAX_OUTPUT_BYTES + 1) as u64)
        .read_to_end(&mut output)?;
    Ok(output)
}

#[cfg(windows)]
fn collect_notification_host_output(
    handle: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    stream_name: &str,
) -> Result<Vec<u8>, String> {
    handle
        .join()
        .map_err(|_| format!("notification host {stream_name} reader panicked"))?
        .map_err(|error| format!("failed to read notification host {stream_name}: {error}"))
}

#[cfg(windows)]
fn invoke_notification_host(app: &AppHandle, request: &Value) -> Result<(PathBuf, Value), String> {
    let request_bytes = serde_json::to_vec(request).map_err(string_error)?;
    let _request_guard = NOTIFICATION_HOST_LOCK
        .lock()
        .map_err(|_| "notification host request lock is poisoned".to_string())?;
    let host_path = notification_host_path(app)?;
    let mut command = Command::new(&host_path);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW);

    let mut child = command.spawn().map_err(string_error)?;
    let mut stdin = child.stdin.take().ok_or_else(|| {
        terminate_notification_host(&mut child);
        "notification host stdin was not available".to_string()
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        terminate_notification_host(&mut child);
        "notification host stdout was not available".to_string()
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        terminate_notification_host(&mut child);
        "notification host stderr was not available".to_string()
    })?;

    let stdout_thread = thread::spawn(move || read_notification_host_output(stdout));
    let stderr_thread = thread::spawn(move || read_notification_host_output(stderr));
    let write_result = stdin
        .write_all(&request_bytes)
        .and_then(|_| stdin.write_all(b"\n"));
    drop(stdin);
    if let Err(error) = write_result {
        terminate_notification_host(&mut child);
        let _ = collect_notification_host_output(stdout_thread, "stdout");
        let _ = collect_notification_host_output(stderr_thread, "stderr");
        return Err(format!(
            "failed to write notification host request: {error}"
        ));
    }

    let status_result = wait_for_notification_host(&mut child);
    let stdout = collect_notification_host_output(stdout_thread, "stdout")?;
    let stderr = collect_notification_host_output(stderr_thread, "stderr")?;
    let status = status_result?;

    if stdout.len() > NOTIFICATION_HOST_MAX_OUTPUT_BYTES
        || stderr.len() > NOTIFICATION_HOST_MAX_OUTPUT_BYTES
    {
        return Err("notification host output exceeded 64 KiB".to_string());
    }

    let stdout = String::from_utf8_lossy(&stdout);
    let response: Value = serde_json::from_str(stdout.trim()).map_err(|error| {
        format!(
            "notification host returned invalid JSON (exit code {:?}): {error}",
            status.code()
        )
    })?;

    if !status.success() || response.get("ok").and_then(Value::as_bool) != Some(true) {
        let code = response
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("request_failed");
        let message = response
            .get("message")
            .and_then(Value::as_str)
            .filter(|message| !message.is_empty())
            .unwrap_or("request failed");
        return Err(format!(
            "notification host request failed ({code}): {message}"
        ));
    }

    Ok((host_path, response))
}

#[cfg(not(windows))]
fn invoke_notification_host(
    _app: &AppHandle,
    _request: &Value,
) -> Result<(PathBuf, Value), String> {
    Err("scheduled notifications are only available on Windows".to_string())
}

async fn invoke_notification_host_async(
    app: AppHandle,
    request: Value,
) -> Result<(PathBuf, Value), String> {
    tauri::async_runtime::spawn_blocking(move || invoke_notification_host(&app, &request))
        .await
        .map_err(string_error)?
}

fn unix_ms_to_utc(unix_ms: i64) -> Result<String, String> {
    Utc.timestamp_millis_opt(unix_ms)
        .single()
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true))
        .ok_or_else(|| "reminder timestamp is outside the supported UTC range".to_string())
}

struct NotificationReminderHost {
    app: AppHandle,
}

impl reminder_orchestrator::ReminderHost for NotificationReminderHost {
    fn list(&self) -> Result<Vec<reminders::ScheduledReminder>, String> {
        let (_, response) = invoke_notification_host(&self.app, &json!({ "operation": "list" }))?;
        let items = response
            .get("items")
            .and_then(Value::as_array)
            .ok_or_else(|| "notification host list response did not contain items".to_string())?;
        items
            .iter()
            .map(|item| {
                let tag = item
                    .get("tag")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "notification host returned an item without a tag".to_string())?
                    .to_string();
                let due_at_utc = item
                    .get("dueAtUtc")
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("scheduled reminder {tag} has no dueAtUtc"))?;
                let due_at_unix_ms = DateTime::parse_from_rfc3339(due_at_utc)
                    .map_err(|error| {
                        format!("scheduled reminder {tag} has invalid dueAtUtc: {error}")
                    })?
                    .timestamp_millis();
                Ok(reminders::ScheduledReminder {
                    tag,
                    due_at_unix_ms,
                    activation_uri: item
                        .get("activationUri")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                })
            })
            .collect()
    }

    fn schedule(&self, spec: reminders::ReminderSpec) -> Result<(), String> {
        let due_at_utc = unix_ms_to_utc(spec.due_at_unix_ms)?;
        let activation_uri = spec
            .activation_uri
            .clone()
            .ok_or_else(|| "scheduled reminders require an activation URI".to_string())?;
        invoke_notification_host(
            &self.app,
            &json!({
                "operation": "schedule",
                "id": spec.tag,
                "title": spec.title,
                "body": spec.body,
                "dueAtUtc": due_at_utc,
                "activationUri": activation_uri,
            }),
        )
        .map(|_| ())
    }

    fn cancel(&self, tag: &str) -> Result<(), String> {
        invoke_notification_host(
            &self.app,
            &json!({
                "operation": "cancel",
                "id": tag,
            }),
        )
        .map(|_| ())
    }
}

fn schedule_reminder_job_retry(app: &AppHandle) {
    let state = app.state::<AppState>();
    let next_retry_at_unix_ms = match state.database.lock() {
        Ok(connection) => reminder_jobs::next_retry_at(&connection).ok().flatten(),
        Err(error) => {
            eprintln!("could not inspect reminder retry schedule: {error}");
            return;
        }
    };
    let Some(next_retry_at_unix_ms) = next_retry_at_unix_ms else {
        state
            .scheduled_reminder_retry_at_unix_ms
            .store(0, Ordering::Release);
        return;
    };

    let now_unix_ms = tasks::now_unix_ms();
    let target_unix_ms = next_retry_at_unix_ms.max(now_unix_ms);
    loop {
        let current_target = state
            .scheduled_reminder_retry_at_unix_ms
            .load(Ordering::Acquire);
        if current_target != 0 && current_target <= target_unix_ms {
            return;
        }
        if state
            .scheduled_reminder_retry_at_unix_ms
            .compare_exchange(
                current_target,
                target_unix_ms,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            break;
        }
    }

    let app_handle = app.clone();
    thread::spawn(move || {
        let delay_unix_ms = target_unix_ms.saturating_sub(tasks::now_unix_ms());
        if delay_unix_ms > 0 {
            thread::sleep(Duration::from_millis(delay_unix_ms as u64));
        }
        let state = app_handle.state::<AppState>();
        if state
            .scheduled_reminder_retry_at_unix_ms
            .compare_exchange(target_unix_ms, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let retry_app = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = drain_reminder_jobs_for_app(retry_app).await {
                eprintln!("reminder retry drain failed: {error}");
            }
        });
    });
}

async fn drain_reminder_jobs_for_app(
    app: AppHandle,
) -> Result<reminder_jobs::JobDrainReport, String> {
    let result = tauri::async_runtime::spawn_blocking({
        let app = app.clone();
        move || {
            let state = app.state::<AppState>();
            reminder_jobs::drain_ready_jobs(
                &state.database,
                &state.reminder_orchestration_lock,
                state.reminder_host.as_ref(),
                tasks::now_unix_ms(),
            )
        }
    })
    .await
    .map_err(string_error);
    schedule_reminder_job_retry(&app);
    result?
}

async fn reconcile_reminders_for_app(app: AppHandle) -> Result<ReconcileReport, String> {
    let preflight_warning = tauri::async_runtime::spawn_blocking({
        let app = app.clone();
        move || {
            let state = app.state::<AppState>();
            let now_unix_ms = tasks::now_unix_ms();
            let scheduled_tags = {
                let _orchestration_guard = state
                    .reminder_orchestration_lock
                    .lock()
                    .map_err(|_| "reminder orchestration lock is poisoned".to_string())?;
                state
                    .reminder_host
                    .list()
                    .map(|scheduled| {
                        scheduled
                            .into_iter()
                            .map(|item| item.tag)
                            .collect::<Vec<_>>()
                    })
                    .map_err(|error| format!("reminder inventory failed: {error}"))
            };
            let mut connection = state
                .database
                .lock()
                .map_err(|_| "database lock is poisoned".to_string())?;
            reminder_jobs::enqueue_sync_for_all(&mut connection, now_unix_ms)?;
            match scheduled_tags {
                Ok(tags) => {
                    reminder_jobs::enqueue_cancels_for_orphaned_tags(
                        &mut connection,
                        &tags,
                        now_unix_ms,
                    )?;
                    Ok::<Option<String>, String>(None)
                }
                Err(warning) => Ok(Some(warning)),
            }
        }
    })
    .await
    .map_err(string_error)??;
    let report = drain_reminder_jobs_for_app(app).await?;
    let warning = match (preflight_warning, report.warning) {
        (Some(preflight), Some(worker)) => Some(format!("{preflight}; {worker}")),
        (Some(preflight), None) => Some(preflight),
        (None, worker) => worker,
    };
    Ok(ReconcileReport {
        scheduled: report.scheduled,
        cancelled: report.cancelled,
        missed_count: report.missed_task_ids.len(),
        missed_task_ids: report.missed_task_ids,
        capability: reminders::DeliveryCapability::OsScheduled,
        warning,
    })
}

async fn sync_latest_task_reminder_for_app(
    app: AppHandle,
    task_id: i64,
) -> Result<Option<String>, String> {
    let report = drain_reminder_jobs_for_app(app.clone()).await?;
    if let Some(message) = report.warning.as_ref() {
        emit_reminder_warning(&app, task_id, message.clone());
    }
    Ok(report.warning)
}

fn unavailable_notification_diagnostics(error: String) -> NotificationDiagnostics {
    NotificationDiagnostics {
        immediate_notifications_available: false,
        scheduled_notifications_available: false,
        scheduled_notifications_status: "host-unavailable".to_string(),
        scheduled_notifications_detail: error,
        pending_count: None,
        notification_host_path: None,
        activation_delivery_available: false,
    }
}

#[tauri::command]
async fn get_notification_diagnostics(app: AppHandle) -> NotificationDiagnostics {
    match invoke_notification_host_async(app, json!({ "operation": "diagnostics" })).await {
        Ok((host_path, response)) => {
            let status = response
                .get("setting")
                .and_then(Value::as_str)
                .unwrap_or("Unknown")
                .to_string();
            let enabled = status.eq_ignore_ascii_case("Enabled");
            NotificationDiagnostics {
                immediate_notifications_available: enabled,
                scheduled_notifications_available: enabled,
                scheduled_notifications_status: status.clone(),
                scheduled_notifications_detail: if enabled {
                    "Notification host connected; immediate notifications, scheduling, and startodo protocol activation are available."
                        .to_string()
                } else {
                    format!("Notification host connected, but Windows reports notification setting {status}.")
                },
                pending_count: response.get("pendingCount").and_then(Value::as_u64),
                notification_host_path: Some(host_path.display().to_string()),
                activation_delivery_available: enabled,
            }
        }
        Err(error) => unavailable_notification_diagnostics(error),
    }
}

#[tauri::command]
async fn send_test_notification(app: AppHandle) -> Result<(), String> {
    invoke_notification_host_async(
        app,
        json!({
            "operation": "show",
            "id": STAGE0_NOTIFICATION_ID,
            "title": "StarToDo",
            "body": "Stage 0 notification test",
            "activationUri": STAGE0_ACTIVATION_URI,
        }),
    )
    .await
    .map(|_| ())
}

#[tauri::command]
async fn schedule_test_notification(app: AppHandle, due_at_utc: String) -> Result<Value, String> {
    invoke_notification_host_async(
        app,
        json!({
            "operation": "schedule",
            "id": STAGE0_NOTIFICATION_ID,
            "title": "StarToDo",
            "body": "Stage 0 scheduled notification test",
            "dueAtUtc": due_at_utc,
            "activationUri": STAGE0_ACTIVATION_URI,
        }),
    )
    .await
    .map(|(_, response)| response)
}

#[tauri::command]
async fn cancel_test_notification(app: AppHandle) -> Result<Value, String> {
    invoke_notification_host_async(
        app,
        json!({
            "operation": "cancel",
            "id": STAGE0_NOTIFICATION_ID,
        }),
    )
    .await
    .map(|(_, response)| response)
}

#[tauri::command]
fn hide_to_tray(app: AppHandle) -> Result<(), String> {
    hide_main_window(&app)
}

#[tauri::command]
fn release_ui(
    app: AppHandle,
    listener_token: u64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    destroy_main_window_and_release_listener(&app, &state.reminder_warning_state, listener_token)
}

#[tauri::command]
fn get_window_preferences(app: AppHandle) -> WindowPreferences {
    read_window_preferences(&app)
}

#[tauri::command]
fn save_window_preferences(app: AppHandle, preferences: WindowPreferences) -> Result<(), String> {
    validate_window_preferences(&preferences)?;
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        apply_window_preferences(&window, &preferences)?;
    }
    save_window_preferences_to_disk(&app, &preferences)
}

#[tauri::command]
fn set_window_mode(app: AppHandle, mode: String) -> Result<(), String> {
    match mode.as_str() {
        "full" | "compact" => set_window_mode_internal(&app, &mode),
        "normal" => app
            .get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| "main window is not available".to_string())?
            .unmaximize()
            .map_err(string_error),
        "minimized" => app
            .get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| "main window is not available".to_string())?
            .minimize()
            .map_err(string_error),
        "maximized" => app
            .get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| "main window is not available".to_string())?
            .maximize()
            .map_err(string_error),
        "fullscreen" => app
            .get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| "main window is not available".to_string())?
            .set_fullscreen(true)
            .map_err(string_error),
        _ => Err(
            "window mode must be full, compact, normal, minimized, maximized, or fullscreen"
                .to_string(),
        ),
    }
}

#[tauri::command]
fn set_always_on_top(app: AppHandle, always_on_top: bool) -> Result<(), String> {
    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "main window is not available".to_string())?;
    window
        .set_always_on_top(always_on_top)
        .map_err(string_error)?;
    let mut preferences = read_window_preferences(&app);
    preferences.always_on_top = always_on_top;
    save_window_preferences_to_disk(&app, &preferences)
}

fn install_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let compact = MenuItem::with_id(app, "compact", "Compact mode", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "Hide", true, None::<&str>)?;
    let release_ui = MenuItem::with_id(app, "release_ui", "Release UI", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &compact, &hide, &release_ui, &quit])?;

    TrayIconBuilder::with_id("main-tray")
        .icon(
            app.default_window_icon()
                .expect("default app icon is missing")
                .clone(),
        )
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_or_create_main_window(app),
            "compact" => {
                let _ = set_window_mode_internal(app, "compact");
                show_or_create_main_window(app);
            }
            "hide" => {
                let _ = hide_main_window(app);
            }
            "release_ui" => {
                let state = app.state::<AppState>();
                let _ = destroy_main_window_and_release_current(app, &state.reminder_warning_state);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_or_create_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must remain the first plugin: a second launch must target the existing process.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_or_create_main_window(app);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            #[cfg(all(debug_assertions, windows))]
            app.deep_link().register_all()?;

            let (database, database_path) =
                open_database(app.handle()).map_err(std::io::Error::other)?;
            let reminder_host: Arc<dyn reminder_orchestrator::ReminderHost> =
                Arc::new(NotificationReminderHost {
                    app: app.handle().clone(),
                });
            app.manage(AppState {
                database: Mutex::new(database),
                database_path,
                reminder_orchestration_lock: Mutex::new(()),
                reminder_host,
                reminder_warning_state: Mutex::new(ReminderWarningState::default()),
                next_reminder_warning_listener_token: AtomicU64::new(1),
                ui_rebuild_in_progress: AtomicBool::new(false),
                next_reminder_warning_id: AtomicU64::new(1),
                scheduled_reminder_retry_at_unix_ms: AtomicI64::new(0),
            });
            install_activation_listener(app);
            queue_current_activation(app);
            install_tray(app)?;

            if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                let preferences = read_window_preferences(app.handle());
                apply_window_preferences(&window, &preferences).map_err(std::io::Error::other)?;
                install_close_to_tray_behavior(&window);
                show_existing_window(&window).map_err(std::io::Error::other)?;
            }

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match reconcile_reminders_for_app(app_handle).await {
                    Ok(report) => {
                        if let Some(warning) = report.warning {
                            eprintln!("reminder reconciliation warning: {warning}");
                        }
                    }
                    Err(error) => eprintln!("reminder reconciliation skipped: {error}"),
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_runtime_snapshot,
            record_ui_ready,
            sample_process_metrics,
            run_database_probe,
            list_projects,
            create_project,
            update_project,
            archive_project,
            restore_project,
            list_tasks,
            list_deleted_tasks,
            create_task,
            update_task,
            set_task_planned_date,
            set_task_completed,
            snooze_task,
            defer_task_to_tomorrow,
            delete_task,
            restore_task,
            permanently_delete_task,
            reconcile_reminders,
            list_missed_reminders,
            list_reliability_incidents,
            acknowledge_reliability_incidents,
            claim_pending_activations,
            acknowledge_pending_activations,
            claim_pending_reminder_warnings,
            acknowledge_reminder_warnings,
            record_ui_not_ready,
            parse_task_drafts,
            get_window_preferences,
            save_window_preferences,
            get_notification_diagnostics,
            send_test_notification,
            schedule_test_notification,
            cancel_test_notification,
            hide_to_tray,
            release_ui,
            set_window_mode,
            set_always_on_top
        ])
        .run(tauri::generate_context!())
        .expect("error while running StarToDo");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent() {
        let mut connection = Connection::open_in_memory().expect("in-memory database should open");
        apply_migrations(&mut connection).expect("first migration pass should succeed");
        apply_migrations(&mut connection).expect("second migration pass should succeed");

        let versions: Vec<i64> = connection
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .expect("migration query should prepare")
            .query_map([], |row| row.get(0))
            .expect("migration rows should query")
            .collect::<rusqlite::Result<_>>()
            .expect("migration rows should decode");
        assert_eq!(
            versions,
            vec![
                STAGE0_MIGRATION_VERSION,
                TASKS_MIGRATION_VERSION,
                TASK_FIELDS_MIGRATION_VERSION,
                TASK_ORGANIZATION_MIGRATION_VERSION,
                TASK_RECURRENCE_MIGRATION_VERSION,
                PROJECTS_AND_RELIABILITY_MIGRATION_VERSION,
                REMINDER_DELIVERY_RETRY_MIGRATION_VERSION,
            ]
        );
    }

    #[test]
    fn claimed_reminder_warnings_are_exclusive_until_released_or_acknowledged() {
        let next_warning_id = AtomicU64::new(1);
        let next_listener_token = AtomicU64::new(1);
        let warning_state = Mutex::new(ReminderWarningState::default());
        let first = enqueue_pending_reminder_warning(
            &next_warning_id,
            &warning_state,
            7,
            "host cancel failed".to_string(),
        )
        .expect("warning should queue")
        .0;
        let second = enqueue_pending_reminder_warning(
            &next_warning_id,
            &warning_state,
            8,
            "host schedule failed".to_string(),
        )
        .expect("warning should queue")
        .0;
        let first_listener = register_reminder_warning_listener(
            &warning_state,
            &next_listener_token,
            "first-ui".to_string(),
            1,
            true,
        )
        .expect("first listener should register");

        let first_claim = claim_pending_reminder_warnings_from(&warning_state, first_listener, 100)
            .expect("first listener should claim warnings");
        assert_eq!(
            first_claim
                .iter()
                .map(|warning| warning.id)
                .collect::<Vec<_>>(),
            vec![first.id, second.id]
        );

        let second_listener = register_reminder_warning_listener(
            &warning_state,
            &next_listener_token,
            "second-ui".to_string(),
            2,
            true,
        )
        .expect("second listener should register");
        assert!(
            claim_pending_reminder_warnings_from(&warning_state, second_listener, 101)
                .expect("second listener should inspect claims")
                .is_empty()
        );

        acknowledge_reminder_warnings_from(&warning_state, first_listener, &[first.id], 101)
            .expect("previous listener should acknowledge only its claimed warning");
        assert!(
            claim_pending_reminder_warnings_from(&warning_state, second_listener, 102)
                .expect("second listener should not take the remaining first-listener claim")
                .is_empty()
        );

        release_reminder_warning_listener(&warning_state, first_listener)
            .expect("first listener cleanup should release its remaining claim");
        let second_claim =
            claim_pending_reminder_warnings_from(&warning_state, second_listener, 103)
                .expect("second listener should claim released warning");
        assert_eq!(
            second_claim
                .iter()
                .map(|warning| warning.id)
                .collect::<Vec<_>>(),
            vec![second.id]
        );

        acknowledge_reminder_warnings_from(&warning_state, second_listener, &[second.id], 104)
            .expect("current listener should acknowledge its warning");
        assert!(
            claim_pending_reminder_warnings_from(&warning_state, second_listener, 104)
                .expect("acknowledged warnings should not replay")
                .is_empty()
        );
    }

    #[test]
    fn expired_reminder_warning_claims_replay_to_the_current_listener() {
        let next_warning_id = AtomicU64::new(1);
        let next_listener_token = AtomicU64::new(1);
        let warning_state = Mutex::new(ReminderWarningState::default());
        let warning = enqueue_pending_reminder_warning(
            &next_warning_id,
            &warning_state,
            7,
            "host cancel failed".to_string(),
        )
        .expect("warning should queue")
        .0;
        let first_listener = register_reminder_warning_listener(
            &warning_state,
            &next_listener_token,
            "first-ui".to_string(),
            1,
            true,
        )
        .expect("first listener should register");
        claim_pending_reminder_warnings_from(&warning_state, first_listener, 100)
            .expect("first listener should claim warning");
        let second_listener = register_reminder_warning_listener(
            &warning_state,
            &next_listener_token,
            "second-ui".to_string(),
            2,
            true,
        )
        .expect("second listener should register");

        assert!(claim_pending_reminder_warnings_from(
            &warning_state,
            second_listener,
            100 + REMINDER_WARNING_CLAIM_LEASE_MS - 1,
        )
        .expect("unexpired claim should remain exclusive")
        .is_empty());
        let replayed = claim_pending_reminder_warnings_from(
            &warning_state,
            second_listener,
            100 + REMINDER_WARNING_CLAIM_LEASE_MS,
        )
        .expect("expired claim should replay");
        assert_eq!(
            replayed.iter().map(|queued| queued.id).collect::<Vec<_>>(),
            vec![warning.id]
        );
        acknowledge_reminder_warnings_from(
            &warning_state,
            first_listener,
            &[warning.id],
            100 + REMINDER_WARNING_CLAIM_LEASE_MS,
        )
        .expect("expired first listener acknowledgement should be ignored");
        assert_eq!(
            claim_pending_reminder_warnings_from(
                &warning_state,
                second_listener,
                100 + REMINDER_WARNING_CLAIM_LEASE_MS + 1,
            )
            .expect("current listener should retain replayed warning")
            .iter()
            .map(|queued| queued.id)
            .collect::<Vec<_>>(),
            vec![warning.id]
        );
    }

    #[test]
    fn stale_reminder_warning_registration_cannot_replace_the_current_listener() {
        let next_listener_token = AtomicU64::new(1);
        let warning_state = Mutex::new(ReminderWarningState::default());
        let current_listener = register_reminder_warning_listener(
            &warning_state,
            &next_listener_token,
            "current-ui".to_string(),
            2,
            true,
        )
        .expect("current listener should register");

        assert_eq!(
            register_reminder_warning_listener(
                &warning_state,
                &next_listener_token,
                "previous-ui".to_string(),
                1,
                true,
            ),
            Err("stale reminder warning listener registration".to_string())
        );
        assert_eq!(
            warning_state
                .lock()
                .expect("warning state should be readable")
                .listener
                .as_ref()
                .map(|listener| listener.token),
            Some(current_listener)
        );
    }

    #[test]
    fn reminder_warning_queue_does_not_silently_drop_warnings() {
        let next_warning_id = AtomicU64::new(1);
        let next_listener_token = AtomicU64::new(1);
        let warning_state = Mutex::new(ReminderWarningState::default());
        for task_id in 1..=33 {
            enqueue_pending_reminder_warning(
                &next_warning_id,
                &warning_state,
                task_id,
                format!("warning {task_id}"),
            )
            .expect("warning should queue");
        }
        let listener = register_reminder_warning_listener(
            &warning_state,
            &next_listener_token,
            "current-ui".to_string(),
            1,
            true,
        )
        .expect("listener should register");

        let warnings = claim_pending_reminder_warnings_from(&warning_state, listener, 100)
            .expect("listener should claim warnings");
        assert_eq!(warnings.len(), 33);
        assert_eq!(warnings.first().map(|warning| warning.id), Some(1));
        assert_eq!(warnings.last().map(|warning| warning.id), Some(33));
    }

    #[test]
    fn notification_activation_requires_a_capability_payload() {
        let activation = reminder_jobs::parse_notification_activation(
            &Url::parse("startodo://reminder/open?delivery=delivery-7&token=AAAAAAAAAAAAAAAA")
                .expect("capability URL should parse"),
        )
        .expect("canonical capability URL should be accepted");
        assert_eq!(activation.delivery_id, "delivery-7");
        assert_eq!(activation.token, "AAAAAAAAAAAAAAAA");

        for raw in [
            "startodo://reminder/open?id=task-7",
            "startodo://reminder/open?delivery=delivery-7&token=AAAAAAAAAAAAAAAA&extra=1",
            "startodo://reminder/open?delivery=delivery-7&token=short",
            "startodo://reminder/open?delivery=delivery-7&token=AAAAAAAAAAAAAAAA#fragment",
            "startodo://other/open?delivery=delivery-7&token=AAAAAAAAAAAAAAAA",
        ] {
            assert!(
                reminder_jobs::parse_notification_activation(&Url::parse(raw).unwrap()).is_none(),
                "{raw}"
            );
        }
    }

    #[test]
    fn v1_database_upgrades_to_v2_without_losing_probe_data() {
        let mut connection = Connection::open_in_memory().expect("in-memory database should open");
        connection
            .execute_batch(
                "
                CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at_unix_ms INTEGER NOT NULL
                );
                CREATE TABLE stage0_probe (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    run_count INTEGER NOT NULL DEFAULT 0,
                    last_probe_at_unix_ms INTEGER NOT NULL
                );
                INSERT INTO schema_migrations (version, applied_at_unix_ms) VALUES (1, 1);
                INSERT INTO stage0_probe (id, run_count, last_probe_at_unix_ms) VALUES (1, 7, 99);
                ",
            )
            .expect("v1 fixture should be created");

        apply_migrations(&mut connection).expect("v1 database should upgrade");
        let probe: (i64, i64) = connection
            .query_row(
                "SELECT run_count, last_probe_at_unix_ms FROM stage0_probe WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("probe data should remain");
        let version_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("migration count should query");
        let tasks_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'tasks')",
                [],
                |row| row.get(0),
            )
            .expect("tasks table should exist");
        assert_eq!(probe, (7, 99));
        assert_eq!(version_count, 7);
        let recurrence_column_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('tasks') WHERE name = 'recurrence_kind')",
                [],
                |row| row.get(0),
            )
            .expect("recurrence column should exist");
        assert!(tasks_exists);
        assert!(recurrence_column_exists);
    }

    #[test]
    fn v2_database_rebuilds_tasks_with_stage2_constraints_and_preserves_rows() {
        let mut connection = Connection::open_in_memory().expect("in-memory database should open");
        connection
            .execute_batch(
                "
                CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at_unix_ms INTEGER NOT NULL
                );
                CREATE TABLE tasks (
                    id INTEGER PRIMARY KEY,
                    title TEXT NOT NULL,
                    notes TEXT NOT NULL DEFAULT '',
                    due_at_unix_ms INTEGER,
                    completed_at_unix_ms INTEGER,
                    created_at_unix_ms INTEGER NOT NULL,
                    updated_at_unix_ms INTEGER NOT NULL,
                    CHECK (length(trim(title)) BETWEEN 1 AND 200),
                    CHECK (length(notes) <= 10000),
                    CHECK (due_at_unix_ms IS NULL OR due_at_unix_ms BETWEEN 0 AND 8640000000000000),
                    CHECK (completed_at_unix_ms IS NULL OR completed_at_unix_ms >= 0)
                );
                INSERT INTO schema_migrations (version, applied_at_unix_ms) VALUES (1, 1), (2, 2);
                INSERT INTO tasks (id, title, notes, due_at_unix_ms, completed_at_unix_ms, created_at_unix_ms, updated_at_unix_ms)
                VALUES (7, 'legacy', 'preserve me', 42, NULL, 100, 101);
                ",
            )
            .expect("v2 fixture should be created");

        apply_migrations(&mut connection).expect("v2 database should upgrade");

        let row: (String, String, Option<String>, Option<i64>, Option<i64>, i64) = connection
            .query_row(
                "SELECT title, notes, planned_date, reminder_at_unix_ms, reminder_fired_at_unix_ms, priority FROM tasks WHERE id = 7",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )
            .expect("legacy task should remain after rebuild");
        assert_eq!(
            row,
            (
                "legacy".to_string(),
                "preserve me".to_string(),
                None,
                None,
                None,
                0
            )
        );

        let invalid_priority = connection.execute(
            "INSERT INTO tasks (title, notes, priority, created_at_unix_ms, updated_at_unix_ms) VALUES ('bad', '', 4, 1, 1)",
            [],
        );
        assert!(
            invalid_priority.is_err(),
            "priority constraint should be enforced after upgrade"
        );

        let index_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = 'idx_tasks_list')",
                [],
                |row| row.get(0),
            )
            .expect("stage2 index should exist");
        assert!(index_exists);

        let deleted_column_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('tasks') WHERE name = 'deleted_at_unix_ms')",
                [],
                |row| row.get(0),
            )
            .expect("trash column should exist after upgrade");
        assert!(deleted_column_exists);
    }

    #[test]
    fn v3_database_adds_trash_column_without_rebuilding_existing_rows() {
        let mut connection = Connection::open_in_memory().expect("in-memory database should open");
        connection
            .execute_batch(
                "
                CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at_unix_ms INTEGER NOT NULL
                );
                CREATE TABLE tasks (
                    id INTEGER PRIMARY KEY,
                    title TEXT NOT NULL,
                    notes TEXT NOT NULL DEFAULT '',
                    planned_date TEXT,
                    due_at_unix_ms INTEGER,
                    reminder_at_unix_ms INTEGER,
                    reminder_fired_at_unix_ms INTEGER,
                    priority INTEGER NOT NULL DEFAULT 0,
                    completed_at_unix_ms INTEGER,
                    created_at_unix_ms INTEGER NOT NULL,
                    updated_at_unix_ms INTEGER NOT NULL,
                    CHECK (length(trim(title)) BETWEEN 1 AND 200),
                    CHECK (length(notes) <= 10000),
                    CHECK (planned_date IS NULL OR (
                        length(planned_date) = 10 AND
                        planned_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
                    )),
                    CHECK (due_at_unix_ms IS NULL OR due_at_unix_ms BETWEEN 0 AND 8640000000000000),
                    CHECK (reminder_at_unix_ms IS NULL OR reminder_at_unix_ms BETWEEN 0 AND 8640000000000000),
                    CHECK (reminder_fired_at_unix_ms IS NULL OR reminder_fired_at_unix_ms BETWEEN 0 AND 8640000000000000),
                    CHECK (priority BETWEEN 0 AND 3),
                    CHECK (completed_at_unix_ms IS NULL OR completed_at_unix_ms >= 0)
                );
                CREATE INDEX idx_tasks_list ON tasks (
                    completed_at_unix_ms,
                    planned_date,
                    due_at_unix_ms,
                    priority,
                    created_at_unix_ms,
                    id
                );
                INSERT INTO schema_migrations (version, applied_at_unix_ms) VALUES (1, 1), (2, 2), (3, 3);
                INSERT INTO tasks (
                    id, title, notes, planned_date, due_at_unix_ms,
                    reminder_at_unix_ms, reminder_fired_at_unix_ms, priority,
                    completed_at_unix_ms, created_at_unix_ms, updated_at_unix_ms
                ) VALUES (11, 'v3 row', 'keep this row', '2026-08-09', 42, 84, NULL, 2, NULL, 100, 101);
                ",
            )
            .expect("v3 fixture should be created");

        apply_migrations(&mut connection).expect("v3 database should upgrade");

        let row: (String, String, Option<String>, Option<i64>, Option<i64>, i64) = connection
            .query_row(
                "SELECT title, notes, planned_date, due_at_unix_ms, deleted_at_unix_ms, priority FROM tasks WHERE id = 11",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )
            .expect("v3 row should remain after v4 migration");
        assert_eq!(
            row,
            (
                "v3 row".to_string(),
                "keep this row".to_string(),
                Some("2026-08-09".to_string()),
                Some(42),
                None,
                2
            )
        );

        for index_name in [
            "idx_tasks_list",
            "idx_tasks_deleted",
            "idx_tasks_recurrence_source",
            "idx_tasks_recurrence_occurrence",
        ] {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1)",
                    [index_name],
                    |row| row.get(0),
                )
                .expect("migration index should query");
            assert!(exists, "expected index {index_name}");
        }

        let versions: Vec<i64> = connection
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .expect("migration versions should prepare")
            .query_map([], |row| row.get(0))
            .expect("migration versions should query")
            .collect::<rusqlite::Result<_>>()
            .expect("migration versions should decode");
        assert_eq!(versions, vec![1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn v4_database_upgrades_to_v5_preserving_rows_and_recurrence_constraints() {
        let mut connection = Connection::open_in_memory().expect("in-memory database should open");
        connection
            .execute_batch(
                "
                CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at_unix_ms INTEGER NOT NULL
                );
                CREATE TABLE stage0_probe (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    run_count INTEGER NOT NULL DEFAULT 0,
                    last_probe_at_unix_ms INTEGER NOT NULL
                );
                CREATE TABLE tasks (
                    id INTEGER PRIMARY KEY,
                    title TEXT NOT NULL,
                    notes TEXT NOT NULL DEFAULT '',
                    planned_date TEXT,
                    due_at_unix_ms INTEGER,
                    reminder_at_unix_ms INTEGER,
                    reminder_fired_at_unix_ms INTEGER,
                    priority INTEGER NOT NULL DEFAULT 0,
                    completed_at_unix_ms INTEGER,
                    created_at_unix_ms INTEGER NOT NULL,
                    updated_at_unix_ms INTEGER NOT NULL,
                    deleted_at_unix_ms INTEGER
                );
                INSERT INTO schema_migrations (version, applied_at_unix_ms)
                VALUES (1, 1), (2, 2), (3, 3), (4, 4);
                INSERT INTO stage0_probe (id, run_count, last_probe_at_unix_ms)
                VALUES (1, 1, 1);
                INSERT INTO tasks (
                    id, title, notes, planned_date, due_at_unix_ms, reminder_at_unix_ms,
                    reminder_fired_at_unix_ms, priority, completed_at_unix_ms,
                    created_at_unix_ms, updated_at_unix_ms, deleted_at_unix_ms
                ) VALUES (11, 'v4 row', 'preserve this', '2026-08-09', 42, 84, NULL, 2, NULL, 100, 101, NULL);
                ",
            )
            .expect("v4 fixture should be created");

        apply_migrations(&mut connection).expect("v4 database should upgrade");

        let row: (
            String,
            String,
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<i64>,
        ) = connection
            .query_row(
                "
                SELECT title, notes, planned_date, reminder_at_unix_ms,
                       recurrence_kind, recurrence_series_id
                FROM tasks WHERE id = 11
                ",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .expect("v4 row should remain after v5 migration");
        assert_eq!(
            row,
            (
                "v4 row".to_string(),
                "preserve this".to_string(),
                Some("2026-08-09".to_string()),
                Some(84),
                None,
                None,
            )
        );

        let template_table_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'recurrence_templates')",
                [],
                |row| row.get(0),
            )
            .expect("template table should exist");
        assert!(template_table_exists);

        connection
            .execute(
                "
                INSERT INTO tasks (
                    id, title, notes, priority, created_at_unix_ms, updated_at_unix_ms,
                    recurrence_source_task_id
                ) VALUES (21, 'source one', '', 0, 1, 1, 91)
                ",
                [],
            )
            .expect("first recurrence source should insert");
        assert!(
            connection
                .execute(
                    "
                    INSERT INTO tasks (
                        id, title, notes, priority, created_at_unix_ms, updated_at_unix_ms,
                        recurrence_source_task_id
                    ) VALUES (22, 'source duplicate', '', 0, 1, 1, 91)
                    ",
                    [],
                )
                .is_err(),
            "source index should reject a duplicate successor source"
        );

        connection
            .execute(
                "
                INSERT INTO tasks (
                    id, title, notes, planned_date, priority, created_at_unix_ms, updated_at_unix_ms,
                    recurrence_series_id
                ) VALUES (23, 'occurrence one', '', '2026-08-11', 0, 1, 1, 77)
                ",
                [],
            )
            .expect("first recurrence occurrence should insert");
        assert!(
            connection
                .execute(
                    "
                    INSERT INTO tasks (
                        id, title, notes, planned_date, priority, created_at_unix_ms, updated_at_unix_ms,
                        recurrence_series_id
                    ) VALUES (24, 'occurrence duplicate', '', '2026-08-11', 0, 1, 1, 77)
                    ",
                    [],
                )
                .is_err(),
            "occurrence index should reject a duplicate series date"
        );
    }

    #[test]
    fn v6_database_upgrades_to_v7_preserving_delivery_hashes_without_plaintext_capability_columns()
    {
        let mut connection = Connection::open_in_memory().expect("in-memory database should open");
        connection
            .execute_batch(
                "
                CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at_unix_ms INTEGER NOT NULL
                );
                INSERT INTO schema_migrations (version, applied_at_unix_ms)
                VALUES (1, 1), (2, 2), (3, 3), (4, 4), (5, 5), (6, 6);

                CREATE TABLE stage0_probe (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    run_count INTEGER NOT NULL DEFAULT 0,
                    last_probe_at_unix_ms INTEGER NOT NULL
                );
                CREATE TABLE projects (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    archived_at_unix_ms INTEGER,
                    created_at_unix_ms INTEGER NOT NULL,
                    updated_at_unix_ms INTEGER NOT NULL
                );
                CREATE UNIQUE INDEX idx_projects_active_name
                    ON projects (name COLLATE NOCASE)
                    WHERE archived_at_unix_ms IS NULL;
                CREATE TABLE tasks (
                    id INTEGER PRIMARY KEY,
                    title TEXT NOT NULL,
                    notes TEXT NOT NULL DEFAULT '',
                    planned_date TEXT,
                    due_at_unix_ms INTEGER,
                    reminder_at_unix_ms INTEGER,
                    reminder_fired_at_unix_ms INTEGER,
                    deleted_at_unix_ms INTEGER,
                    project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL,
                    priority INTEGER NOT NULL DEFAULT 0,
                    recurrence_kind TEXT,
                    recurrence_series_id INTEGER,
                    recurrence_source_task_id INTEGER,
                    recurrence_timezone TEXT,
                    recurrence_dst_policy TEXT,
                    completed_at_unix_ms INTEGER,
                    created_at_unix_ms INTEGER NOT NULL,
                    updated_at_unix_ms INTEGER NOT NULL
                );
                CREATE INDEX idx_tasks_list ON tasks (
                    completed_at_unix_ms,
                    planned_date,
                    due_at_unix_ms,
                    priority,
                    created_at_unix_ms,
                    id
                );
                CREATE INDEX idx_tasks_deleted ON tasks (deleted_at_unix_ms, id);
                CREATE INDEX idx_tasks_project_visibility ON tasks (
                    project_id,
                    deleted_at_unix_ms,
                    completed_at_unix_ms,
                    planned_date,
                    due_at_unix_ms,
                    priority,
                    created_at_unix_ms,
                    id
                );
                CREATE UNIQUE INDEX idx_tasks_recurrence_source
                    ON tasks (recurrence_source_task_id)
                    WHERE recurrence_source_task_id IS NOT NULL;
                CREATE UNIQUE INDEX idx_tasks_recurrence_occurrence
                    ON tasks (recurrence_series_id, planned_date)
                    WHERE recurrence_series_id IS NOT NULL
                      AND planned_date IS NOT NULL;
                CREATE TABLE recurrence_templates (
                    series_id INTEGER PRIMARY KEY,
                    reminder_at_unix_ms INTEGER
                );
                CREATE TABLE reminder_sync_jobs (
                    task_id INTEGER PRIMARY KEY,
                    intent TEXT NOT NULL CHECK (intent IN ('sync', 'cancel')),
                    generation INTEGER NOT NULL,
                    attempt_count INTEGER NOT NULL DEFAULT 0,
                    next_attempt_at_unix_ms INTEGER NOT NULL,
                    last_error TEXT,
                    updated_at_unix_ms INTEGER NOT NULL
                );
                CREATE INDEX idx_reminder_sync_jobs_ready
                    ON reminder_sync_jobs (next_attempt_at_unix_ms, updated_at_unix_ms);
                CREATE TABLE reminder_deliveries (
                    id TEXT PRIMARY KEY,
                    task_id INTEGER NOT NULL,
                    reminder_at_unix_ms INTEGER NOT NULL,
                    activation_token_hash BLOB NOT NULL,
                    state TEXT NOT NULL,
                    created_at_unix_ms INTEGER NOT NULL,
                    scheduled_at_unix_ms INTEGER,
                    activated_at_unix_ms INTEGER
                );
                CREATE UNIQUE INDEX idx_reminder_deliveries_live_task
                    ON reminder_deliveries (task_id)
                    WHERE state IN ('pending_schedule', 'scheduled', 'pending_cancel');
                CREATE TABLE activation_inbox (
                    id INTEGER PRIMARY KEY,
                    delivery_id TEXT NOT NULL UNIQUE,
                    task_id INTEGER NOT NULL,
                    received_at_unix_ms INTEGER NOT NULL,
                    claimed_by TEXT,
                    claim_expires_at_unix_ms INTEGER,
                    acknowledged_at_unix_ms INTEGER
                );
                CREATE INDEX idx_activation_inbox_claimable
                    ON activation_inbox (
                        acknowledged_at_unix_ms,
                        claim_expires_at_unix_ms,
                        received_at_unix_ms
                    );
                CREATE TABLE reliability_incidents (
                    id INTEGER PRIMARY KEY,
                    kind TEXT NOT NULL CHECK (kind IN ('reminder_host', 'database')),
                    dedupe_key TEXT NOT NULL UNIQUE,
                    task_id INTEGER,
                    operation TEXT NOT NULL,
                    message TEXT NOT NULL,
                    status TEXT NOT NULL CHECK (status IN ('open', 'resolved', 'acknowledged')),
                    first_seen_at_unix_ms INTEGER NOT NULL,
                    last_seen_at_unix_ms INTEGER NOT NULL,
                    occurrence_count INTEGER NOT NULL DEFAULT 1,
                    resolved_at_unix_ms INTEGER,
                    acknowledged_at_unix_ms INTEGER
                );
                INSERT INTO projects (id, name, created_at_unix_ms, updated_at_unix_ms)
                VALUES (1, 'Legacy project', 1, 1);
                INSERT INTO tasks (
                    id, title, notes, reminder_at_unix_ms, project_id, priority,
                    created_at_unix_ms, updated_at_unix_ms
                ) VALUES (7, 'Legacy reminder', '', 1234, 1, 0, 1, 1);
                INSERT INTO reminder_deliveries (
                    id, task_id, reminder_at_unix_ms, activation_token_hash,
                    state, created_at_unix_ms, scheduled_at_unix_ms, activated_at_unix_ms
                ) VALUES (
                    'delivery-v6', 7, 1234, X'0102030405060708090A0B0C0D0E0F10',
                    'scheduled', 1000, 1100, NULL
                );
                ",
            )
            .expect("v6 fixture should be created");

        apply_migrations(&mut connection).expect("v6 database should upgrade to v7");

        let row: (Vec<u8>, String, i64, Option<Vec<u8>>, String) = connection
            .query_row(
                "
                SELECT activation_token_hash, state, generation,
                       activation_uri_hash, typeof(activation_token_hash)
                FROM reminder_deliveries WHERE id = 'delivery-v6'
                ",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .expect("legacy delivery should remain after v7 migration");
        assert_eq!(
            row,
            (
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                "scheduled".to_string(),
                0,
                None,
                "blob".to_string(),
            )
        );

        let columns: Vec<String> = connection
            .prepare("SELECT name FROM pragma_table_info('reminder_deliveries') ORDER BY cid")
            .expect("delivery columns should query")
            .query_map([], |row| row.get(0))
            .expect("delivery columns should decode")
            .collect::<rusqlite::Result<_>>()
            .expect("delivery columns should collect");
        assert!(columns
            .iter()
            .any(|column| column == "activation_token_hash"));
        assert!(columns.iter().any(|column| column == "generation"));
        assert!(columns.iter().any(|column| column == "activation_uri_hash"));
        assert!(!columns.iter().any(|column| column == "activation_token"));
        assert!(!columns.iter().any(|column| column == "force_reschedule"));

        let versions: Vec<i64> = connection
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .expect("migration versions should prepare")
            .query_map([], |row| row.get(0))
            .expect("migration versions should query")
            .collect::<rusqlite::Result<_>>()
            .expect("migration versions should collect");
        assert_eq!(versions, vec![1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn database_probe_increments_without_duplicate_migrations() {
        let mut connection = Connection::open_in_memory().expect("in-memory database should open");
        let first = execute_database_probe(&mut connection, Path::new(":memory:"))
            .expect("first probe should succeed");
        let second = execute_database_probe(&mut connection, Path::new(":memory:"))
            .expect("second probe should succeed");

        assert_eq!(first.migration_count, 7);
        assert_eq!(second.migration_count, 7);
        assert_eq!(first.run_count, 1);
        assert_eq!(second.run_count, 2);
        assert!(second.last_probe_at_unix_ms >= first.last_probe_at_unix_ms);
    }
}
