//! SQLite-authoritative Pomodoro state machine.
//!
//! `lib.rs` owns migration execution and native notification side effects. This
//! module owns durable, transactional state changes. Every public data function
//! accepts an injected wall-clock timestamp so restart recovery and unit tests
//! take precisely the same path.

use std::collections::HashSet;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{Duration as ChronoDuration, Local, TimeZone};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use url::Url;

pub const POMODORO_MIGRATION_VERSION: i64 = 8;
pub const POMODORO_ACTIVATION_INBOX_MIGRATION_VERSION: i64 = 9;
pub const POMODORO_NOTIFICATION_CHANNEL: &str = "pomodoro";
pub const POMODORO_ACTIVATION_CLAIM_LEASE_MS: i64 = 30_000;
const POMODORO_ACTIVATION_GRACE_MS: i64 = 24 * 60 * 60 * 1_000;
pub const DEFAULT_FOCUS_MINUTES: i64 = 25;
pub const DEFAULT_SHORT_BREAK_MINUTES: i64 = 5;
pub const DEFAULT_LONG_BREAK_MINUTES: i64 = 15;
pub const DEFAULT_LONG_BREAK_INTERVAL: i64 = 4;
const MAX_ACTIVATION_CONSUMER_ID_LENGTH: usize = 128;

/// Run this DDL in lib.rs's v8 migration transaction, then record schema v8.
pub const MIGRATION_SQL: &str = r#"
CREATE TABLE pomodoro_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    focus_duration_seconds INTEGER NOT NULL CHECK (focus_duration_seconds BETWEEN 60 AND 10800),
    short_break_duration_seconds INTEGER NOT NULL CHECK (short_break_duration_seconds BETWEEN 60 AND 3600),
    long_break_duration_seconds INTEGER NOT NULL CHECK (long_break_duration_seconds BETWEEN 60 AND 3600),
    long_break_interval INTEGER NOT NULL CHECK (long_break_interval BETWEEN 2 AND 12),
    updated_at_unix_ms INTEGER NOT NULL
);
CREATE TABLE pomodoro_cycle_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    completed_focuses_in_cycle INTEGER NOT NULL DEFAULT 0 CHECK (completed_focuses_in_cycle >= 0),
    recommended_phase TEXT NOT NULL CHECK (recommended_phase IN ('focus', 'shortBreak', 'longBreak')),
    updated_at_unix_ms INTEGER NOT NULL
);
CREATE TABLE pomodoro_sessions (
    id INTEGER PRIMARY KEY,
    task_id INTEGER REFERENCES tasks(id) ON DELETE SET NULL,
    task_title_snapshot TEXT,
    phase TEXT NOT NULL CHECK (phase IN ('focus', 'shortBreak', 'longBreak')),
    status TEXT NOT NULL CHECK (status IN ('running', 'paused', 'completed', 'skipped', 'cancelled')),
    planned_duration_seconds INTEGER NOT NULL CHECK (planned_duration_seconds BETWEEN 1 AND 10800),
    paused_remaining_seconds INTEGER,
    started_at_unix_ms INTEGER,
    target_ends_at_unix_ms INTEGER,
    paused_at_unix_ms INTEGER,
    ended_at_unix_ms INTEGER,
    notification_tag TEXT,
    notification_token_hash BLOB,
    notification_activated_at_unix_ms INTEGER,
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    CHECK ((status = 'running' AND started_at_unix_ms IS NOT NULL AND target_ends_at_unix_ms IS NOT NULL
            AND paused_remaining_seconds IS NULL AND paused_at_unix_ms IS NULL AND ended_at_unix_ms IS NULL)
        OR (status = 'paused' AND paused_remaining_seconds IS NOT NULL AND paused_remaining_seconds > 0
            AND paused_at_unix_ms IS NOT NULL AND started_at_unix_ms IS NOT NULL
            AND target_ends_at_unix_ms IS NULL AND ended_at_unix_ms IS NULL)
        OR (status IN ('completed', 'skipped', 'cancelled') AND ended_at_unix_ms IS NOT NULL))
);
CREATE UNIQUE INDEX idx_pomodoro_one_active_session
    ON pomodoro_sessions ((1)) WHERE status IN ('running', 'paused');
CREATE INDEX idx_pomodoro_sessions_task_completed
    ON pomodoro_sessions (task_id, phase, status, ended_at_unix_ms);
CREATE INDEX idx_pomodoro_sessions_completed_date
    ON pomodoro_sessions (phase, status, ended_at_unix_ms);
CREATE INDEX idx_pomodoro_sessions_notification_tag
    ON pomodoro_sessions (notification_tag) WHERE notification_tag IS NOT NULL;
"#;

pub const ACTIVATION_INBOX_MIGRATION_SQL: &str = r#"
CREATE TABLE pomodoro_activation_inbox (
    id INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL UNIQUE REFERENCES pomodoro_sessions(id) ON DELETE RESTRICT,
    received_at_unix_ms INTEGER NOT NULL,
    claimed_by TEXT,
    claim_expires_at_unix_ms INTEGER,
    acknowledged_at_unix_ms INTEGER
);
CREATE INDEX idx_pomodoro_activation_inbox_claimable
    ON pomodoro_activation_inbox (
        acknowledged_at_unix_ms,
        claim_expires_at_unix_ms,
        received_at_unix_ms,
        id
    );
"#;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum PomodoroPhase {
    #[default]
    Focus,
    ShortBreak,
    LongBreak,
}

impl PomodoroPhase {
    fn as_db(self) -> &'static str {
        match self {
            Self::Focus => "focus",
            Self::ShortBreak => "shortBreak",
            Self::LongBreak => "longBreak",
        }
    }

    fn from_db(value: String) -> rusqlite::Result<Self> {
        match value.as_str() {
            "focus" => Ok(Self::Focus),
            "shortBreak" => Ok(Self::ShortBreak),
            "longBreak" => Ok(Self::LongBreak),
            _ => invalid_enum("pomodoro phase", value),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PomodoroStatus {
    Running,
    Paused,
    Completed,
    Skipped,
    Cancelled,
}

impl PomodoroStatus {
    fn as_db(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Skipped => "skipped",
            Self::Cancelled => "cancelled",
        }
    }

    fn from_db(value: String) -> rusqlite::Result<Self> {
        match value.as_str() {
            "running" => Ok(Self::Running),
            "paused" => Ok(Self::Paused),
            "completed" => Ok(Self::Completed),
            "skipped" => Ok(Self::Skipped),
            "cancelled" => Ok(Self::Cancelled),
            _ => invalid_enum("pomodoro status", value),
        }
    }
}

fn invalid_enum<T>(name: &str, value: String) -> rusqlite::Result<T> {
    Err(rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        format!("{name} is invalid: {value}").into(),
    ))
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PomodoroSettings {
    pub focus_minutes: i64,
    pub short_break_minutes: i64,
    pub long_break_minutes: i64,
    pub long_break_interval: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePomodoroSettingsInput {
    pub focus_minutes: i64,
    pub short_break_minutes: i64,
    pub long_break_minutes: i64,
    pub long_break_interval: i64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartPomodoroInput {
    pub phase: PomodoroPhase,
    #[serde(default)]
    pub task_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PomodoroSession {
    pub id: i64,
    pub task_id: Option<i64>,
    pub task_title_snapshot: Option<String>,
    pub phase: PomodoroPhase,
    pub status: PomodoroStatus,
    pub planned_duration_seconds: i64,
    pub paused_remaining_seconds: Option<i64>,
    pub started_at_unix_ms: Option<i64>,
    pub target_ends_at_unix_ms: Option<i64>,
    pub paused_at_unix_ms: Option<i64>,
    pub ended_at_unix_ms: Option<i64>,
    pub notification_tag: Option<String>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PomodoroTaskSummary {
    pub task_id: i64,
    pub title: String,
    pub completed_focus_count: i64,
    pub completed_focus_today_count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PomodoroSnapshot {
    pub settings: PomodoroSettings,
    pub current_session: Option<PomodoroSession>,
    pub completed_focuses_in_cycle: i64,
    pub recommended_phase: PomodoroPhase,
    pub completed_focus_today_count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PomodoroView {
    pub snapshot: PomodoroSnapshot,
    pub task_summaries: Vec<PomodoroTaskSummary>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PomodoroMutationResult {
    pub snapshot: PomodoroSnapshot,
    pub session: Option<PomodoroSession>,
    /// Native integration cancels this tag after a terminal/pause transition.
    pub notification_tag_to_cancel: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PomodoroNotificationActivation {
    pub session_id: i64,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PomodoroNotificationSpec {
    pub session_id: i64,
    pub tag: String,
    pub activation_uri: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PendingPomodoroActivation {
    pub id: i64,
    pub session_id: i64,
    pub received_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PomodoroActivationAckResult {
    pub acknowledged_ids: Vec<i64>,
}

const SESSION_COLUMNS: &str = "id, task_id, task_title_snapshot, phase, status, planned_duration_seconds, paused_remaining_seconds, started_at_unix_ms, target_ends_at_unix_ms, paused_at_unix_ms, ended_at_unix_ms, notification_tag, created_at_unix_ms, updated_at_unix_ms";

fn database_error(_: rusqlite::Error) -> String {
    "database operation failed".to_string()
}

fn validate_settings(input: &UpdatePomodoroSettingsInput) -> Result<(), String> {
    if !(1..=180).contains(&input.focus_minutes) {
        return Err("focusMinutes must be between 1 and 180".to_string());
    }
    if !(1..=60).contains(&input.short_break_minutes) {
        return Err("shortBreakMinutes must be between 1 and 60".to_string());
    }
    if !(1..=60).contains(&input.long_break_minutes) {
        return Err("longBreakMinutes must be between 1 and 60".to_string());
    }
    if !(2..=12).contains(&input.long_break_interval) {
        return Err("longBreakInterval must be between 2 and 12".to_string());
    }
    Ok(())
}

fn default_settings(now: i64) -> PomodoroSettings {
    PomodoroSettings {
        focus_minutes: DEFAULT_FOCUS_MINUTES,
        short_break_minutes: DEFAULT_SHORT_BREAK_MINUTES,
        long_break_minutes: DEFAULT_LONG_BREAK_MINUTES,
        long_break_interval: DEFAULT_LONG_BREAK_INTERVAL,
        updated_at_unix_ms: now,
    }
}

fn ensure_singletons(connection: &Connection, now: i64) -> Result<(), String> {
    let defaults = default_settings(now);
    connection.execute(
        "INSERT OR IGNORE INTO pomodoro_settings (id, focus_duration_seconds, short_break_duration_seconds, long_break_duration_seconds, long_break_interval, updated_at_unix_ms) VALUES (1, ?1, ?2, ?3, ?4, ?5)",
        params![defaults.focus_minutes * 60, defaults.short_break_minutes * 60, defaults.long_break_minutes * 60, defaults.long_break_interval, now],
    ).map_err(database_error)?;
    connection.execute(
        "INSERT OR IGNORE INTO pomodoro_cycle_state (id, completed_focuses_in_cycle, recommended_phase, updated_at_unix_ms) VALUES (1, 0, 'focus', ?1)",
        [now],
    ).map_err(database_error)?;
    Ok(())
}

fn settings_from_row(row: &Row<'_>) -> rusqlite::Result<PomodoroSettings> {
    Ok(PomodoroSettings {
        focus_minutes: row.get::<_, i64>(0)? / 60,
        short_break_minutes: row.get::<_, i64>(1)? / 60,
        long_break_minutes: row.get::<_, i64>(2)? / 60,
        long_break_interval: row.get(3)?,
        updated_at_unix_ms: row.get(4)?,
    })
}

fn settings(connection: &Connection) -> Result<PomodoroSettings, String> {
    connection
        .query_row(
            "SELECT focus_duration_seconds, short_break_duration_seconds, long_break_duration_seconds, long_break_interval, updated_at_unix_ms FROM pomodoro_settings WHERE id = 1",
            [],
            settings_from_row,
        )
        .optional()
        .map_err(database_error)?
        .ok_or_else(|| format!("pomodoro settings are missing; apply migration v{POMODORO_MIGRATION_VERSION} before use"))
}

fn session_from_row(row: &Row<'_>) -> rusqlite::Result<PomodoroSession> {
    Ok(PomodoroSession {
        id: row.get("id")?,
        task_id: row.get("task_id")?,
        task_title_snapshot: row.get("task_title_snapshot")?,
        phase: PomodoroPhase::from_db(row.get("phase")?)?,
        status: PomodoroStatus::from_db(row.get("status")?)?,
        planned_duration_seconds: row.get("planned_duration_seconds")?,
        paused_remaining_seconds: row.get("paused_remaining_seconds")?,
        started_at_unix_ms: row.get("started_at_unix_ms")?,
        target_ends_at_unix_ms: row.get("target_ends_at_unix_ms")?,
        paused_at_unix_ms: row.get("paused_at_unix_ms")?,
        ended_at_unix_ms: row.get("ended_at_unix_ms")?,
        notification_tag: row.get("notification_tag")?,
        created_at_unix_ms: row.get("created_at_unix_ms")?,
        updated_at_unix_ms: row.get("updated_at_unix_ms")?,
    })
}

fn active_session(connection: &Connection) -> Result<Option<PomodoroSession>, String> {
    connection
        .query_row(
            &format!("SELECT {SESSION_COLUMNS} FROM pomodoro_sessions WHERE status IN ('running', 'paused') LIMIT 1"),
            [],
            session_from_row,
        )
        .optional()
        .map_err(database_error)
}

fn session(connection: &Connection, id: i64) -> Result<PomodoroSession, String> {
    connection
        .query_row(
            &format!("SELECT {SESSION_COLUMNS} FROM pomodoro_sessions WHERE id = ?1"),
            [id],
            session_from_row,
        )
        .optional()
        .map_err(database_error)?
        .ok_or_else(|| "pomodoro session not found".to_string())
}

fn phase_duration_seconds(settings: &PomodoroSettings, phase: PomodoroPhase) -> i64 {
    match phase {
        PomodoroPhase::Focus => settings.focus_minutes * 60,
        PomodoroPhase::ShortBreak => settings.short_break_minutes * 60,
        PomodoroPhase::LongBreak => settings.long_break_minutes * 60,
    }
}

fn local_day_start_unix_ms(now: i64) -> Result<i64, String> {
    let local_now = Local
        .timestamp_millis_opt(now)
        .single()
        .ok_or_else(|| "current timestamp is outside the supported local date range".to_string())?;
    let midnight = local_now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "local day boundary is invalid".to_string())?;
    for minute in 0..=180 {
        let candidate = midnight + ChronoDuration::minutes(minute);
        if let Some(value) = Local.from_local_datetime(&candidate).earliest() {
            return Ok(value.timestamp_millis());
        }
    }
    Err("local day boundary could not be resolved".to_string())
}

fn task_title_for_binding(
    connection: &Connection,
    task_id: Option<i64>,
) -> Result<Option<String>, String> {
    let Some(task_id) = task_id else {
        return Ok(None);
    };
    if task_id <= 0 {
        return Err("taskId must be greater than 0".to_string());
    }
    connection
        .query_row(
            "SELECT title FROM tasks WHERE id = ?1 AND deleted_at_unix_ms IS NULL AND completed_at_unix_ms IS NULL",
            [task_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(database_error)?
        .ok_or_else(|| "task is not available for pomodoro binding".to_string())
        .map(Some)
}

fn advance_cycle(
    transaction: &Transaction<'_>,
    phase: PomodoroPhase,
    natural: bool,
    now: i64,
) -> Result<(), String> {
    let (current, interval): (i64, i64) = transaction
        .query_row(
            "SELECT c.completed_focuses_in_cycle, s.long_break_interval FROM pomodoro_cycle_state c JOIN pomodoro_settings s ON s.id = 1 WHERE c.id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(database_error)?;
    let (next_count, recommended) = match phase {
        PomodoroPhase::Focus if natural => {
            let next = current.saturating_add(1);
            (
                next,
                if next >= interval {
                    PomodoroPhase::LongBreak
                } else {
                    PomodoroPhase::ShortBreak
                },
            )
        }
        PomodoroPhase::Focus => (current, PomodoroPhase::ShortBreak),
        PomodoroPhase::ShortBreak => (current, PomodoroPhase::Focus),
        PomodoroPhase::LongBreak => (0, PomodoroPhase::Focus),
    };
    transaction
        .execute(
            "UPDATE pomodoro_cycle_state SET completed_focuses_in_cycle = ?1, recommended_phase = ?2, updated_at_unix_ms = ?3 WHERE id = 1",
            params![next_count, recommended.as_db(), now],
        )
        .map_err(database_error)?;
    Ok(())
}

/// Idempotently completes an expired running session. `Some` means this call
/// performed the state transition; repeated recovery calls return `None`.
pub fn settle_at(connection: &mut Connection, now: i64) -> Result<Option<PomodoroSession>, String> {
    ensure_singletons(connection, now)?;
    let transaction = connection.transaction().map_err(database_error)?;
    let expired = transaction
        .query_row(
            &format!("SELECT {SESSION_COLUMNS} FROM pomodoro_sessions WHERE status = 'running' AND target_ends_at_unix_ms <= ?1 ORDER BY id LIMIT 1"),
            [now],
            session_from_row,
        )
        .optional()
        .map_err(database_error)?;
    let Some(expired) = expired else {
        transaction.commit().map_err(database_error)?;
        return Ok(None);
    };
    let ended_at_unix_ms = expired
        .target_ends_at_unix_ms
        .expect("expired running session target invariant");
    let changed = transaction
        .execute(
            "UPDATE pomodoro_sessions SET status = 'completed', ended_at_unix_ms = ?1, updated_at_unix_ms = ?2 WHERE id = ?3 AND status = 'running' AND target_ends_at_unix_ms <= ?2",
            params![ended_at_unix_ms, now, expired.id],
        )
        .map_err(database_error)?;
    if changed == 1 {
        advance_cycle(&transaction, expired.phase, true, now)?;
    }
    transaction.commit().map_err(database_error)?;
    Ok((changed == 1).then(|| PomodoroSession {
        status: PomodoroStatus::Completed,
        ended_at_unix_ms: Some(ended_at_unix_ms),
        updated_at_unix_ms: now,
        ..expired
    }))
}

fn snapshot_at(connection: &Connection, now: i64) -> Result<PomodoroSnapshot, String> {
    let settings = settings(connection)?;
    let (completed_focuses_in_cycle, recommended_phase) = connection
        .query_row(
            "SELECT completed_focuses_in_cycle, recommended_phase FROM pomodoro_cycle_state WHERE id = 1",
            [],
            |row| Ok((row.get::<_, i64>(0)?, PomodoroPhase::from_db(row.get(1)?)?)),
        )
        .map_err(database_error)?;
    let day_start = local_day_start_unix_ms(now)?;
    let completed_focus_today_count = connection
        .query_row(
            "SELECT COUNT(*) FROM pomodoro_sessions WHERE phase = 'focus' AND status = 'completed' AND ended_at_unix_ms >= ?1 AND ended_at_unix_ms <= ?2",
            params![day_start, now],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    Ok(PomodoroSnapshot {
        settings,
        current_session: active_session(connection)?,
        completed_focuses_in_cycle,
        recommended_phase,
        completed_focus_today_count,
    })
}

pub fn get_at(connection: &mut Connection, now: i64) -> Result<PomodoroSnapshot, String> {
    ensure_singletons(connection, now)?;
    settle_at(connection, now)?;
    snapshot_at(connection, now)
}

pub fn update_settings_at(
    connection: &mut Connection,
    input: UpdatePomodoroSettingsInput,
    now: i64,
) -> Result<PomodoroMutationResult, String> {
    validate_settings(&input)?;
    ensure_singletons(connection, now)?;
    settle_at(connection, now)?;
    connection
        .execute(
            "UPDATE pomodoro_settings SET focus_duration_seconds = ?1, short_break_duration_seconds = ?2, long_break_duration_seconds = ?3, long_break_interval = ?4, updated_at_unix_ms = ?5 WHERE id = 1",
            params![input.focus_minutes * 60, input.short_break_minutes * 60, input.long_break_minutes * 60, input.long_break_interval, now],
        )
        .map_err(database_error)?;
    let snapshot = snapshot_at(connection, now)?;
    Ok(PomodoroMutationResult {
        session: snapshot.current_session.clone(),
        snapshot,
        notification_tag_to_cancel: None,
    })
}

pub fn start_at(
    connection: &mut Connection,
    input: StartPomodoroInput,
    now: i64,
) -> Result<PomodoroMutationResult, String> {
    ensure_singletons(connection, now)?;
    settle_at(connection, now)?;
    if active_session(connection)?.is_some() {
        return Err("a pomodoro session is already active".to_string());
    }
    if input.phase != PomodoroPhase::Focus && input.task_id.is_some() {
        return Err("only focus sessions may bind a task".to_string());
    }
    let task_title_snapshot = task_title_for_binding(connection, input.task_id)?;
    let duration = phase_duration_seconds(&settings(connection)?, input.phase);
    let transaction = connection.transaction().map_err(database_error)?;
    transaction
        .execute(
            "INSERT INTO pomodoro_sessions (task_id, task_title_snapshot, phase, status, planned_duration_seconds, paused_remaining_seconds, started_at_unix_ms, target_ends_at_unix_ms, paused_at_unix_ms, ended_at_unix_ms, notification_tag, notification_token_hash, notification_activated_at_unix_ms, created_at_unix_ms, updated_at_unix_ms) VALUES (?1, ?2, ?3, 'running', ?4, NULL, ?5, ?6, NULL, NULL, NULL, NULL, NULL, ?5, ?5)",
            params![input.task_id, task_title_snapshot, input.phase.as_db(), duration, now, now.saturating_add(duration.saturating_mul(1000))],
        )
        .map_err(database_error)?;
    let id = transaction.last_insert_rowid();
    transaction.commit().map_err(database_error)?;
    let session = session(connection, id)?;
    let snapshot = snapshot_at(connection, now)?;
    Ok(PomodoroMutationResult {
        snapshot,
        session: Some(session),
        notification_tag_to_cancel: None,
    })
}

pub fn pause_at(connection: &mut Connection, now: i64) -> Result<PomodoroMutationResult, String> {
    ensure_singletons(connection, now)?;
    settle_at(connection, now)?;
    let current =
        active_session(connection)?.ok_or_else(|| "no active pomodoro session".to_string())?;
    if current.status != PomodoroStatus::Running {
        return Err("pomodoro session is not running".to_string());
    }
    let target = current
        .target_ends_at_unix_ms
        .expect("running session target invariant");
    let remaining = ((target.saturating_sub(now)).saturating_add(999) / 1000).max(1);
    connection
        .execute(
            "UPDATE pomodoro_sessions SET status = 'paused', paused_remaining_seconds = ?1, target_ends_at_unix_ms = NULL, paused_at_unix_ms = ?2, notification_token_hash = NULL, updated_at_unix_ms = ?2 WHERE id = ?3 AND status = 'running'",
            params![remaining, now, current.id],
        )
        .map_err(database_error)?;
    let session = session(connection, current.id)?;
    let snapshot = snapshot_at(connection, now)?;
    Ok(PomodoroMutationResult {
        snapshot,
        session: Some(session),
        notification_tag_to_cancel: current.notification_tag,
    })
}

pub fn resume_at(connection: &mut Connection, now: i64) -> Result<PomodoroMutationResult, String> {
    ensure_singletons(connection, now)?;
    settle_at(connection, now)?;
    let current =
        active_session(connection)?.ok_or_else(|| "no active pomodoro session".to_string())?;
    if current.status != PomodoroStatus::Paused {
        return Err("pomodoro session is not paused".to_string());
    }
    let remaining = current
        .paused_remaining_seconds
        .expect("paused session remaining invariant");
    connection
        .execute(
            "UPDATE pomodoro_sessions SET status = 'running', paused_remaining_seconds = NULL, target_ends_at_unix_ms = ?1, paused_at_unix_ms = NULL, notification_token_hash = NULL, notification_activated_at_unix_ms = NULL, updated_at_unix_ms = ?2 WHERE id = ?3 AND status = 'paused'",
            params![now.saturating_add(remaining.saturating_mul(1000)), now, current.id],
        )
        .map_err(database_error)?;
    let session = session(connection, current.id)?;
    let snapshot = snapshot_at(connection, now)?;
    Ok(PomodoroMutationResult {
        snapshot,
        session: Some(session),
        notification_tag_to_cancel: current.notification_tag,
    })
}

fn terminal_at(
    connection: &mut Connection,
    status: PomodoroStatus,
    now: i64,
) -> Result<PomodoroMutationResult, String> {
    ensure_singletons(connection, now)?;
    settle_at(connection, now)?;
    let current =
        active_session(connection)?.ok_or_else(|| "no active pomodoro session".to_string())?;
    let transaction = connection.transaction().map_err(database_error)?;
    transaction
        .execute(
            "UPDATE pomodoro_sessions SET status = ?1, ended_at_unix_ms = ?2, target_ends_at_unix_ms = NULL, paused_remaining_seconds = NULL, notification_token_hash = NULL, updated_at_unix_ms = ?2 WHERE id = ?3 AND status IN ('running', 'paused')",
            params![status.as_db(), now, current.id],
        )
        .map_err(database_error)?;
    if status == PomodoroStatus::Skipped {
        advance_cycle(&transaction, current.phase, false, now)?;
    } else if status == PomodoroStatus::Cancelled {
        transaction
            .execute(
                "UPDATE pomodoro_cycle_state SET recommended_phase = ?1, updated_at_unix_ms = ?2 WHERE id = 1",
                params![current.phase.as_db(), now],
            )
            .map_err(database_error)?;
    }
    transaction.commit().map_err(database_error)?;
    let session = session(connection, current.id)?;
    let snapshot = snapshot_at(connection, now)?;
    Ok(PomodoroMutationResult {
        snapshot,
        session: Some(session),
        notification_tag_to_cancel: current.notification_tag,
    })
}

pub fn skip_at(connection: &mut Connection, now: i64) -> Result<PomodoroMutationResult, String> {
    terminal_at(connection, PomodoroStatus::Skipped, now)
}

pub fn reset_at(connection: &mut Connection, now: i64) -> Result<PomodoroMutationResult, String> {
    terminal_at(connection, PomodoroStatus::Cancelled, now)
}

fn task_summaries_at(
    connection: &Connection,
    now: i64,
) -> Result<Vec<PomodoroTaskSummary>, String> {
    let day_start = local_day_start_unix_ms(now)?;
    let mut statement = connection
        .prepare(
            "SELECT t.id, t.title, COUNT(s.id), COALESCE(SUM(CASE WHEN s.ended_at_unix_ms >= ?1 AND s.ended_at_unix_ms <= ?2 THEN 1 ELSE 0 END), 0) FROM tasks t LEFT JOIN pomodoro_sessions s ON s.task_id = t.id AND s.phase = 'focus' AND s.status = 'completed' WHERE t.deleted_at_unix_ms IS NULL AND t.completed_at_unix_ms IS NULL GROUP BY t.id, t.title ORDER BY t.id",
        )
        .map_err(database_error)?;
    let summaries = statement
        .query_map(params![day_start, now], |row| {
            Ok(PomodoroTaskSummary {
                task_id: row.get(0)?,
                title: row.get(1)?,
                completed_focus_count: row.get(2)?,
                completed_focus_today_count: row.get(3)?,
            })
        })
        .map_err(database_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(database_error)?;
    Ok(summaries)
}

fn reconciled_view_at(connection: &Connection, now: i64) -> Result<PomodoroView, String> {
    Ok(PomodoroView {
        snapshot: snapshot_at(connection, now)?,
        task_summaries: task_summaries_at(connection, now)?,
    })
}

pub fn view_at(connection: &mut Connection, now: i64) -> Result<PomodoroView, String> {
    ensure_singletons(connection, now)?;
    settle_at(connection, now)?;
    reconciled_view_at(connection, now)
}

pub fn list_task_summaries_at(
    connection: &mut Connection,
    now: i64,
) -> Result<Vec<PomodoroTaskSummary>, String> {
    ensure_singletons(connection, now)?;
    settle_at(connection, now)?;
    task_summaries_at(connection, now)
}

pub fn notification_tag(session_id: i64) -> Result<String, String> {
    if session_id <= 0 {
        return Err("pomodoro session id must be greater than 0".to_string());
    }
    Ok(format!("pomodoro-session-{session_id}"))
}

fn is_ascii_identifier(value: &str, max_length: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn is_base64url_token(value: &str) -> bool {
    (16..=256).contains(&value.len()) && value.len() % 4 != 1 && is_ascii_identifier(value, 256)
}

fn validate_activation_consumer_id(consumer_id: &str) -> Result<(), String> {
    if is_ascii_identifier(consumer_id, MAX_ACTIVATION_CONSUMER_ID_LENGTH) {
        Ok(())
    } else {
        Err("pomodoro activation consumer id is invalid".to_string())
    }
}

fn canonical_positive_id(value: &str) -> Option<i64> {
    if value.is_empty()
        || value.len() > 19
        || value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    value.parse::<i64>().ok().filter(|id| *id > 0)
}

/// Creates the persisted native-notification capability. The caller generates
/// `token` securely; this module stores only its SHA-256 hash.
pub fn prepare_notification_at(
    connection: &mut Connection,
    session_id: i64,
    token: &str,
    now: i64,
) -> Result<PomodoroNotificationSpec, String> {
    if !is_base64url_token(token) {
        return Err("pomodoro notification token is invalid".to_string());
    }
    let current = session(connection, session_id)?;
    if current.status != PomodoroStatus::Running {
        return Err("only running pomodoro sessions can schedule notifications".to_string());
    }
    let tag = notification_tag(session_id)?;
    connection
        .execute(
            "UPDATE pomodoro_sessions SET notification_tag = ?1, notification_token_hash = ?2, notification_activated_at_unix_ms = NULL, updated_at_unix_ms = ?3 WHERE id = ?4 AND status = 'running'",
            params![tag, Sha256::digest(token.as_bytes()).as_slice(), now, session_id],
        )
        .map_err(database_error)?;
    Ok(PomodoroNotificationSpec {
        session_id,
        tag,
        activation_uri: format!("startodo://focus/open?session={session_id}&token={token}"),
    })
}

pub fn generate_notification_token() -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("secure random generation failed: {error}"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub fn parse_notification_activation(url: &Url) -> Option<PomodoroNotificationActivation> {
    if url.scheme() != "startodo"
        || url.host_str() != Some("focus")
        || url.path() != "/open"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let mut session_id = None;
    let mut token = None;
    for part in url.query()?.split('&') {
        let (key, value) = part.split_once('=')?;
        if value.contains('=') {
            return None;
        }
        match key {
            "session" if session_id.is_none() => session_id = canonical_positive_id(value),
            "token" if token.is_none() => token = Some(value),
            _ => return None,
        }
    }
    let session_id = session_id?;
    let token = token?;
    is_base64url_token(token).then(|| PomodoroNotificationActivation {
        session_id,
        token: token.to_string(),
    })
}

pub fn scheduled_notification_matches_running_session(
    connection: &Connection,
    session_id: i64,
    tag: &str,
    due_at_unix_ms: i64,
    activation_uri: &str,
) -> Result<bool, String> {
    let Some(activation) = Url::parse(activation_uri)
        .ok()
        .and_then(|url| parse_notification_activation(&url))
    else {
        return Ok(false);
    };
    if activation.session_id != session_id || tag != notification_tag(session_id)? {
        return Ok(false);
    }
    let stored = connection
        .query_row(
            "SELECT status, target_ends_at_unix_ms, notification_tag, notification_token_hash, notification_activated_at_unix_ms FROM pomodoro_sessions WHERE id = ?1",
            [session_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<Vec<u8>>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?;
    let Some((status, target, stored_tag, token_hash, activated_at)) = stored else {
        return Ok(false);
    };
    let supplied_hash = Sha256::digest(activation.token.as_bytes());
    Ok(status == PomodoroStatus::Running.as_db()
        && target == Some(due_at_unix_ms)
        && stored_tag.as_deref() == Some(tag)
        && activated_at.is_none()
        && token_hash.as_ref().is_some_and(|hash| {
            hash.len() == supplied_hash.len()
                && hash.ct_eq(supplied_hash.as_slice()).unwrap_u8() == 1
        }))
}

/// Validates a due notification capability exactly once. It deliberately does
/// not emit events or touch native notifications; lib.rs owns those effects.
pub fn consume_notification_activation_at(
    connection: &mut Connection,
    activation: &PomodoroNotificationActivation,
    now: i64,
) -> Result<Option<i64>, String> {
    let transaction = connection.transaction().map_err(database_error)?;
    let stored = transaction
        .query_row(
            "SELECT status, target_ends_at_unix_ms, notification_token_hash, notification_activated_at_unix_ms FROM pomodoro_sessions WHERE id = ?1",
            [activation.session_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?;
    let Some((status, target, token_hash, activated)) = stored else {
        transaction.commit().map_err(database_error)?;
        return Ok(None);
    };
    let supplied_hash = Sha256::digest(activation.token.as_bytes());
    let valid = matches!(status.as_str(), "running" | "completed")
        && target.is_some_and(|value| {
            value <= now && now <= value.saturating_add(POMODORO_ACTIVATION_GRACE_MS)
        })
        && activated.is_none()
        && token_hash.as_ref().is_some_and(|hash| {
            hash.len() == supplied_hash.len()
                && hash.ct_eq(supplied_hash.as_slice()).unwrap_u8() == 1
        });
    if !valid {
        transaction.commit().map_err(database_error)?;
        return Ok(None);
    }
    let changed = transaction
        .execute(
            "UPDATE pomodoro_sessions SET notification_activated_at_unix_ms = ?1 WHERE id = ?2 AND notification_activated_at_unix_ms IS NULL",
            params![now, activation.session_id],
        )
        .map_err(database_error)?;
    if changed == 1 {
        transaction
            .execute(
                "INSERT INTO pomodoro_activation_inbox (session_id, received_at_unix_ms) VALUES (?1, ?2) ON CONFLICT(session_id) DO NOTHING",
                params![activation.session_id, now],
            )
            .map_err(database_error)?;
    }
    transaction.commit().map_err(database_error)?;
    Ok((changed == 1).then_some(activation.session_id))
}

pub fn claim_pending_activations_at(
    connection: &mut Connection,
    consumer_id: &str,
    limit: usize,
    now: i64,
) -> Result<Vec<PendingPomodoroActivation>, String> {
    validate_activation_consumer_id(consumer_id)?;
    let limit = limit.clamp(1, 100) as i64;
    let transaction = connection.transaction().map_err(database_error)?;
    let candidates = {
        let mut statement = transaction
            .prepare(
                "SELECT id, session_id, received_at_unix_ms FROM pomodoro_activation_inbox WHERE acknowledged_at_unix_ms IS NULL AND (claimed_by IS NULL OR claim_expires_at_unix_ms IS NULL OR claim_expires_at_unix_ms <= ?1 OR claimed_by = ?2) ORDER BY received_at_unix_ms ASC, id ASC LIMIT ?3",
            )
            .map_err(database_error)?;
        let candidates = statement
            .query_map(params![now, consumer_id, limit], |row| {
                Ok(PendingPomodoroActivation {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    received_at_unix_ms: row.get(2)?,
                })
            })
            .map_err(database_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(database_error)?;
        candidates
    };
    let claim_expires_at_unix_ms = now.saturating_add(POMODORO_ACTIVATION_CLAIM_LEASE_MS);
    let mut claimed = Vec::new();
    for candidate in candidates {
        let changed = transaction
            .execute(
                "UPDATE pomodoro_activation_inbox SET claimed_by = ?1, claim_expires_at_unix_ms = ?2 WHERE id = ?3 AND acknowledged_at_unix_ms IS NULL AND (claimed_by IS NULL OR claim_expires_at_unix_ms IS NULL OR claim_expires_at_unix_ms <= ?4 OR claimed_by = ?1)",
                params![consumer_id, claim_expires_at_unix_ms, candidate.id, now],
            )
            .map_err(database_error)?;
        if changed == 1 {
            claimed.push(candidate);
        }
    }
    transaction.commit().map_err(database_error)?;
    Ok(claimed)
}

pub fn acknowledge_pending_activations_at(
    connection: &mut Connection,
    consumer_id: &str,
    ids: &[i64],
    now: i64,
) -> Result<PomodoroActivationAckResult, String> {
    validate_activation_consumer_id(consumer_id)?;
    let mut unique_ids = Vec::new();
    let mut seen_ids = HashSet::new();
    for id in ids.iter().copied().filter(|id| *id > 0) {
        if seen_ids.insert(id) {
            unique_ids.push(id);
        }
    }
    if unique_ids.is_empty() {
        return Ok(PomodoroActivationAckResult {
            acknowledged_ids: Vec::new(),
        });
    }
    let transaction = connection.transaction().map_err(database_error)?;
    let mut acknowledged_ids = Vec::new();
    for id in unique_ids {
        let changed = transaction
            .execute(
                "UPDATE pomodoro_activation_inbox SET acknowledged_at_unix_ms = ?1 WHERE id = ?2 AND acknowledged_at_unix_ms IS NULL AND claimed_by = ?3 AND claim_expires_at_unix_ms > ?1",
                params![now, id, consumer_id],
            )
            .map_err(database_error)?;
        if changed == 1 {
            acknowledged_ids.push(id);
        }
    }
    transaction.commit().map_err(database_error)?;
    Ok(PomodoroActivationAckResult { acknowledged_ids })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: i64 = 1_728_000_000_000;

    fn database() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON; CREATE TABLE tasks (id INTEGER PRIMARY KEY, title TEXT NOT NULL, deleted_at_unix_ms INTEGER, completed_at_unix_ms INTEGER);",
            )
            .unwrap();
        connection.execute_batch(MIGRATION_SQL).unwrap();
        connection
            .execute_batch(ACTIVATION_INBOX_MIGRATION_SQL)
            .unwrap();
        connection
    }

    fn seed(connection: &Connection, id: i64, title: &str) {
        connection
            .execute(
                "INSERT INTO tasks (id, title) VALUES (?1, ?2)",
                params![id, title],
            )
            .unwrap();
    }

    fn start_focus(connection: &mut Connection, now: i64) -> PomodoroSession {
        start_at(
            connection,
            StartPomodoroInput {
                phase: PomodoroPhase::Focus,
                task_id: None,
            },
            now,
        )
        .unwrap()
        .session
        .unwrap()
    }

    #[test]
    fn public_types_are_camel_case_and_settings_validate_boundaries() {
        let mut connection = database();
        assert_eq!(
            get_at(&mut connection, NOW).unwrap().settings.focus_minutes,
            25
        );
        assert_eq!(
            serde_json::to_value(PomodoroPhase::ShortBreak).unwrap(),
            json!("shortBreak")
        );
        assert_eq!(
            serde_json::to_value(PomodoroStatus::Completed).unwrap(),
            json!("completed")
        );
        assert_eq!(
            update_settings_at(
                &mut connection,
                UpdatePomodoroSettingsInput {
                    focus_minutes: 0,
                    short_break_minutes: 5,
                    long_break_minutes: 15,
                    long_break_interval: 4,
                },
                NOW,
            )
            .unwrap_err(),
            "focusMinutes must be between 1 and 180"
        );
        update_settings_at(
            &mut connection,
            UpdatePomodoroSettingsInput {
                focus_minutes: 180,
                short_break_minutes: 1,
                long_break_minutes: 60,
                long_break_interval: 12,
            },
            NOW,
        )
        .unwrap();
    }

    #[test]
    fn start_pause_resume_skip_and_reset_are_deterministic() {
        let mut connection = database();
        let running = start_focus(&mut connection, NOW);
        assert_eq!(running.target_ends_at_unix_ms, Some(NOW + 1_500_000));
        assert_eq!(
            pause_at(&mut connection, NOW + 501)
                .unwrap()
                .session
                .unwrap()
                .paused_remaining_seconds,
            Some(1500)
        );
        assert_eq!(
            resume_at(&mut connection, NOW + 10_000)
                .unwrap()
                .session
                .unwrap()
                .target_ends_at_unix_ms,
            Some(NOW + 1_510_000)
        );
        let skipped = skip_at(&mut connection, NOW + 10_001).unwrap();
        assert_eq!(skipped.session.unwrap().status, PomodoroStatus::Skipped);
        assert_eq!(
            skipped.snapshot.recommended_phase,
            PomodoroPhase::ShortBreak
        );
        start_at(
            &mut connection,
            StartPomodoroInput {
                phase: PomodoroPhase::ShortBreak,
                task_id: None,
            },
            NOW + 10_002,
        )
        .unwrap();
        reset_at(&mut connection, NOW + 10_003).unwrap();
        assert_eq!(
            get_at(&mut connection, NOW + 10_003)
                .unwrap()
                .recommended_phase,
            PomodoroPhase::ShortBreak
        );
        start_at(
            &mut connection,
            StartPomodoroInput {
                phase: PomodoroPhase::Focus,
                task_id: None,
            },
            NOW + 10_004,
        )
        .unwrap();
        reset_at(&mut connection, NOW + 10_005).unwrap();
        assert_eq!(
            get_at(&mut connection, NOW + 10_005)
                .unwrap()
                .recommended_phase,
            PomodoroPhase::Focus
        );
    }

    #[test]
    fn natural_focus_completion_is_idempotent_and_drives_cycle_recommendations() {
        let mut connection = database();
        update_settings_at(
            &mut connection,
            UpdatePomodoroSettingsInput {
                focus_minutes: 1,
                short_break_minutes: 1,
                long_break_minutes: 1,
                long_break_interval: 2,
            },
            NOW,
        )
        .unwrap();
        for number in 1..=2 {
            let started = start_focus(&mut connection, NOW + number * 100_000);
            let due = started.target_ends_at_unix_ms.unwrap();
            assert!(settle_at(&mut connection, due).unwrap().is_some());
            assert!(settle_at(&mut connection, due + 1).unwrap().is_none());
            let snapshot = get_at(&mut connection, due + 1).unwrap();
            assert_eq!(snapshot.completed_focuses_in_cycle, number);
            assert_eq!(
                snapshot.recommended_phase,
                if number == 2 {
                    PomodoroPhase::LongBreak
                } else {
                    PomodoroPhase::ShortBreak
                }
            );
        }
        start_at(
            &mut connection,
            StartPomodoroInput {
                phase: PomodoroPhase::LongBreak,
                task_id: None,
            },
            NOW + 300_000,
        )
        .unwrap();
        skip_at(&mut connection, NOW + 300_001).unwrap();
        assert_eq!(
            (
                get_at(&mut connection, NOW + 300_001)
                    .unwrap()
                    .completed_focuses_in_cycle,
                get_at(&mut connection, NOW + 300_001)
                    .unwrap()
                    .recommended_phase
            ),
            (0, PomodoroPhase::Focus)
        );
    }

    #[test]
    fn delayed_settlement_preserves_target_time_and_local_day_attribution() {
        let mut connection = database();
        let local_day_start = local_day_start_unix_ms(NOW).unwrap();
        let started_at = local_day_start - 90_000;
        update_settings_at(
            &mut connection,
            UpdatePomodoroSettingsInput {
                focus_minutes: 1,
                short_break_minutes: 5,
                long_break_minutes: 15,
                long_break_interval: 4,
            },
            started_at,
        )
        .unwrap();
        let started = start_focus(&mut connection, started_at);
        let target = started.target_ends_at_unix_ms.unwrap();
        assert!(target < local_day_start);

        let settled = settle_at(&mut connection, local_day_start + 30_000)
            .unwrap()
            .unwrap();
        assert_eq!(settled.ended_at_unix_ms, Some(target));
        assert_eq!(settled.updated_at_unix_ms, local_day_start + 30_000);
        assert_eq!(
            get_at(&mut connection, local_day_start + 30_000)
                .unwrap()
                .completed_focus_today_count,
            0
        );
    }

    #[test]
    fn aggregated_view_settles_snapshot_and_task_summary_at_the_same_time() {
        let mut connection = database();
        seed(&connection, 7, "Write migration");
        update_settings_at(
            &mut connection,
            UpdatePomodoroSettingsInput {
                focus_minutes: 1,
                short_break_minutes: 5,
                long_break_minutes: 15,
                long_break_interval: 4,
            },
            NOW,
        )
        .unwrap();
        let started = start_at(
            &mut connection,
            StartPomodoroInput {
                phase: PomodoroPhase::Focus,
                task_id: Some(7),
            },
            NOW,
        )
        .unwrap()
        .session
        .unwrap();
        let due = started.target_ends_at_unix_ms.unwrap();

        let view = view_at(&mut connection, due).unwrap();

        assert_eq!(view.snapshot.current_session, None);
        assert_eq!(view.snapshot.completed_focus_today_count, 1);
        assert_eq!(view.task_summaries.len(), 1);
        assert_eq!(view.task_summaries[0].task_id, 7);
        assert_eq!(view.task_summaries[0].completed_focus_count, 1);
        assert_eq!(view.task_summaries[0].completed_focus_today_count, 1);
    }

    #[test]
    fn task_binding_counts_only_natural_focus_and_history_survives_permanent_delete() {
        let mut connection = database();
        seed(&connection, 7, "Write migration");
        let started = start_at(
            &mut connection,
            StartPomodoroInput {
                phase: PomodoroPhase::Focus,
                task_id: Some(7),
            },
            NOW,
        )
        .unwrap()
        .session
        .unwrap();
        let due = started.target_ends_at_unix_ms.unwrap();
        settle_at(&mut connection, due).unwrap();
        assert_eq!(
            list_task_summaries_at(&mut connection, due).unwrap()[0].completed_focus_count,
            1
        );
        connection
            .execute("DELETE FROM tasks WHERE id = 7", [])
            .unwrap();
        let retained = session(&connection, started.id).unwrap();
        assert_eq!(retained.task_id, None);
        assert_eq!(
            retained.task_title_snapshot.as_deref(),
            Some("Write migration")
        );
        assert!(start_at(
            &mut connection,
            StartPomodoroInput {
                phase: PomodoroPhase::Focus,
                task_id: Some(7)
            },
            due + 1
        )
        .is_err());
    }

    #[test]
    fn task_summaries_only_include_bindable_tasks() {
        let mut connection = database();
        seed(&connection, 1, "available");
        connection
            .execute(
                "INSERT INTO tasks (id, title, completed_at_unix_ms) VALUES (2, 'done', 1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO tasks (id, title, deleted_at_unix_ms) VALUES (3, 'deleted', 1)",
                [],
            )
            .unwrap();

        assert_eq!(
            list_task_summaries_at(&mut connection, NOW)
                .unwrap()
                .into_iter()
                .map(|summary| summary.task_id)
                .collect::<Vec<_>>(),
            vec![1]
        );

        connection
            .execute(
                "UPDATE tasks SET completed_at_unix_ms = NULL WHERE id = 2",
                [],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE tasks SET deleted_at_unix_ms = NULL WHERE id = 3",
                [],
            )
            .unwrap();
        assert_eq!(
            list_task_summaries_at(&mut connection, NOW)
                .unwrap()
                .into_iter()
                .map(|summary| summary.task_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn active_binding_and_unique_active_session_are_enforced() {
        let mut connection = database();
        seed(&connection, 1, "available");
        connection
            .execute(
                "INSERT INTO tasks (id, title, completed_at_unix_ms) VALUES (2, 'done', 1)",
                [],
            )
            .unwrap();
        assert_eq!(
            start_at(
                &mut connection,
                StartPomodoroInput {
                    phase: PomodoroPhase::Focus,
                    task_id: Some(2)
                },
                NOW
            )
            .unwrap_err(),
            "task is not available for pomodoro binding"
        );
        start_at(
            &mut connection,
            StartPomodoroInput {
                phase: PomodoroPhase::Focus,
                task_id: Some(1),
            },
            NOW,
        )
        .unwrap();
        assert_eq!(
            start_at(
                &mut connection,
                StartPomodoroInput {
                    phase: PomodoroPhase::Focus,
                    task_id: None
                },
                NOW
            )
            .unwrap_err(),
            "a pomodoro session is already active"
        );
    }

    #[test]
    fn activation_inbox_claims_expire_and_only_the_current_consumer_can_acknowledge() {
        let mut connection = database();
        let started = start_focus(&mut connection, NOW);
        let spec =
            prepare_notification_at(&mut connection, started.id, "AAAAAAAAAAAAAAAA", NOW + 1)
                .unwrap();
        let activation =
            parse_notification_activation(&Url::parse(&spec.activation_uri).unwrap()).unwrap();
        let due = started.target_ends_at_unix_ms.unwrap();
        settle_at(&mut connection, due).unwrap();
        consume_notification_activation_at(&mut connection, &activation, due).unwrap();

        let first = claim_pending_activations_at(&mut connection, "consumer_one", 1, due)
            .unwrap()
            .remove(0);
        assert_eq!(first.session_id, started.id);
        assert!(
            claim_pending_activations_at(&mut connection, "consumer_two", 1, due)
                .unwrap()
                .is_empty()
        );
        assert!(acknowledge_pending_activations_at(
            &mut connection,
            "consumer_two",
            &[first.id],
            due + 1
        )
        .unwrap()
        .acknowledged_ids
        .is_empty());

        let reclaimed = claim_pending_activations_at(
            &mut connection,
            "consumer_two",
            1,
            due + POMODORO_ACTIVATION_CLAIM_LEASE_MS,
        )
        .unwrap()
        .remove(0);
        assert_eq!(reclaimed.id, first.id);
        assert!(acknowledge_pending_activations_at(
            &mut connection,
            "consumer_one",
            &[first.id],
            due + POMODORO_ACTIVATION_CLAIM_LEASE_MS + 1
        )
        .unwrap()
        .acknowledged_ids
        .is_empty());
        assert_eq!(
            acknowledge_pending_activations_at(
                &mut connection,
                "consumer_two",
                &[first.id, first.id, -1],
                due + POMODORO_ACTIVATION_CLAIM_LEASE_MS + 1
            )
            .unwrap()
            .acknowledged_ids,
            vec![first.id]
        );
        assert!(claim_pending_activations_at(
            &mut connection,
            "consumer_two",
            1,
            due + POMODORO_ACTIVATION_CLAIM_LEASE_MS + 2
        )
        .unwrap()
        .is_empty());
        assert_eq!(
            claim_pending_activations_at(&mut connection, "invalid consumer", 1, due).unwrap_err(),
            "pomodoro activation consumer id is invalid"
        );
    }

    #[test]
    fn notification_capability_is_strict_due_and_single_use() {
        let mut connection = database();
        let started = start_focus(&mut connection, NOW);
        let spec =
            prepare_notification_at(&mut connection, started.id, "AAAAAAAAAAAAAAAA", NOW + 1)
                .unwrap();
        assert_eq!(spec.tag, "pomodoro-session-1");
        let activation =
            parse_notification_activation(&Url::parse(&spec.activation_uri).unwrap()).unwrap();
        let due = started.target_ends_at_unix_ms.unwrap();
        assert!(scheduled_notification_matches_running_session(
            &connection,
            started.id,
            &spec.tag,
            due,
            &spec.activation_uri
        )
        .unwrap());
        assert!(!scheduled_notification_matches_running_session(
            &connection,
            started.id,
            &spec.tag,
            due + 1,
            &spec.activation_uri
        )
        .unwrap());
        assert!(!scheduled_notification_matches_running_session(
            &connection,
            started.id,
            &spec.tag,
            due,
            "startodo://focus/open?session=1&token=BBBBBBBBBBBBBBBB"
        )
        .unwrap());
        assert_eq!(
            consume_notification_activation_at(&mut connection, &activation, NOW + 1).unwrap(),
            None
        );
        let forged = PomodoroNotificationActivation {
            session_id: started.id,
            token: "BBBBBBBBBBBBBBBB".to_string(),
        };
        assert_eq!(
            consume_notification_activation_at(
                &mut connection,
                &forged,
                started.target_ends_at_unix_ms.unwrap()
            )
            .unwrap(),
            None
        );

        let mut expired_connection = database();
        let expired_started = start_focus(&mut expired_connection, NOW);
        let expired_spec = prepare_notification_at(
            &mut expired_connection,
            expired_started.id,
            "CCCCCCCCCCCCCCCC",
            NOW + 1,
        )
        .unwrap();
        let expired_activation =
            parse_notification_activation(&Url::parse(&expired_spec.activation_uri).unwrap())
                .unwrap();
        let expired_due = expired_started.target_ends_at_unix_ms.unwrap();
        settle_at(&mut expired_connection, expired_due).unwrap();
        assert_eq!(
            consume_notification_activation_at(
                &mut expired_connection,
                &expired_activation,
                expired_due + POMODORO_ACTIVATION_GRACE_MS + 1,
            )
            .unwrap(),
            None
        );

        assert_eq!(
            consume_notification_activation_at(
                &mut connection,
                &activation,
                started.target_ends_at_unix_ms.unwrap()
            )
            .unwrap(),
            Some(started.id)
        );
        assert_eq!(
            consume_notification_activation_at(
                &mut connection,
                &activation,
                started.target_ends_at_unix_ms.unwrap()
            )
            .unwrap(),
            None
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM pomodoro_activation_inbox WHERE session_id = ?1",
                    [started.id],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        assert!(parse_notification_activation(
            &Url::parse("startodo://focus/open?session=1&token=AAAAAAAAAAAAAAAA&x=1").unwrap()
        )
        .is_none());
        assert!(parse_notification_activation(
            &Url::parse("startodo://focus/open?session=01&token=AAAAAAAAAAAAAAAA").unwrap()
        )
        .is_none());
        assert!(parse_notification_activation(
            &Url::parse("startodo://focus/open?session=0&token=AAAAAAAAAAAAAAAA").unwrap()
        )
        .is_none());
        assert!(parse_notification_activation(
            &Url::parse("startodo://focus/open?session=9223372036854775808&token=AAAAAAAAAAAAAAAA")
                .unwrap()
        )
        .is_none());
    }
}
