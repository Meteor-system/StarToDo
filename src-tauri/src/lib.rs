use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::Connection;
use serde::Serialize;
use sysinfo::{
    get_current_pid, Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System,
    MINIMUM_CPU_UPDATE_INTERVAL,
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, State, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_notification::NotificationExt;

const MAIN_WINDOW_LABEL: &str = "main";
const DATABASE_FILE_NAME: &str = "star-to-do.sqlite3";
const STAGE0_MIGRATION_VERSION: i64 = 1;

struct AppState {
    database: Mutex<Connection>,
    database_path: PathBuf,
    ui_ready: AtomicBool,
    ui_rebuild_in_progress: AtomicBool,
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
    scheduled_notifications_status: &'static str,
    scheduled_notifications_detail: &'static str,
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

fn open_database(app: &AppHandle) -> Result<(Connection, PathBuf), String> {
    let data_dir = app.path().app_data_dir().map_err(string_error)?;
    fs::create_dir_all(&data_dir).map_err(string_error)?;

    let database_path = data_dir.join(DATABASE_FILE_NAME);
    let connection = Connection::open(&database_path).map_err(string_error)?;
    apply_migrations(&connection).map_err(string_error)?;
    Ok((connection, database_path))
}

fn apply_migrations(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at_unix_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS stage0_probe (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            run_count INTEGER NOT NULL DEFAULT 0,
            last_probe_at_unix_ms INTEGER NOT NULL
        );
        ",
    )?;

    connection.execute(
        "INSERT OR IGNORE INTO schema_migrations (version, applied_at_unix_ms) VALUES (?1, ?2)",
        (STAGE0_MIGRATION_VERSION, now_unix_ms() as i64),
    )?;
    Ok(())
}

fn execute_database_probe(
    connection: &Connection,
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

fn build_main_window_from_config(app: &AppHandle) -> Result<(), String> {
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

    RuntimeSnapshot {
        process_id: std::process::id(),
        main_window_exists,
        main_window_visible,
        ui_ready: state.ui_ready.load(Ordering::Acquire),
        ui_rebuild_in_progress: state.ui_rebuild_in_progress.load(Ordering::Acquire),
        database_path: state.database_path.display().to_string(),
    }
}

#[tauri::command]
fn get_runtime_snapshot(app: AppHandle, state: State<'_, AppState>) -> RuntimeSnapshot {
    runtime_snapshot(&app, &state)
}

#[tauri::command]
fn record_ui_ready(app: AppHandle, state: State<'_, AppState>) -> RuntimeSnapshot {
    state.ui_ready.store(true, Ordering::Release);
    runtime_snapshot(&app, &state)
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
    let connection = state.database.lock().map_err(string_error)?;
    execute_database_probe(&connection, &state.database_path).map_err(string_error)
}

#[tauri::command]
fn get_notification_diagnostics() -> NotificationDiagnostics {
    NotificationDiagnostics {
        immediate_notifications_available: true,
        scheduled_notifications_available: false,
        scheduled_notifications_status: "not-configured",
        scheduled_notifications_detail:
            "A notification scheduling sidecar has not been built or connected in Stage 0.",
    }
}

#[tauri::command]
fn send_test_notification(app: AppHandle) -> Result<(), String> {
    app.notification()
        .builder()
        .title("StarToDo")
        .body("Stage 0 notification test")
        .show()
        .map_err(string_error)
}

#[tauri::command]
fn hide_to_tray(app: AppHandle) -> Result<(), String> {
    hide_main_window(&app)
}

#[tauri::command]
fn release_ui(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.ui_ready.store(false, Ordering::Release);
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window.destroy().map_err(string_error)?;
    }
    Ok(())
}

#[tauri::command]
fn set_window_mode(app: AppHandle, mode: String) -> Result<(), String> {
    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "main window is not available".to_string())?;

    match mode.as_str() {
        "normal" => window.unmaximize().map_err(string_error),
        "minimized" => window.minimize().map_err(string_error),
        "maximized" => window.maximize().map_err(string_error),
        "fullscreen" => window.set_fullscreen(true).map_err(string_error),
        _ => {
            Err("window mode must be one of: normal, minimized, maximized, fullscreen".to_string())
        }
    }
}

#[tauri::command]
fn set_always_on_top(app: AppHandle, always_on_top: bool) -> Result<(), String> {
    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "main window is not available".to_string())?;
    window
        .set_always_on_top(always_on_top)
        .map_err(string_error)
}

fn install_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "Hide", true, None::<&str>)?;
    let release_ui = MenuItem::with_id(app, "release_ui", "Release UI", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &hide, &release_ui, &quit])?;

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
            "hide" => {
                let _ = hide_main_window(app);
            }
            "release_ui" => {
                app.state::<AppState>()
                    .ui_ready
                    .store(false, Ordering::Release);
                if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                    let _ = window.destroy();
                }
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
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let (database, database_path) =
                open_database(app.handle()).map_err(std::io::Error::other)?;
            app.manage(AppState {
                database: Mutex::new(database),
                database_path,
                ui_ready: AtomicBool::new(false),
                ui_rebuild_in_progress: AtomicBool::new(false),
            });
            install_tray(app)?;

            if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                install_close_to_tray_behavior(&window);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_runtime_snapshot,
            record_ui_ready,
            sample_process_metrics,
            run_database_probe,
            get_notification_diagnostics,
            send_test_notification,
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
        let connection = Connection::open_in_memory().expect("in-memory database should open");
        apply_migrations(&connection).expect("first migration pass should succeed");
        apply_migrations(&connection).expect("second migration pass should succeed");

        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("migration count should be queryable");
        assert_eq!(count, 1);
    }

    #[test]
    fn database_probe_increments_without_duplicate_migrations() {
        let connection = Connection::open_in_memory().expect("in-memory database should open");
        let first = execute_database_probe(&connection, Path::new(":memory:"))
            .expect("first probe should succeed");
        let second = execute_database_probe(&connection, Path::new(":memory:"))
            .expect("second probe should succeed");

        assert_eq!(first.migration_count, 1);
        assert_eq!(second.migration_count, 1);
        assert_eq!(first.run_count, 1);
        assert_eq!(second.run_count, 2);
        assert!(second.last_probe_at_unix_ms >= first.last_probe_at_unix_ms);
    }
}
