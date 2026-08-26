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
    AppHandle, Emitter, Listener, LogicalPosition, LogicalSize, Manager, State, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_deep_link::DeepLinkExt;
use url::Url;

mod parser;
mod pomodoro;
mod projects;
mod reminder_jobs;
mod reminder_orchestrator;
mod reminders;
mod tasks;

const MAIN_WINDOW_LABEL: &str = "main";
const FLOATING_WINDOW_LABEL: &str = "floating";
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
const POMODORO_MIGRATION_VERSION: i64 = pomodoro::POMODORO_MIGRATION_VERSION;
const POMODORO_ACTIVATION_INBOX_MIGRATION_VERSION: i64 =
    pomodoro::POMODORO_ACTIVATION_INBOX_MIGRATION_VERSION;
const TASK_NOTIFICATION_CHANNEL: &str = "tasks";
const POMODORO_NOTIFICATION_CHANNEL: &str = pomodoro::POMODORO_NOTIFICATION_CHANNEL;
const WINDOW_PREFERENCES_FILE_NAME: &str = "window-preferences.json";
const FLOATING_WINDOW_PREFERENCES_FILE_NAME: &str = "floating-window-preferences.json";
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
    pomodoro_wake_generation: AtomicU64,
    pomodoro_operation_lock: Mutex<()>,
    immersive_restore_state: Mutex<Option<ImmersiveRestoreState>>,
    pending_floating_intent: Mutex<Option<FloatingIntent>>,
    floating_window_preferences_lock: Mutex<()>,
    floating_window_runtime: Mutex<FloatingWindowRuntime>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum WindowLayout {
    Adaptive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowBounds {
    x: Option<i32>,
    y: Option<i32>,
    width: u32,
    height: u32,
}

impl WindowBounds {
    fn from_physical(
        position: Option<tauri::PhysicalPosition<i32>>,
        size: tauri::PhysicalSize<u32>,
        scale_factor: f64,
    ) -> Self {
        let scale = if scale_factor > 0.0 {
            scale_factor
        } else {
            1.0
        };
        Self {
            x: position.map(|value| (value.x as f64 / scale).round() as i32),
            y: position.map(|value| (value.y as f64 / scale).round() as i32),
            width: (size.width as f64 / scale).round() as u32,
            height: (size.height as f64 / scale).round() as u32,
        }
    }
}

impl Default for WindowBounds {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            width: 960,
            height: 680,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowPreferences {
    layout: WindowLayout,
    maximized: bool,
    normal_bounds: WindowBounds,
    always_on_top: bool,
    last_immersive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImmersiveRestoreState {
    maximized: bool,
    normal_bounds: WindowBounds,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WindowRestoreTarget {
    Maximized,
    Normal(WindowBounds),
}

fn select_immersive_restore_bounds(
    maximized: bool,
    fullscreen: bool,
    persisted: &WindowBounds,
    current: Option<WindowBounds>,
) -> WindowBounds {
    if !maximized && !fullscreen {
        current.unwrap_or_else(|| persisted.clone())
    } else {
        persisted.clone()
    }
}

impl ImmersiveRestoreState {
    fn restore_target(&self) -> WindowRestoreTarget {
        if self.maximized {
            WindowRestoreTarget::Maximized
        } else {
            WindowRestoreTarget::Normal(self.normal_bounds.clone())
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WindowState {
    maximized: bool,
    fullscreen: bool,
    normal_bounds: WindowBounds,
}

impl Default for WindowPreferences {
    fn default() -> Self {
        Self {
            layout: WindowLayout::Adaptive,
            maximized: true,
            normal_bounds: WindowBounds::default(),
            always_on_top: false,
            last_immersive: false,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyWindowPreferences {
    mode: String,
    width: u32,
    height: u32,
    always_on_top: bool,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StoredWindowPreferences {
    Current(WindowPreferences),
    Legacy(LegacyWindowPreferences),
}

struct DecodedWindowPreferences {
    preferences: WindowPreferences,
    migrated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FloatingIntent {
    view: String,
    task_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum FloatingDisplayMode {
    Capsule,
    Expanded,
}

fn default_floating_display_mode() -> FloatingDisplayMode {
    FloatingDisplayMode::Expanded
}

fn floating_display_size(mode: FloatingDisplayMode) -> (f64, f64) {
    match mode {
        FloatingDisplayMode::Capsule => (340.0, 64.0),
        FloatingDisplayMode::Expanded => (360.0, 260.0),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FloatingWindowPreferences {
    visible: bool,
    x: Option<i32>,
    y: Option<i32>,
    width: f64,
    height: f64,
    always_on_top: bool,
    #[serde(default)]
    user_resized: bool,
    #[serde(default = "default_floating_display_mode")]
    display_mode: FloatingDisplayMode,
}

impl Default for FloatingWindowPreferences {
    fn default() -> Self {
        Self {
            visible: false,
            x: None,
            y: None,
            width: 360.0,
            height: 260.0,
            always_on_top: true,
            user_resized: false,
            display_mode: FloatingDisplayMode::Expanded,
        }
    }
}

#[derive(Debug, Default)]
struct FloatingWindowRuntime {
    programmatic_target: Option<(u32, u32)>,
    programmatic_until_unix_ms: u128,
    programmatic_generation: u64,
}

fn install_programmatic_resize_target(
    runtime: &mut FloatingWindowRuntime,
    target: (u32, u32),
    deadline: u128,
) -> u64 {
    runtime.programmatic_generation = runtime.programmatic_generation.wrapping_add(1);
    runtime.programmatic_target = Some(target);
    runtime.programmatic_until_unix_ms = deadline;
    runtime.programmatic_generation
}

fn clear_failed_programmatic_resize(
    runtime: &mut FloatingWindowRuntime,
    target: (u32, u32),
    deadline: u128,
    generation: u64,
) {
    if runtime.programmatic_target == Some(target)
        && runtime.programmatic_until_unix_ms == deadline
        && runtime.programmatic_generation == generation
    {
        runtime.programmatic_target = None;
        runtime.programmatic_until_unix_ms = 0;
    }
}

fn classify_floating_resize(
    runtime: &mut FloatingWindowRuntime,
    actual: (u32, u32),
    now_unix_ms: u128,
) -> bool {
    let Some(target) = runtime.programmatic_target else {
        return true;
    };
    if now_unix_ms <= runtime.programmatic_until_unix_ms {
        if actual == target {
            runtime.programmatic_target = None;
            runtime.programmatic_until_unix_ms = 0;
        }
        false
    } else {
        runtime.programmatic_target = None;
        runtime.programmatic_until_unix_ms = 0;
        true
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

fn emit_tasks_changed(app: &AppHandle) {
    let _ = app.emit("tasks-changed", ());
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

    if !migration_applied(POMODORO_MIGRATION_VERSION)? {
        transaction.execute_batch(pomodoro::MIGRATION_SQL)?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_unix_ms) VALUES (?1, ?2)",
            (POMODORO_MIGRATION_VERSION, now_unix_ms() as i64),
        )?;
    }

    if !migration_applied(POMODORO_ACTIVATION_INBOX_MIGRATION_VERSION)? {
        transaction.execute_batch(pomodoro::ACTIVATION_INBOX_MIGRATION_SQL)?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_unix_ms) VALUES (?1, ?2)",
            (
                POMODORO_ACTIVATION_INBOX_MIGRATION_VERSION,
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
    if let Some(activation) = pomodoro::parse_notification_activation(url) {
        let state = app.state::<AppState>();
        let _operation = match state.pomodoro_operation_lock.lock() {
            Ok(guard) => guard,
            Err(error) => {
                eprintln!("pomodoro notification activation operation lock failed: {error}");
                return;
            }
        };
        let now = tasks::now_unix_ms();
        let session_id = match state.database.lock() {
            Ok(mut connection) => match pomodoro::settle_at(&mut connection, now).and_then(|_| {
                pomodoro::consume_notification_activation_at(&mut connection, &activation, now)
            }) {
                Ok(session_id) => session_id,
                Err(error) => {
                    eprintln!("pomodoro notification activation consumption failed: {error}");
                    return;
                }
            },
            Err(error) => {
                eprintln!("pomodoro notification activation database lock failed: {error}");
                return;
            }
        };
        let Some(session_id) = session_id else {
            return;
        };
        match reconcile_pomodoro_locked(app, true) {
            Ok((_, Some(warning))) => {
                eprintln!("pomodoro activation reconciliation warning: {warning}")
            }
            Ok((_, None)) => {}
            Err(error) => eprintln!("pomodoro activation reconciliation failed: {error}"),
        }
        show_or_create_main_window(app);
        let _ = app.emit("pomodoro-activation", session_id);
        return;
    }

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
    if !(520..=7_680).contains(&preferences.normal_bounds.width) {
        return Err("window width must be between 520 and 7680".to_string());
    }
    if !(420..=4_320).contains(&preferences.normal_bounds.height) {
        return Err("window height must be between 420 and 4320".to_string());
    }
    for coordinate in [preferences.normal_bounds.x, preferences.normal_bounds.y]
        .into_iter()
        .flatten()
    {
        if !(-32_768..=32_767).contains(&coordinate) {
            return Err("window coordinates must be between -32768 and 32767".to_string());
        }
    }
    Ok(())
}

fn decode_window_preferences(content: &str) -> Result<DecodedWindowPreferences, String> {
    match serde_json::from_str::<StoredWindowPreferences>(content).map_err(string_error)? {
        StoredWindowPreferences::Current(preferences) => {
            validate_window_preferences(&preferences)?;
            Ok(DecodedWindowPreferences {
                preferences,
                migrated: false,
            })
        }
        StoredWindowPreferences::Legacy(legacy) => {
            let maximized = match legacy.mode.as_str() {
                "full" => true,
                "compact" => false,
                _ => return Err("unknown legacy window mode".to_string()),
            };
            let preferences = WindowPreferences {
                maximized,
                normal_bounds: WindowBounds {
                    width: legacy.width.clamp(520, 7_680),
                    height: legacy.height.clamp(420, 4_320),
                    ..WindowBounds::default()
                },
                always_on_top: legacy.always_on_top,
                ..WindowPreferences::default()
            };
            Ok(DecodedWindowPreferences {
                preferences,
                migrated: true,
            })
        }
    }
}

fn read_window_preferences(app: &AppHandle) -> WindowPreferences {
    let Ok(path) = window_preferences_path(app) else {
        return WindowPreferences::default();
    };
    let Ok(content) = fs::read_to_string(path) else {
        return WindowPreferences::default();
    };
    let Ok(decoded) = decode_window_preferences(&content) else {
        return WindowPreferences::default();
    };
    if decoded.migrated {
        if let Err(error) = save_window_preferences_to_disk(app, &decoded.preferences) {
            eprintln!("window preferences migration write failed: {error}");
        }
    }
    decoded.preferences
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainWindowPlacementStep {
    Unmaximize,
    SetNormalSize,
    SetNormalPosition,
    Maximize,
    LeaveNormal,
}

fn main_window_placement_plan(maximized: bool) -> [MainWindowPlacementStep; 4] {
    [
        MainWindowPlacementStep::Unmaximize,
        MainWindowPlacementStep::SetNormalSize,
        MainWindowPlacementStep::SetNormalPosition,
        if maximized {
            MainWindowPlacementStep::Maximize
        } else {
            MainWindowPlacementStep::LeaveNormal
        },
    ]
}

fn apply_window_preferences(
    window: &WebviewWindow,
    preferences: &WindowPreferences,
) -> Result<(), String> {
    validate_window_preferences(preferences)?;
    window
        .set_min_size(Some(LogicalSize::new(520.0, 420.0)))
        .map_err(string_error)?;
    window.set_resizable(true).map_err(string_error)?;
    window
        .set_always_on_top(preferences.always_on_top)
        .map_err(string_error)?;
    window.set_fullscreen(false).map_err(string_error)?;
    for step in main_window_placement_plan(preferences.maximized) {
        match step {
            MainWindowPlacementStep::Unmaximize => {
                window.unmaximize().map_err(string_error)?;
            }
            MainWindowPlacementStep::SetNormalSize => {
                window
                    .set_size(LogicalSize::new(
                        preferences.normal_bounds.width,
                        preferences.normal_bounds.height,
                    ))
                    .map_err(string_error)?;
            }
            MainWindowPlacementStep::SetNormalPosition => {
                if let (Some(x), Some(y)) =
                    (preferences.normal_bounds.x, preferences.normal_bounds.y)
                {
                    window
                        .set_position(LogicalPosition::new(x as f64, y as f64))
                        .map_err(string_error)?;
                }
            }
            MainWindowPlacementStep::Maximize => {
                window.maximize().map_err(string_error)?;
            }
            MainWindowPlacementStep::LeaveNormal => {}
        }
    }
    Ok(())
}

fn floating_window_preferences_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join(FLOATING_WINDOW_PREFERENCES_FILE_NAME))
        .map_err(string_error)
}

fn floating_window_size_bounds() -> ((f64, f64), (f64, f64)) {
    ((260.0, 56.0), (800.0, 800.0))
}

fn validate_floating_window_preferences(
    preferences: &FloatingWindowPreferences,
) -> Result<(), String> {
    let ((min_width, min_height), (max_width, max_height)) = floating_window_size_bounds();
    if !(min_width..=max_width).contains(&preferences.width) {
        return Err("floating window width must be between 260 and 800".to_string());
    }
    if !(min_height..=max_height).contains(&preferences.height) {
        return Err("floating window height must be between 56 and 800".to_string());
    }
    for coordinate in [preferences.x, preferences.y].into_iter().flatten() {
        if !(-10_000..=10_000).contains(&coordinate) {
            return Err("floating window coordinates must be between -10000 and 10000".to_string());
        }
    }
    Ok(())
}

fn read_floating_window_preferences_from_disk(app: &AppHandle) -> FloatingWindowPreferences {
    let Ok(path) = floating_window_preferences_path(app) else {
        return FloatingWindowPreferences::default();
    };
    let Ok(content) = fs::read_to_string(path) else {
        return FloatingWindowPreferences::default();
    };
    serde_json::from_str::<FloatingWindowPreferences>(&content)
        .ok()
        .filter(|preferences| validate_floating_window_preferences(preferences).is_ok())
        .unwrap_or_default()
}

fn save_floating_window_preferences_to_disk_unlocked(
    app: &AppHandle,
    preferences: &FloatingWindowPreferences,
) -> Result<(), String> {
    validate_floating_window_preferences(preferences)?;
    let path = floating_window_preferences_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(string_error)?;
    }
    let content = serde_json::to_vec_pretty(preferences).map_err(string_error)?;
    fs::write(path, content).map_err(string_error)
}

fn read_floating_window_preferences(app: &AppHandle) -> FloatingWindowPreferences {
    let state = app.state::<AppState>();
    let _guard = state
        .floating_window_preferences_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    read_floating_window_preferences_from_disk(app)
}

fn update_floating_window_preferences(
    app: &AppHandle,
    update: impl FnOnce(&mut FloatingWindowPreferences),
) -> Result<FloatingWindowPreferences, String> {
    let state = app.state::<AppState>();
    let _guard = state
        .floating_window_preferences_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut preferences = read_floating_window_preferences_from_disk(app);
    update(&mut preferences);
    save_floating_window_preferences_to_disk_unlocked(app, &preferences)?;
    Ok(preferences)
}

fn install_floating_window_tracking(app: &AppHandle, window: &WebviewWindow) {
    let window_for_events = window.clone();
    let app_for_events = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = window_for_events.hide();
            let _ = update_floating_window_preferences(&app_for_events, |preferences| {
                preferences.visible = false;
            });
            return;
        }

        match event {
            WindowEvent::Moved(position) => {
                let Ok(scale_factor) = window_for_events.scale_factor() else {
                    return;
                };
                let logical: LogicalPosition<f64> = position.to_logical(scale_factor);
                let _ = update_floating_window_preferences(&app_for_events, |preferences| {
                    preferences.x = Some(logical.x.round() as i32);
                    preferences.y = Some(logical.y.round() as i32);
                });
            }
            WindowEvent::Resized(size) => {
                let Ok(scale_factor) = window_for_events.scale_factor() else {
                    return;
                };
                let logical: LogicalSize<f64> = size.to_logical(scale_factor);
                let actual = (logical.width.round() as u32, logical.height.round() as u32);
                let user_resized = {
                    let state = app_for_events.state::<AppState>();
                    let Ok(mut runtime) = state.floating_window_runtime.lock() else {
                        return;
                    };
                    classify_floating_resize(&mut runtime, actual, now_unix_ms())
                };
                let _ = update_floating_window_preferences(&app_for_events, |preferences| {
                    preferences.width = f64::from(actual.0);
                    preferences.height = f64::from(actual.1);
                    if user_resized {
                        preferences.user_resized = true;
                    }
                });
            }
            _ => {}
        }
    });
}

fn set_floating_window_size(
    app: &AppHandle,
    window: &WebviewWindow,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let target = (width.round() as u32, height.round() as u32);
    let deadline = now_unix_ms() + 1_500;
    let generation = {
        let state = app.state::<AppState>();
        let mut runtime = state.floating_window_runtime.lock().map_err(string_error)?;
        install_programmatic_resize_target(&mut runtime, target, deadline)
    };

    if let Err(error) = window.set_size(LogicalSize::new(width, height)) {
        let state = app.state::<AppState>();
        let mut runtime = state.floating_window_runtime.lock().map_err(string_error)?;
        clear_failed_programmatic_resize(&mut runtime, target, deadline, generation);
        return Err(string_error(error));
    }
    Ok(())
}

fn show_floating_window_internal(app: &AppHandle) -> Result<(), String> {
    let preferences = read_floating_window_preferences(app);
    if let Some(window) = app.get_webview_window(FLOATING_WINDOW_LABEL) {
        window
            .set_always_on_top(preferences.always_on_top)
            .map_err(string_error)?;
        window.show().map_err(string_error)?;
        window.set_focus().map_err(string_error)?;
        update_floating_window_preferences(app, |preferences| {
            preferences.visible = true;
        })?;
        return Ok(());
    }

    let app_for_thread = app.clone();
    let preferences_for_thread = preferences.clone();
    thread::spawn(move || {
        if let Err(error) = create_floating_window(&app_for_thread, &preferences_for_thread) {
            eprintln!("创建悬浮窗失败: {error}");
        }
    });
    Ok(())
}

fn create_floating_window(
    app: &AppHandle,
    preferences: &FloatingWindowPreferences,
) -> Result<(), String> {
    let ((min_width, min_height), (max_width, max_height)) = floating_window_size_bounds();
    let window = WebviewWindowBuilder::new(
        app,
        FLOATING_WINDOW_LABEL,
        WebviewUrl::App("?window=floating".into()),
    )
    .title("StarToDo 悬浮窗")
    .inner_size(preferences.width, preferences.height)
    .min_inner_size(min_width, min_height)
    .max_inner_size(max_width, max_height)
    .decorations(false)
    .always_on_top(preferences.always_on_top)
    .skip_taskbar(true)
    .resizable(true)
    .maximizable(false)
    .build()
    .map_err(string_error)?;

    if let (Some(x), Some(y)) = (preferences.x, preferences.y) {
        window
            .set_position(LogicalPosition::<f64>::new(x as f64, y as f64))
            .map_err(string_error)?;
    }
    install_floating_window_tracking(app, &window);

    update_floating_window_preferences(app, |preferences| {
        preferences.visible = true;
    })?;
    window.show().map_err(string_error)?;
    window.set_focus().map_err(string_error)
}

fn hide_floating_window_internal(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(FLOATING_WINDOW_LABEL) {
        window.hide().map_err(string_error)?;
    }
    update_floating_window_preferences(app, |preferences| {
        preferences.visible = false;
    })?;
    Ok(())
}

fn toggle_floating_window_internal(app: &AppHandle) -> Result<(), String> {
    let visible = app
        .get_webview_window(FLOATING_WINDOW_LABEL)
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false);
    if visible {
        hide_floating_window_internal(app)
    } else {
        show_floating_window_internal(app)
    }
}

fn install_main_window_tracking(app: &AppHandle, window: &WebviewWindow) {
    let window_for_events = window.clone();
    let app_for_events = app.clone();
    window.on_window_event(move |event| {
        if !matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_)) {
            return;
        }
        let fullscreen = window_for_events.is_fullscreen().unwrap_or(false);
        let mut preferences = read_window_preferences(&app_for_events);
        if fullscreen {
            preferences.last_immersive = true;
            let _ = save_window_preferences_to_disk(&app_for_events, &preferences);
            return;
        }
        preferences.last_immersive = false;
        preferences.maximized = window_for_events.is_maximized().unwrap_or(false);
        if !preferences.maximized {
            if let Ok(scale) = window_for_events.scale_factor() {
                if let Ok(position) = window_for_events.outer_position() {
                    let p: LogicalPosition<f64> = position.to_logical(scale);
                    preferences.normal_bounds.x = Some(p.x.round() as i32);
                    preferences.normal_bounds.y = Some(p.y.round() as i32);
                }
                if let Ok(size) = window_for_events.inner_size() {
                    let s: LogicalSize<f64> = size.to_logical(scale);
                    preferences.normal_bounds.width = s.width.round() as u32;
                    preferences.normal_bounds.height = s.height.round() as u32;
                }
            }
        }
        let _ = save_window_preferences_to_disk(&app_for_events, &preferences);
    });
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
        install_close_to_tray_behavior(&window);
    }

    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "main window was not created".to_string())?;
    apply_window_preferences(&window, &preferences)?;
    install_main_window_tracking(app, &window);
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
    emit_tasks_changed(&app);
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
    app: AppHandle,
    id: i64,
    planned_date: Option<String>,
    state: State<'_, AppState>,
) -> Result<tasks::Task, String> {
    let task = {
        let mut connection = state.database.lock().map_err(string_error)?;
        tasks::set_planned_date(&mut connection, id, planned_date)?
    };
    emit_tasks_changed(&app);
    Ok(task)
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
    emit_tasks_changed(&app);
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
    emit_tasks_changed(&app);
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
    emit_tasks_changed(&app);
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PomodoroCommandResult {
    snapshot: pomodoro::PomodoroSnapshot,
    session: Option<pomodoro::PomodoroSession>,
    notification_tag_to_cancel: Option<String>,
    notification_warning: Option<String>,
}

fn emit_pomodoro_state_changed(app: &AppHandle, snapshot: &pomodoro::PomodoroSnapshot) {
    let _ = app.emit("pomodoro-state-changed", snapshot);
}

fn list_pomodoro_notifications(
    app: &AppHandle,
) -> Result<Vec<reminders::ScheduledReminder>, String> {
    let (_, response) = invoke_notification_host(
        app,
        &json!({
            "operation": "list",
            "channel": POMODORO_NOTIFICATION_CHANNEL,
        }),
    )?;
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
                .ok_or_else(|| "scheduled pomodoro notification has no tag".to_string())?
                .to_string();
            let due_at_utc = item
                .get("dueAtUtc")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("scheduled pomodoro notification {tag} has no dueAtUtc"))?;
            let due_at_unix_ms = DateTime::parse_from_rfc3339(due_at_utc)
                .map_err(|error| {
                    format!("scheduled pomodoro notification {tag} has invalid dueAtUtc: {error}")
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

fn cancel_pomodoro_notification(app: &AppHandle, tag: &str) -> Result<(), String> {
    invoke_notification_host(
        app,
        &json!({
            "operation": "cancel",
            "id": tag,
            "channel": POMODORO_NOTIFICATION_CHANNEL,
        }),
    )
    .map(|_| ())
}

fn cancel_optional_pomodoro_notification(
    app: &AppHandle,
    tag: Option<&str>,
    warning: &mut Option<String>,
) {
    if let Some(tag) = tag {
        if let Err(error) = cancel_pomodoro_notification(app, tag) {
            append_pomodoro_warning(
                warning,
                format!("无法取消番茄钟通知，计时状态已保存：{error}"),
            );
        }
    }
}

fn append_pomodoro_warning(warning: &mut Option<String>, message: String) {
    *warning = Some(match warning.take() {
        Some(current) => format!("{current}；{message}"),
        None => message,
    });
}

fn pomodoro_notification_text(session: &pomodoro::PomodoroSession) -> (String, String) {
    let task = session.task_title_snapshot.as_deref().unwrap_or("独立专注");
    match session.phase {
        pomodoro::PomodoroPhase::Focus => (
            "专注阶段完成".to_string(),
            format!("「{task}」的专注时间已结束。请休息一下，再手动开始下一阶段。"),
        ),
        pomodoro::PomodoroPhase::ShortBreak => (
            "短休息结束".to_string(),
            "短休息已结束。准备好后可手动开始下一次专注。".to_string(),
        ),
        pomodoro::PomodoroPhase::LongBreak => (
            "长休息结束".to_string(),
            "长休息已结束。准备好后可手动开始新的专注周期。".to_string(),
        ),
    }
}

fn prepare_and_schedule_pomodoro_notification(
    app: &AppHandle,
    session: &pomodoro::PomodoroSession,
    now: i64,
) -> Result<(pomodoro::PomodoroSession, pomodoro::PomodoroSnapshot), String> {
    let token = pomodoro::generate_notification_token()?;
    let (spec, prepared, snapshot) = {
        let state = app.state::<AppState>();
        let mut connection = state.database.lock().map_err(string_error)?;
        let spec = pomodoro::prepare_notification_at(&mut connection, session.id, &token, now)?;
        let snapshot = pomodoro::get_at(&mut connection, now)?;
        let prepared = snapshot.current_session.clone().ok_or_else(|| {
            "pomodoro session disappeared while preparing notification".to_string()
        })?;
        (spec, prepared, snapshot)
    };
    let due_at_utc = prepared
        .target_ends_at_unix_ms
        .ok_or_else(|| "running pomodoro session has no target time".to_string())
        .and_then(unix_ms_to_utc)?;
    let (title, body) = pomodoro_notification_text(&prepared);
    invoke_notification_host(
        app,
        &json!({
            "operation": "schedule",
            "id": spec.tag,
            "title": title,
            "body": body,
            "dueAtUtc": due_at_utc,
            "activationUri": spec.activation_uri,
            "channel": POMODORO_NOTIFICATION_CHANNEL,
        }),
    )?;
    Ok((prepared, snapshot))
}

fn invalidate_pomodoro_wake(state: &AppState) {
    state
        .pomodoro_wake_generation
        .fetch_add(1, Ordering::AcqRel);
}

fn schedule_pomodoro_wake(app: &AppHandle, session: &pomodoro::PomodoroSession) {
    let Some(target) = session.target_ends_at_unix_ms else {
        return;
    };
    let generation = app
        .state::<AppState>()
        .pomodoro_wake_generation
        .fetch_add(1, Ordering::AcqRel)
        + 1;
    let app = app.clone();
    thread::spawn(move || {
        let delay = target.saturating_sub(tasks::now_unix_ms()).max(0) as u64;
        if delay > 0 {
            thread::sleep(Duration::from_millis(delay));
        }
        let state = app.state::<AppState>();
        if state.pomodoro_wake_generation.load(Ordering::Acquire) != generation {
            return;
        }
        let _operation = match state.pomodoro_operation_lock.lock() {
            Ok(guard) => guard,
            Err(error) => {
                eprintln!("pomodoro wake operation lock failed: {error}");
                return;
            }
        };
        if state.pomodoro_wake_generation.load(Ordering::Acquire) != generation {
            return;
        }
        if let Err(error) = reconcile_pomodoro_locked(&app, true) {
            eprintln!("pomodoro wake reconciliation failed: {error}");
        }
    });
}

/// Must be called with `pomodoro_operation_lock` held. It never holds the
/// database mutex while talking to the notification sidecar.
fn reconcile_pomodoro_at_locked(
    app: &AppHandle,
    emit_state: bool,
    now: i64,
) -> Result<(pomodoro::PomodoroSnapshot, Option<String>), String> {
    let state = app.state::<AppState>();
    let (settled_tag, mut snapshot) = {
        let mut connection = state.database.lock().map_err(string_error)?;
        let settled_tag =
            pomodoro::settle_at(&mut connection, now)?.and_then(|session| session.notification_tag);
        let snapshot = pomodoro::get_at(&mut connection, now)?;
        (settled_tag, snapshot)
    };
    let mut warning = None;
    if settled_tag.is_some() {
        invalidate_pomodoro_wake(&state);
    }
    let scheduled = match list_pomodoro_notifications(app) {
        Ok(items) => items,
        Err(error) => {
            append_pomodoro_warning(&mut warning, format!("无法列出番茄钟通知：{error}"));
            if let Some(session) = snapshot
                .current_session
                .as_ref()
                .filter(|session| session.status == pomodoro::PomodoroStatus::Running)
            {
                schedule_pomodoro_wake(app, session);
            } else {
                invalidate_pomodoro_wake(&state);
            }
            if emit_state {
                emit_pomodoro_state_changed(app, &snapshot);
            }
            return Ok((snapshot, warning));
        }
    };
    let running = snapshot
        .current_session
        .as_ref()
        .filter(|session| session.status == pomodoro::PomodoroStatus::Running)
        .cloned();
    let mut keep_tag = None;
    if let Some(session) = running.as_ref() {
        for item in &scheduled {
            let matches = match item.activation_uri.as_deref() {
                Some(activation_uri) => {
                    let connection = state.database.lock().map_err(string_error)?;
                    pomodoro::scheduled_notification_matches_running_session(
                        &connection,
                        session.id,
                        &item.tag,
                        item.due_at_unix_ms,
                        activation_uri,
                    )?
                }
                None => false,
            };
            if matches && keep_tag.is_none() {
                keep_tag = Some(item.tag.clone());
            }
        }
    }
    for item in &scheduled {
        if keep_tag.as_deref() != Some(item.tag.as_str()) {
            cancel_optional_pomodoro_notification(app, Some(&item.tag), &mut warning);
        }
    }
    if let Some(session) = running {
        if keep_tag.is_none() {
            match prepare_and_schedule_pomodoro_notification(app, &session, now) {
                Ok((prepared, prepared_snapshot)) => {
                    snapshot = prepared_snapshot;
                    schedule_pomodoro_wake(app, &prepared);
                }
                Err(error) => {
                    append_pomodoro_warning(
                        &mut warning,
                        format!("无法安排番茄钟通知，计时仍会继续：{error}"),
                    );
                    schedule_pomodoro_wake(app, &session);
                }
            }
        } else {
            schedule_pomodoro_wake(app, &session);
        }
    } else {
        invalidate_pomodoro_wake(&state);
    }
    if emit_state {
        emit_pomodoro_state_changed(app, &snapshot);
    }
    Ok((snapshot, warning))
}

fn reconcile_pomodoro_locked(
    app: &AppHandle,
    emit_state: bool,
) -> Result<(pomodoro::PomodoroSnapshot, Option<String>), String> {
    reconcile_pomodoro_at_locked(app, emit_state, tasks::now_unix_ms())
}

fn reconcile_pomodoro(
    app: &AppHandle,
    emit_state: bool,
) -> Result<(pomodoro::PomodoroSnapshot, Option<String>), String> {
    let state = app.state::<AppState>();
    let _operation = state.pomodoro_operation_lock.lock().map_err(string_error)?;
    reconcile_pomodoro_locked(app, emit_state)
}

fn complete_pomodoro_mutation_locked(
    app: &AppHandle,
    mutation: pomodoro::PomodoroMutationResult,
) -> Result<PomodoroCommandResult, String> {
    let state = app.state::<AppState>();
    invalidate_pomodoro_wake(&state);
    let mut warning = None;
    cancel_optional_pomodoro_notification(
        app,
        mutation.notification_tag_to_cancel.as_deref(),
        &mut warning,
    );
    let (snapshot, reconciliation_warning) = reconcile_pomodoro_locked(app, false)?;
    if let Some(message) = reconciliation_warning {
        append_pomodoro_warning(&mut warning, message);
    }
    let session = snapshot.current_session.clone();
    emit_pomodoro_state_changed(app, &snapshot);
    Ok(PomodoroCommandResult {
        snapshot,
        session,
        notification_tag_to_cancel: mutation.notification_tag_to_cancel,
        notification_warning: warning,
    })
}

fn restore_pomodoro_for_app(app: &AppHandle) {
    match reconcile_pomodoro(app, true) {
        Ok((_, Some(warning))) => eprintln!("pomodoro startup reconciliation warning: {warning}"),
        Ok((_, None)) => {}
        Err(error) => eprintln!("pomodoro startup reconciliation failed: {error}"),
    }
}

#[tauri::command]
fn get_pomodoro(app: AppHandle) -> Result<pomodoro::PomodoroSnapshot, String> {
    let (snapshot, warning) = reconcile_pomodoro(&app, false)?;
    if let Some(warning) = warning {
        eprintln!("pomodoro get reconciliation warning: {warning}");
    }
    Ok(snapshot)
}

#[tauri::command]
fn get_pomodoro_view(app: AppHandle) -> Result<pomodoro::PomodoroView, String> {
    let state = app.state::<AppState>();
    let _operation = state.pomodoro_operation_lock.lock().map_err(string_error)?;
    let now = tasks::now_unix_ms();
    let (_, warning) = reconcile_pomodoro_at_locked(&app, false, now)?;
    if let Some(warning) = warning {
        eprintln!("pomodoro view reconciliation warning: {warning}");
    }
    let mut connection = state.database.lock().map_err(string_error)?;
    pomodoro::view_at(&mut connection, now)
}

fn run_pomodoro_mutation(
    app: &AppHandle,
    operation: impl FnOnce(&mut Connection, i64) -> Result<pomodoro::PomodoroMutationResult, String>,
) -> Result<PomodoroCommandResult, String> {
    let state = app.state::<AppState>();
    let _operation = state.pomodoro_operation_lock.lock().map_err(string_error)?;
    let mutation = {
        let mut connection = state.database.lock().map_err(string_error)?;
        operation(&mut connection, tasks::now_unix_ms())?
    };
    complete_pomodoro_mutation_locked(app, mutation)
}

#[tauri::command]
fn update_pomodoro_settings(
    app: AppHandle,
    input: pomodoro::UpdatePomodoroSettingsInput,
) -> Result<PomodoroCommandResult, String> {
    run_pomodoro_mutation(&app, |connection, now| {
        pomodoro::update_settings_at(connection, input, now)
    })
}

#[tauri::command]
fn start_pomodoro(
    app: AppHandle,
    input: pomodoro::StartPomodoroInput,
) -> Result<PomodoroCommandResult, String> {
    run_pomodoro_mutation(&app, |connection, now| {
        pomodoro::start_at(connection, input, now)
    })
}

#[tauri::command]
fn pause_pomodoro(app: AppHandle) -> Result<PomodoroCommandResult, String> {
    run_pomodoro_mutation(&app, pomodoro::pause_at)
}

#[tauri::command]
fn resume_pomodoro(app: AppHandle) -> Result<PomodoroCommandResult, String> {
    run_pomodoro_mutation(&app, pomodoro::resume_at)
}

#[tauri::command]
fn skip_pomodoro(app: AppHandle) -> Result<PomodoroCommandResult, String> {
    run_pomodoro_mutation(&app, pomodoro::skip_at)
}

#[tauri::command]
fn reset_pomodoro(app: AppHandle) -> Result<PomodoroCommandResult, String> {
    run_pomodoro_mutation(&app, pomodoro::reset_at)
}

#[tauri::command]
fn list_pomodoro_task_summaries(
    app: AppHandle,
) -> Result<Vec<pomodoro::PomodoroTaskSummary>, String> {
    let state = app.state::<AppState>();
    let _operation = state.pomodoro_operation_lock.lock().map_err(string_error)?;
    let (_, warning) = reconcile_pomodoro_locked(&app, false)?;
    if let Some(warning) = warning {
        eprintln!("pomodoro summary reconciliation warning: {warning}");
    }
    let mut connection = state.database.lock().map_err(string_error)?;
    pomodoro::list_task_summaries_at(&mut connection, tasks::now_unix_ms())
}

#[tauri::command]
fn claim_pending_pomodoro_activations(
    consumer_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<pomodoro::PendingPomodoroActivation>, String> {
    let _operation = state.pomodoro_operation_lock.lock().map_err(string_error)?;
    let mut connection = state.database.lock().map_err(string_error)?;
    pomodoro::claim_pending_activations_at(&mut connection, &consumer_id, 1, tasks::now_unix_ms())
}

#[tauri::command]
fn acknowledge_pending_pomodoro_activations(
    consumer_id: String,
    ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<pomodoro::PomodoroActivationAckResult, String> {
    let _operation = state.pomodoro_operation_lock.lock().map_err(string_error)?;
    let mut connection = state.database.lock().map_err(string_error)?;
    pomodoro::acknowledge_pending_activations_at(
        &mut connection,
        &consumer_id,
        &ids,
        tasks::now_unix_ms(),
    )
}

struct NotificationReminderHost {
    app: AppHandle,
}

impl reminder_orchestrator::ReminderHost for NotificationReminderHost {
    fn list(&self) -> Result<Vec<reminders::ScheduledReminder>, String> {
        let (_, response) = invoke_notification_host(
            &self.app,
            &json!({ "operation": "list", "channel": TASK_NOTIFICATION_CHANNEL }),
        )?;
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
                "channel": TASK_NOTIFICATION_CHANNEL,
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
                "channel": TASK_NOTIFICATION_CHANNEL,
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
    match invoke_notification_host_async(
        app,
        json!({ "operation": "diagnostics", "channel": TASK_NOTIFICATION_CHANNEL }),
    )
    .await
    {
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
            "channel": TASK_NOTIFICATION_CHANNEL,
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
            "channel": TASK_NOTIFICATION_CHANNEL,
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
            "channel": TASK_NOTIFICATION_CHANNEL,
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

fn current_window_state(app: &AppHandle) -> Result<WindowState, String> {
    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "main window is not available".to_string())?;
    let maximized = window.is_maximized().map_err(string_error)?;
    let fullscreen = window.is_fullscreen().map_err(string_error)?;
    let mut preferences = read_window_preferences(app);
    if !maximized && !fullscreen {
        if let Ok(size) = window.inner_size() {
            let position = window.outer_position().ok();
            let scale_factor = window.scale_factor().unwrap_or(1.0);
            preferences.normal_bounds = WindowBounds::from_physical(position, size, scale_factor);
        }
    }
    Ok(WindowState {
        maximized,
        fullscreen,
        normal_bounds: preferences.normal_bounds,
    })
}

#[tauri::command]
fn get_window_state(app: AppHandle) -> Result<WindowState, String> {
    current_window_state(&app)
}

#[tauri::command]
fn enter_immersive_mode(app: AppHandle, state: State<'_, AppState>) -> Result<WindowState, String> {
    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "main window is not available".to_string())?;
    let preferences = read_window_preferences(&app);
    let maximized = window.is_maximized().map_err(string_error)?;
    let fullscreen = window.is_fullscreen().map_err(string_error)?;
    if fullscreen {
        return current_window_state(&app);
    }
    let current_bounds = if !maximized && !fullscreen {
        let size = window.inner_size().map_err(string_error)?;
        let position = window.outer_position().map_err(string_error).ok();
        let scale_factor = window.scale_factor().map_err(string_error)?;
        Some(WindowBounds::from_physical(position, size, scale_factor))
    } else {
        None
    };
    let normal_bounds = select_immersive_restore_bounds(
        maximized,
        fullscreen,
        &preferences.normal_bounds,
        current_bounds,
    );
    let restore = ImmersiveRestoreState {
        maximized,
        normal_bounds,
    };
    window.set_fullscreen(true).map_err(string_error)?;
    *state.immersive_restore_state.lock().map_err(string_error)? = Some(restore);
    let mut updated = preferences;
    updated.last_immersive = true;
    save_window_preferences_to_disk(&app, &updated)?;
    current_window_state(&app)
}

#[tauri::command]
fn exit_immersive_mode(app: AppHandle, state: State<'_, AppState>) -> Result<WindowState, String> {
    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "main window is not available".to_string())?;
    window.set_fullscreen(false).map_err(string_error)?;
    let mut preferences = read_window_preferences(&app);
    let restore = state
        .immersive_restore_state
        .lock()
        .map_err(string_error)?
        .take()
        .unwrap_or(ImmersiveRestoreState {
            maximized: preferences.maximized,
            normal_bounds: preferences.normal_bounds.clone(),
        });
    match restore.restore_target() {
        WindowRestoreTarget::Maximized => window.maximize().map_err(string_error)?,
        WindowRestoreTarget::Normal(bounds) => {
            window.unmaximize().map_err(string_error)?;
            window
                .set_size(LogicalSize::new(bounds.width, bounds.height))
                .map_err(string_error)?;
            if let (Some(x), Some(y)) = (bounds.x, bounds.y) {
                window
                    .set_position(LogicalPosition::new(x, y))
                    .map_err(string_error)?;
            }
            preferences.normal_bounds = bounds;
        }
    }
    preferences.maximized = restore.maximized;
    preferences.last_immersive = false;
    save_window_preferences_to_disk(&app, &preferences)?;
    current_window_state(&app)
}

#[tauri::command]
fn set_main_window_maximized(app: AppHandle, maximized: bool) -> Result<WindowState, String> {
    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "main window is not available".to_string())?;
    if maximized {
        window.maximize().map_err(string_error)?;
    } else {
        window.unmaximize().map_err(string_error)?;
    }
    let mut preferences = read_window_preferences(&app);
    preferences.maximized = maximized;
    preferences.last_immersive = false;
    save_window_preferences_to_disk(&app, &preferences)?;
    current_window_state(&app)
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

#[tauri::command]
fn get_floating_window_preferences(app: AppHandle) -> FloatingWindowPreferences {
    read_floating_window_preferences(&app)
}

#[tauri::command]
fn set_floating_display_mode(
    app: AppHandle,
    mode: FloatingDisplayMode,
) -> Result<FloatingWindowPreferences, String> {
    let preferences = update_floating_window_preferences(&app, |preferences| {
        preferences.display_mode = mode;
        if !preferences.user_resized {
            let (width, height) = floating_display_size(mode);
            preferences.width = width;
            preferences.height = height;
        }
    })?;
    if !preferences.user_resized {
        if let Some(window) = app.get_webview_window(FLOATING_WINDOW_LABEL) {
            set_floating_window_size(&app, &window, preferences.width, preferences.height)?;
        }
    }
    Ok(preferences)
}

#[tauri::command]
fn reset_floating_auto_size(app: AppHandle) -> Result<FloatingWindowPreferences, String> {
    let preferences = update_floating_window_preferences(&app, |preferences| {
        preferences.user_resized = false;
        let (width, height) = floating_display_size(preferences.display_mode);
        preferences.width = width;
        preferences.height = height;
    })?;
    if let Some(window) = app.get_webview_window(FLOATING_WINDOW_LABEL) {
        set_floating_window_size(&app, &window, preferences.width, preferences.height)?;
    }
    Ok(preferences)
}

#[tauri::command]
fn show_floating_window(app: AppHandle) -> Result<(), String> {
    show_floating_window_internal(&app)
}

#[tauri::command]
fn hide_floating_window(app: AppHandle) -> Result<(), String> {
    hide_floating_window_internal(&app)
}

#[tauri::command]
fn toggle_floating_window(app: AppHandle) -> Result<(), String> {
    toggle_floating_window_internal(&app)
}

#[tauri::command]
fn open_task_from_floating(
    app: AppHandle,
    task_id: i64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if task_id <= 0 {
        return Err("task id must be positive".to_string());
    }
    {
        let mut pending = state.pending_floating_intent.lock().map_err(string_error)?;
        *pending = Some(FloatingIntent {
            view: "tasks".to_string(),
            task_id: Some(task_id),
        });
    }
    show_or_create_main_window(&app);
    let _ = app.emit("floating-intent-available", ());
    Ok(())
}

#[tauri::command]
fn open_focus_from_floating(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut pending = state.pending_floating_intent.lock().map_err(string_error)?;
        *pending = Some(FloatingIntent {
            view: "focus".to_string(),
            task_id: None,
        });
    }
    show_or_create_main_window(&app);
    let _ = app.emit("floating-intent-available", ());
    Ok(())
}

#[tauri::command]
fn take_pending_floating_intent(
    state: State<'_, AppState>,
) -> Result<Option<FloatingIntent>, String> {
    let mut pending = state.pending_floating_intent.lock().map_err(string_error)?;
    Ok(pending.take())
}

fn install_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let focus = MenuItem::with_id(app, "focus", "Focus", true, None::<&str>)?;
    let floating = MenuItem::with_id(app, "floating", "悬浮窗", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "Hide", true, None::<&str>)?;
    let release_ui = MenuItem::with_id(app, "release_ui", "Release UI", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &focus, &floating, &hide, &release_ui, &quit])?;

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
            "focus" => {
                let state = app.state::<AppState>();
                if let Ok(mut pending) = state.pending_floating_intent.lock() {
                    *pending = Some(FloatingIntent {
                        view: "focus".to_string(),
                        task_id: None,
                    });
                }
                show_or_create_main_window(app);
                let _ = app.emit("floating-intent-available", ());
            }
            "floating" => {
                let _ = toggle_floating_window_internal(app);
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

fn should_prevent_exit(code: Option<i32>) -> bool {
    code.is_none()
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
                pomodoro_wake_generation: AtomicU64::new(0),
                pomodoro_operation_lock: Mutex::new(()),
                immersive_restore_state: Mutex::new(None),
                pending_floating_intent: Mutex::new(None),
                floating_window_preferences_lock: Mutex::new(()),
                floating_window_runtime: Mutex::new(FloatingWindowRuntime::default()),
            });
            install_activation_listener(app);
            queue_current_activation(app);
            install_tray(app)?;

            if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                let preferences = read_window_preferences(app.handle());
                apply_window_preferences(&window, &preferences).map_err(std::io::Error::other)?;
                install_close_to_tray_behavior(&window);
                install_main_window_tracking(app.handle(), &window);
                show_existing_window(&window).map_err(std::io::Error::other)?;
            }

            let pomodoro_app = app.handle().clone();
            thread::spawn(move || restore_pomodoro_for_app(&pomodoro_app));

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
            claim_pending_pomodoro_activations,
            acknowledge_pending_pomodoro_activations,
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
            get_window_state,
            enter_immersive_mode,
            exit_immersive_mode,
            set_main_window_maximized,
            set_always_on_top,
            get_floating_window_preferences,
            set_floating_display_mode,
            reset_floating_auto_size,
            show_floating_window,
            hide_floating_window,
            toggle_floating_window,
            open_task_from_floating,
            open_focus_from_floating,
            take_pending_floating_intent,
            get_pomodoro,
            get_pomodoro_view,
            update_pomodoro_settings,
            start_pomodoro,
            pause_pomodoro,
            resume_pomodoro,
            skip_pomodoro,
            reset_pomodoro,
            list_pomodoro_task_summaries
        ])
        .build(tauri::generate_context!())
        .expect("error while building StarToDo")
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested { code, api, .. } = event {
                if should_prevent_exit(code) {
                    api.prevent_exit();
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implicit_last_window_exit_is_prevented_but_explicit_exit_is_allowed() {
        assert!(should_prevent_exit(None));
        assert!(!should_prevent_exit(Some(0)));
        assert!(!should_prevent_exit(Some(7)));
    }

    #[test]
    fn maximized_startup_seeds_normal_placement_before_final_maximize() {
        assert_eq!(
            main_window_placement_plan(true),
            [
                MainWindowPlacementStep::Unmaximize,
                MainWindowPlacementStep::SetNormalSize,
                MainWindowPlacementStep::SetNormalPosition,
                MainWindowPlacementStep::Maximize,
            ]
        );
        assert_eq!(
            main_window_placement_plan(false),
            [
                MainWindowPlacementStep::Unmaximize,
                MainWindowPlacementStep::SetNormalSize,
                MainWindowPlacementStep::SetNormalPosition,
                MainWindowPlacementStep::LeaveNormal,
            ]
        );
    }

    #[test]
    fn floating_display_modes_have_recommended_sizes() {
        assert_eq!(
            floating_display_size(FloatingDisplayMode::Capsule),
            (340.0, 64.0)
        );
        assert_eq!(
            floating_display_size(FloatingDisplayMode::Expanded),
            (360.0, 260.0)
        );
    }

    #[test]
    fn floating_window_bounds_are_exact() {
        assert_eq!(
            floating_window_size_bounds(),
            ((260.0, 56.0), (800.0, 800.0))
        );
    }

    #[test]
    fn legacy_floating_preferences_enable_auto_size() {
        let preferences = serde_json::from_str::<FloatingWindowPreferences>(
            r#"{"visible":true,"x":10,"y":20,"width":340,"height":180,"alwaysOnTop":true}"#,
        )
        .expect("legacy floating preferences should decode");

        assert!(!preferences.user_resized);
        assert_eq!(preferences.display_mode, FloatingDisplayMode::Expanded);
    }

    #[test]
    fn matching_programmatic_resize_before_deadline_clears_target_without_marking_user() {
        let mut runtime = FloatingWindowRuntime {
            programmatic_target: Some((340, 64)),
            programmatic_until_unix_ms: 1_500,
            ..FloatingWindowRuntime::default()
        };

        assert!(!classify_floating_resize(&mut runtime, (340, 64), 1_500));
        assert_eq!(runtime.programmatic_target, None);
        assert_eq!(runtime.programmatic_until_unix_ms, 0);
    }

    #[test]
    fn intermediate_programmatic_resize_before_deadline_retains_target() {
        let mut runtime = FloatingWindowRuntime {
            programmatic_target: Some((340, 64)),
            programmatic_until_unix_ms: 1_500,
            ..FloatingWindowRuntime::default()
        };

        assert!(!classify_floating_resize(&mut runtime, (350, 100), 1_499));
        assert_eq!(runtime.programmatic_target, Some((340, 64)));
        assert_eq!(runtime.programmatic_until_unix_ms, 1_500);
    }

    #[test]
    fn expired_programmatic_resize_clears_target_and_marks_user() {
        let mut runtime = FloatingWindowRuntime {
            programmatic_target: Some((340, 64)),
            programmatic_until_unix_ms: 1_500,
            ..FloatingWindowRuntime::default()
        };

        assert!(classify_floating_resize(&mut runtime, (340, 64), 1_501));
        assert_eq!(runtime.programmatic_target, None);
        assert_eq!(runtime.programmatic_until_unix_ms, 0);
    }

    #[test]
    fn resize_without_programmatic_target_marks_user() {
        let mut runtime = FloatingWindowRuntime::default();

        assert!(classify_floating_resize(&mut runtime, (360, 260), 1_000));
        assert_eq!(runtime.programmatic_target, None);
        assert_eq!(runtime.programmatic_until_unix_ms, 0);
    }

    #[test]
    fn repeated_programmatic_resize_requests_receive_distinct_generations() {
        let mut runtime = FloatingWindowRuntime::default();

        let first = install_programmatic_resize_target(&mut runtime, (340, 64), 1_500);
        let second = install_programmatic_resize_target(&mut runtime, (340, 64), 1_500);

        assert_ne!(first, second);
        assert_eq!(runtime.programmatic_generation, second);
    }

    #[test]
    fn failed_older_resize_does_not_clear_newer_identical_target() {
        let mut runtime = FloatingWindowRuntime::default();
        let first = install_programmatic_resize_target(&mut runtime, (340, 64), 1_500);
        let second = install_programmatic_resize_target(&mut runtime, (340, 64), 1_500);

        clear_failed_programmatic_resize(&mut runtime, (340, 64), 1_500, first);

        assert_eq!(runtime.programmatic_target, Some((340, 64)));
        assert_eq!(runtime.programmatic_until_unix_ms, 1_500);
        assert_eq!(runtime.programmatic_generation, second);
    }

    #[test]
    fn immersive_restore_bounds_prefer_current_only_when_window_is_normal() {
        let persisted = WindowBounds {
            x: Some(1),
            y: Some(2),
            width: 500,
            height: 400,
        };
        let current = WindowBounds {
            x: Some(30),
            y: Some(40),
            width: 900,
            height: 700,
        };
        assert_eq!(
            select_immersive_restore_bounds(false, false, &persisted, Some(current.clone())),
            current
        );
        assert_eq!(
            select_immersive_restore_bounds(true, false, &persisted, Some(current.clone())),
            persisted
        );
        assert_eq!(
            select_immersive_restore_bounds(false, true, &persisted, Some(current)),
            persisted
        );
    }

    #[test]
    fn physical_window_bounds_convert_to_logical_coordinates() {
        let bounds = WindowBounds::from_physical(
            Some(tauri::PhysicalPosition::new(60, 80)),
            tauri::PhysicalSize::new(1800, 1400),
            2.0,
        );
        assert_eq!(
            bounds,
            WindowBounds {
                x: Some(30),
                y: Some(40),
                width: 900,
                height: 700
            }
        );
    }

    #[test]
    fn immersive_restore_prefers_maximized() {
        let restore = ImmersiveRestoreState {
            maximized: true,
            normal_bounds: WindowBounds::default(),
        };
        assert_eq!(restore.restore_target(), WindowRestoreTarget::Maximized);
    }

    #[test]
    fn immersive_restore_uses_normal_bounds() {
        let bounds = WindowBounds {
            x: Some(30),
            y: Some(40),
            width: 900,
            height: 700,
        };
        let restore = ImmersiveRestoreState {
            maximized: false,
            normal_bounds: bounds.clone(),
        };
        assert_eq!(
            restore.restore_target(),
            WindowRestoreTarget::Normal(bounds)
        );
    }

    #[test]
    fn compact_window_preferences_migrate_to_adaptive() {
        let decoded = decode_window_preferences(
            r#"{"mode":"compact","width":380,"height":520,"alwaysOnTop":true}"#,
        )
        .unwrap();
        assert!(decoded.migrated);
        assert_eq!(decoded.preferences.layout, WindowLayout::Adaptive);
        assert!(!decoded.preferences.maximized);
        assert_eq!(decoded.preferences.normal_bounds.width, 520);
        assert_eq!(decoded.preferences.normal_bounds.height, 520);
        assert!(decoded.preferences.always_on_top);
    }

    #[test]
    fn full_window_preferences_migrate_to_maximized_adaptive() {
        let decoded = decode_window_preferences(
            r#"{"mode":"full","width":800,"height":600,"alwaysOnTop":false}"#,
        )
        .unwrap();
        assert!(decoded.migrated);
        assert_eq!(decoded.preferences.layout, WindowLayout::Adaptive);
        assert!(decoded.preferences.maximized);
        assert_eq!(decoded.preferences.normal_bounds.width, 800);
        assert_eq!(decoded.preferences.normal_bounds.height, 600);
    }

    #[test]
    fn adaptive_window_preferences_reject_too_small_bounds() {
        let preferences = WindowPreferences {
            normal_bounds: WindowBounds {
                width: 519,
                height: 420,
                ..WindowBounds::default()
            },
            ..WindowPreferences::default()
        };
        assert_eq!(
            validate_window_preferences(&preferences).unwrap_err(),
            "window width must be between 520 and 7680"
        );
    }

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
                POMODORO_MIGRATION_VERSION,
                POMODORO_ACTIVATION_INBOX_MIGRATION_VERSION,
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
        assert_eq!(version_count, 9);
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
        assert_eq!(versions, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
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
        assert_eq!(versions, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn v7_database_upgrades_to_v9_with_pomodoro_foreign_key_partial_unique_index_and_activation_inbox(
    ) {
        let mut connection = Connection::open_in_memory().expect("in-memory database should open");
        connection
            .execute_batch(
                "
                PRAGMA foreign_keys = ON;
                CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at_unix_ms INTEGER NOT NULL
                );
                INSERT INTO schema_migrations (version, applied_at_unix_ms)
                VALUES (1, 1), (2, 2), (3, 3), (4, 4), (5, 5), (6, 6), (7, 7);
                CREATE TABLE tasks (
                    id INTEGER PRIMARY KEY,
                    title TEXT NOT NULL,
                    notes TEXT NOT NULL DEFAULT '',
                    planned_date TEXT,
                    due_at_unix_ms INTEGER,
                    reminder_at_unix_ms INTEGER,
                    reminder_fired_at_unix_ms INTEGER,
                    deleted_at_unix_ms INTEGER,
                    project_id INTEGER,
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
                CREATE TABLE reminder_deliveries (
                    id TEXT PRIMARY KEY,
                    task_id INTEGER NOT NULL,
                    reminder_at_unix_ms INTEGER NOT NULL,
                    activation_token_hash BLOB NOT NULL,
                    state TEXT NOT NULL,
                    created_at_unix_ms INTEGER NOT NULL,
                    scheduled_at_unix_ms INTEGER,
                    activated_at_unix_ms INTEGER,
                    generation INTEGER NOT NULL DEFAULT 0,
                    activation_uri_hash BLOB
                );
                CREATE TABLE stage0_probe (id INTEGER PRIMARY KEY, run_count INTEGER NOT NULL, last_probe_at_unix_ms INTEGER NOT NULL);
                ",
            )
            .expect("v7 fixture should be created");
        apply_migrations(&mut connection).expect("v7 database should upgrade to v9");

        let versions: Vec<i64> = connection
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .expect("versions should prepare")
            .query_map([], |row| row.get(0))
            .expect("versions should query")
            .collect::<rusqlite::Result<_>>()
            .expect("versions should collect");
        assert_eq!(versions, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);

        let foreign_key_target: String = connection
            .query_row(
                "SELECT \"table\" FROM pragma_foreign_key_list('pomodoro_sessions') WHERE \"from\" = 'task_id'",
                [],
                |row| row.get(0),
            )
            .expect("pomodoro task foreign key should exist");
        assert_eq!(foreign_key_target, "tasks");
        let partial_index_sql: String = connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = 'idx_pomodoro_one_active_session'",
                [],
                |row| row.get(0),
            )
            .expect("active-session partial index should exist");
        assert!(partial_index_sql.contains("WHERE status IN ('running', 'paused')"));
        let activation_inbox_index: String = connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = 'idx_pomodoro_activation_inbox_claimable'",
                [],
                |row| row.get(0),
            )
            .expect("activation inbox claimable index should exist");
        assert!(activation_inbox_index.contains("acknowledged_at_unix_ms"));
        connection
            .execute(
                "INSERT INTO pomodoro_sessions (phase, status, planned_duration_seconds, started_at_unix_ms, target_ends_at_unix_ms, created_at_unix_ms, updated_at_unix_ms) VALUES ('focus', 'running', 60, 1, 2, 1, 1)",
                [],
            )
            .expect("first active session should insert");
        assert!(connection
            .execute(
                "INSERT INTO pomodoro_sessions (phase, status, planned_duration_seconds, started_at_unix_ms, target_ends_at_unix_ms, created_at_unix_ms, updated_at_unix_ms) VALUES ('shortBreak', 'running', 60, 1, 2, 1, 1)",
                [],
            )
            .is_err());
    }

    #[test]
    fn v8_database_upgrades_to_v9_activation_inbox_idempotently() {
        let mut connection = Connection::open_in_memory().expect("in-memory database should open");
        apply_migrations(&mut connection).expect("base migrations should succeed");
        connection
            .execute_batch(
                "
                DROP INDEX idx_pomodoro_activation_inbox_claimable;
                DROP TABLE pomodoro_activation_inbox;
                DELETE FROM schema_migrations WHERE version = 9;
                ",
            )
            .expect("v9 artifacts should be removable for v8 fixture");

        apply_migrations(&mut connection).expect("v8 database should upgrade to v9");
        apply_migrations(&mut connection).expect("v9 migration should be idempotent");

        let inbox_columns: Vec<String> = connection
            .prepare("SELECT name FROM pragma_table_info('pomodoro_activation_inbox') ORDER BY cid")
            .expect("activation inbox columns should query")
            .query_map([], |row| row.get(0))
            .expect("activation inbox columns should decode")
            .collect::<rusqlite::Result<_>>()
            .expect("activation inbox columns should collect");
        assert_eq!(
            inbox_columns,
            vec![
                "id",
                "session_id",
                "received_at_unix_ms",
                "claimed_by",
                "claim_expires_at_unix_ms",
                "acknowledged_at_unix_ms",
            ]
        );
        let index_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_pomodoro_activation_inbox_claimable'",
                [],
                |row| row.get(0),
            )
            .expect("activation inbox index should query");
        assert_eq!(index_count, 1);
        let migration_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("migration count should query");
        assert_eq!(migration_count, 9);
    }

    #[test]
    fn database_probe_increments_without_duplicate_migrations() {
        let mut connection = Connection::open_in_memory().expect("in-memory database should open");
        let first = execute_database_probe(&mut connection, Path::new(":memory:"))
            .expect("first probe should succeed");
        let second = execute_database_probe(&mut connection, Path::new(":memory:"))
            .expect("second probe should succeed");

        assert_eq!(first.migration_count, 9);
        assert_eq!(second.migration_count, 9);
        assert_eq!(first.run_count, 1);
        assert_eq!(second.run_count, 2);
        assert!(second.last_probe_at_unix_ms >= first.last_probe_at_unix_ms);
    }
}
