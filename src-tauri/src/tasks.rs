use std::{
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{Duration, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

pub const MAX_JAVASCRIPT_DATE_UNIX_MS: i64 = 8_640_000_000_000_000;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    #[default]
    None,
    Low,
    Medium,
    High,
}

impl Priority {
    pub fn to_db(self) -> i64 {
        match self {
            Self::None => 0,
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
        }
    }

    pub fn from_db(value: i64) -> rusqlite::Result<Self> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Low),
            2 => Ok(Self::Medium),
            3 => Ok(Self::High),
            _ => Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Integer,
                format!("priority must be between 0 and 3, got {value}").into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RecurrenceKind {
    #[default]
    None,
    Daily,
    Weekly,
}

impl RecurrenceKind {
    fn to_db(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Daily => Some("daily"),
            Self::Weekly => Some("weekly"),
        }
    }

    fn from_db(value: Option<String>) -> rusqlite::Result<Self> {
        match value.as_deref() {
            None => Ok(Self::None),
            Some("daily") => Ok(Self::Daily),
            Some("weekly") => Ok(Self::Weekly),
            Some(value) => Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                format!("recurrence kind must be daily or weekly, got {value}").into(),
            )),
        }
    }
}

const RECURRENCE_DST_POLICY: &str = "shift_forward_earlier";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub notes: String,
    pub planned_date: Option<String>,
    pub due_at_unix_ms: Option<i64>,
    pub reminder_at_unix_ms: Option<i64>,
    pub reminder_fired_at_unix_ms: Option<i64>,
    pub deleted_at_unix_ms: Option<i64>,
    pub project_id: Option<i64>,
    pub priority: Priority,
    pub recurrence_kind: RecurrenceKind,
    pub recurrence_series_id: Option<i64>,
    pub recurrence_source_task_id: Option<i64>,
    pub recurrence_timezone: Option<String>,
    pub recurrence_dst_policy: Option<String>,
    pub completed_at_unix_ms: Option<i64>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskCompletionMutation {
    pub task: Task,
    pub next_task: Option<Task>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskInput {
    pub title: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub planned_date: Option<String>,
    #[serde(default)]
    pub due_at_unix_ms: Option<i64>,
    #[serde(default)]
    pub reminder_at_unix_ms: Option<i64>,
    #[serde(default)]
    pub project_id: Option<i64>,
    #[serde(default)]
    pub priority: Priority,
    #[serde(default)]
    pub recurrence_kind: RecurrenceKind,
    #[serde(default)]
    pub recurrence_timezone: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTaskInput {
    pub title: String,
    pub notes: String,
    #[serde(default)]
    pub planned_date: Option<String>,
    pub due_at_unix_ms: Option<i64>,
    #[serde(default)]
    pub reminder_at_unix_ms: Option<i64>,
    #[serde(default)]
    pub project_id: Option<i64>,
    #[serde(default)]
    pub priority: Priority,
    #[serde(default)]
    pub recurrence_kind: RecurrenceKind,
    #[serde(default)]
    pub recurrence_timezone: Option<String>,
}

pub fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn database_error(_: rusqlite::Error) -> String {
    "database operation failed".to_string()
}

fn enqueue_reminder_sync(
    connection: &Connection,
    task_id: i64,
    now_unix_ms: i64,
) -> Result<(), String> {
    crate::reminder_jobs::upsert_sync_intent_at(connection, task_id, now_unix_ms)
}

fn enqueue_reminder_cancel(
    connection: &Connection,
    task_id: i64,
    now_unix_ms: i64,
) -> Result<(), String> {
    crate::reminder_jobs::upsert_cancel_intent_at(connection, task_id, now_unix_ms)
}

fn validate_id(id: i64) -> Result<(), String> {
    if id > 0 {
        Ok(())
    } else {
        Err("task id must be greater than 0".to_string())
    }
}

fn validate_planned_date(planned_date: Option<&str>) -> Result<(), String> {
    let Some(planned_date) = planned_date else {
        return Ok(());
    };
    if planned_date.len() != 10
        || !NaiveDate::parse_from_str(planned_date, "%Y-%m-%d")
            .map(|date| date.format("%Y-%m-%d").to_string() == planned_date)
            .unwrap_or(false)
    {
        return Err("plannedDate must be a valid YYYY-MM-DD date".to_string());
    }
    Ok(())
}

fn validate_epoch(field: &str, value: Option<i64>) -> Result<(), String> {
    if value.is_some_and(|value| !(0..=MAX_JAVASCRIPT_DATE_UNIX_MS).contains(&value)) {
        return Err(format!("{field} must be a valid JavaScript date"));
    }
    Ok(())
}

fn parse_timezone(value: &str) -> Result<Tz, String> {
    Tz::from_str(value).map_err(|_| "recurrenceTimezone must be a valid IANA timezone".to_string())
}

fn validate_recurrence(
    recurrence_kind: RecurrenceKind,
    planned_date: Option<&str>,
    recurrence_timezone: Option<String>,
) -> Result<Option<String>, String> {
    if recurrence_kind == RecurrenceKind::None {
        return Ok(None);
    }
    if planned_date.is_none() {
        return Err("recurring tasks require plannedDate".to_string());
    }
    let timezone = recurrence_timezone
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "recurring tasks require recurrenceTimezone".to_string())?;
    Ok(Some(parse_timezone(&timezone)?.name().to_string()))
}

fn validate_input(
    title: String,
    notes: String,
    planned_date: Option<String>,
    due_at_unix_ms: Option<i64>,
    reminder_at_unix_ms: Option<i64>,
    recurrence_kind: RecurrenceKind,
    recurrence_timezone: Option<String>,
) -> Result<(String, String, Option<String>, Option<String>), String> {
    let title = title.trim().to_string();

    if !(1..=200).contains(&title.chars().count()) {
        return Err("title must be between 1 and 200 characters".to_string());
    }
    if notes.chars().count() > 10_000 {
        return Err("notes must be at most 10000 characters".to_string());
    }
    validate_planned_date(planned_date.as_deref())?;
    validate_epoch("dueAtUnixMs", due_at_unix_ms)?;
    validate_epoch("reminderAtUnixMs", reminder_at_unix_ms)?;
    let recurrence_timezone = validate_recurrence(
        recurrence_kind,
        planned_date.as_deref(),
        recurrence_timezone,
    )?;

    Ok((title, notes, planned_date, recurrence_timezone))
}

fn planned_date_value(value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| "plannedDate must be a valid YYYY-MM-DD date".to_string())
}

fn next_planned_date(current: &str, recurrence_kind: RecurrenceKind) -> Result<String, String> {
    let days = match recurrence_kind {
        RecurrenceKind::Daily => 1,
        RecurrenceKind::Weekly => 7,
        RecurrenceKind::None => return Err("task is not recurring".to_string()),
    };
    planned_date_value(current)?
        .checked_add_signed(Duration::days(days))
        .map(|date| date.format("%Y-%m-%d").to_string())
        .ok_or_else(|| "next recurrence date is out of range".to_string())
}

fn resolved_epoch_in_timezone(timezone: Tz, local: NaiveDateTime) -> Result<i64, String> {
    let resolved = match timezone.from_local_datetime(&local) {
        LocalResult::Single(value) => value,
        LocalResult::Ambiguous(first, second) => first.min(second),
        LocalResult::None => {
            let mut candidate = local;
            for _ in 0..(24 * 60) {
                candidate = candidate
                    .checked_add_signed(Duration::minutes(1))
                    .ok_or_else(|| "local date-time is out of range".to_string())?;
                match timezone.from_local_datetime(&candidate) {
                    LocalResult::Single(value) => return Ok(value.timestamp_millis()),
                    LocalResult::Ambiguous(first, second) => {
                        return Ok(first.min(second).timestamp_millis())
                    }
                    LocalResult::None => {}
                }
            }
            return Err("local date-time cannot be resolved in recurrenceTimezone".to_string());
        }
    };
    Ok(resolved.timestamp_millis())
}

fn local_datetime_from_epoch(timezone: Tz, value: i64) -> Result<NaiveDateTime, String> {
    Utc.timestamp_millis_opt(value)
        .single()
        .map(|date_time| date_time.with_timezone(&timezone).naive_local())
        .ok_or_else(|| "stored date-time is invalid".to_string())
}

fn shift_epoch_relative_to_planned_date(
    value: Option<i64>,
    source_planned_date: &str,
    target_planned_date: &str,
    timezone: Tz,
) -> Result<Option<i64>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let source_date = planned_date_value(source_planned_date)?;
    let target_date = planned_date_value(target_planned_date)?;
    let source_local = local_datetime_from_epoch(timezone, value)?;
    let day_offset = source_local
        .date()
        .signed_duration_since(source_date)
        .num_days();
    let target_local_date = target_date
        .checked_add_signed(Duration::days(day_offset))
        .ok_or_else(|| "next recurrence date is out of range".to_string())?;
    resolved_epoch_in_timezone(timezone, target_local_date.and_time(source_local.time())).map(Some)
}

fn shift_epoch_to_local_date(
    value: Option<i64>,
    target_date: NaiveDate,
    timezone: Tz,
) -> Result<Option<i64>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let local = local_datetime_from_epoch(timezone, value)?;
    resolved_epoch_in_timezone(timezone, target_date.and_time(local.time())).map(Some)
}

fn local_date_in_timezone(timezone: Tz, now_unix_ms: i64) -> Result<NaiveDate, String> {
    local_datetime_from_epoch(timezone, now_unix_ms).map(|local| local.date())
}

fn task_from_row(row: &Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get("id")?,
        title: row.get("title")?,
        notes: row.get("notes")?,
        planned_date: row.get("planned_date")?,
        due_at_unix_ms: row.get("due_at_unix_ms")?,
        reminder_at_unix_ms: row.get("reminder_at_unix_ms")?,
        reminder_fired_at_unix_ms: row.get("reminder_fired_at_unix_ms")?,
        deleted_at_unix_ms: row.get("deleted_at_unix_ms")?,
        project_id: row.get("project_id")?,
        priority: Priority::from_db(row.get("priority")?)?,
        recurrence_kind: RecurrenceKind::from_db(row.get("recurrence_kind")?)?,
        recurrence_series_id: row.get("recurrence_series_id")?,
        recurrence_source_task_id: row.get("recurrence_source_task_id")?,
        recurrence_timezone: row.get("recurrence_timezone")?,
        recurrence_dst_policy: row.get("recurrence_dst_policy")?,
        completed_at_unix_ms: row.get("completed_at_unix_ms")?,
        created_at_unix_ms: row.get("created_at_unix_ms")?,
        updated_at_unix_ms: row.get("updated_at_unix_ms")?,
    })
}

#[allow(dead_code)]
pub fn get(connection: &Connection, id: i64) -> Result<Task, String> {
    validate_id(id)?;
    get_task(connection, id)
}

/// Returns a task regardless of soft-deletion, and `None` after permanent deletion.
pub fn find(connection: &Connection, id: i64) -> Result<Option<Task>, String> {
    validate_id(id)?;
    connection
        .query_row(
            "
            SELECT id, title, notes, planned_date, due_at_unix_ms,
                   reminder_at_unix_ms, reminder_fired_at_unix_ms, deleted_at_unix_ms, project_id,
                   priority, recurrence_kind, recurrence_series_id, recurrence_source_task_id,
                   recurrence_timezone, recurrence_dst_policy,
                   completed_at_unix_ms, created_at_unix_ms, updated_at_unix_ms
            FROM tasks
            WHERE id = ?1
            ",
            [id],
            task_from_row,
        )
        .optional()
        .map_err(database_error)
}

fn get_task(connection: &Connection, id: i64) -> Result<Task, String> {
    connection
        .query_row(
            "
            SELECT id, title, notes, planned_date, due_at_unix_ms,
                   reminder_at_unix_ms, reminder_fired_at_unix_ms, deleted_at_unix_ms, project_id,
                   priority, recurrence_kind, recurrence_series_id, recurrence_source_task_id,
                   recurrence_timezone, recurrence_dst_policy,
                   completed_at_unix_ms, created_at_unix_ms, updated_at_unix_ms
            FROM tasks
            WHERE id = ?1
            ",
            [id],
            task_from_row,
        )
        .optional()
        .map_err(database_error)?
        .ok_or_else(|| "task not found".to_string())
}

pub fn list(connection: &Connection) -> Result<Vec<Task>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, title, notes, planned_date, due_at_unix_ms,
                   reminder_at_unix_ms, reminder_fired_at_unix_ms, deleted_at_unix_ms, project_id,
                   priority, recurrence_kind, recurrence_series_id, recurrence_source_task_id,
                   recurrence_timezone, recurrence_dst_policy,
                   completed_at_unix_ms, created_at_unix_ms, updated_at_unix_ms
            FROM tasks
            WHERE deleted_at_unix_ms IS NULL
            ORDER BY
                completed_at_unix_ms IS NOT NULL ASC,
                CASE
                    WHEN completed_at_unix_ms IS NULL AND planned_date IS NULL THEN 1
                    ELSE 0
                END ASC,
                CASE WHEN completed_at_unix_ms IS NULL THEN planned_date END ASC,
                CASE
                    WHEN completed_at_unix_ms IS NULL AND due_at_unix_ms IS NULL THEN 1
                    ELSE 0
                END ASC,
                CASE WHEN completed_at_unix_ms IS NULL THEN due_at_unix_ms END ASC,
                CASE WHEN completed_at_unix_ms IS NULL THEN priority END DESC,
                CASE WHEN completed_at_unix_ms IS NULL THEN created_at_unix_ms END DESC,
                CASE WHEN completed_at_unix_ms IS NOT NULL THEN completed_at_unix_ms END DESC,
                created_at_unix_ms DESC,
                id DESC
            ",
        )
        .map_err(database_error)?;
    let tasks = statement
        .query_map([], task_from_row)
        .map_err(database_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(database_error)?;
    Ok(tasks)
}

fn recurrence_reminder_template(
    connection: &Connection,
    series_id: i64,
) -> Result<Option<Option<i64>>, String> {
    connection
        .query_row(
            "SELECT reminder_at_unix_ms FROM recurrence_templates WHERE series_id = ?1",
            [series_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()
        .map_err(database_error)
}

fn upsert_recurrence_reminder_template(
    connection: &Connection,
    series_id: i64,
    reminder_at_unix_ms: Option<i64>,
) -> Result<(), String> {
    connection
        .execute(
            "
            INSERT INTO recurrence_templates (series_id, reminder_at_unix_ms)
            VALUES (?1, ?2)
            ON CONFLICT(series_id) DO UPDATE SET
                reminder_at_unix_ms = excluded.reminder_at_unix_ms
            ",
            params![series_id, reminder_at_unix_ms],
        )
        .map(|_| ())
        .map_err(database_error)
}

pub fn create(connection: &mut Connection, input: CreateTaskInput) -> Result<Task, String> {
    let CreateTaskInput {
        title,
        notes,
        planned_date,
        due_at_unix_ms,
        reminder_at_unix_ms,
        project_id,
        priority,
        recurrence_kind,
        recurrence_timezone,
    } = input;
    let (title, notes, planned_date, recurrence_timezone) = validate_input(
        title,
        notes.unwrap_or_default(),
        planned_date,
        due_at_unix_ms,
        reminder_at_unix_ms,
        recurrence_kind,
        recurrence_timezone,
    )?;
    let recurrence_kind_db = recurrence_kind.to_db();
    let recurrence_dst_policy =
        (recurrence_kind != RecurrenceKind::None).then_some(RECURRENCE_DST_POLICY);
    let transaction = connection.transaction().map_err(database_error)?;
    crate::projects::validate_project_assignment(&transaction, project_id)?;
    let now = now_unix_ms();
    transaction
        .execute(
            "
            INSERT INTO tasks (
                title, notes, planned_date, due_at_unix_ms, reminder_at_unix_ms,
                reminder_fired_at_unix_ms, deleted_at_unix_ms, project_id, priority,
                recurrence_kind, recurrence_series_id, recurrence_source_task_id,
                recurrence_timezone, recurrence_dst_policy, completed_at_unix_ms,
                created_at_unix_ms, updated_at_unix_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, ?6, ?7, ?8, NULL, NULL, ?9, ?10, NULL, ?11, ?11)
            ",
            params![
                title,
                notes,
                planned_date,
                due_at_unix_ms,
                reminder_at_unix_ms,
                project_id,
                priority.to_db(),
                recurrence_kind_db,
                recurrence_timezone,
                recurrence_dst_policy,
                now
            ],
        )
        .map_err(database_error)?;
    let id = transaction.last_insert_rowid();
    if recurrence_kind != RecurrenceKind::None {
        transaction
            .execute(
                "UPDATE tasks SET recurrence_series_id = ?1 WHERE id = ?1",
                [id],
            )
            .map_err(database_error)?;
        upsert_recurrence_reminder_template(&transaction, id, reminder_at_unix_ms)?;
    }
    let task = get_task(&transaction, id)?;
    enqueue_reminder_sync(&transaction, task.id, now)?;
    transaction.commit().map_err(database_error)?;
    Ok(task)
}

fn find_successor(connection: &Connection, source_task_id: i64) -> Result<Option<Task>, String> {
    connection
        .query_row(
            "
            SELECT id, title, notes, planned_date, due_at_unix_ms,
                   reminder_at_unix_ms, reminder_fired_at_unix_ms, deleted_at_unix_ms, project_id,
                   priority, recurrence_kind, recurrence_series_id, recurrence_source_task_id,
                   recurrence_timezone, recurrence_dst_policy,
                   completed_at_unix_ms, created_at_unix_ms, updated_at_unix_ms
            FROM tasks
            WHERE recurrence_source_task_id = ?1
            ",
            [source_task_id],
            task_from_row,
        )
        .optional()
        .map_err(database_error)
}

pub fn update(
    connection: &mut Connection,
    id: i64,
    input: UpdateTaskInput,
) -> Result<Task, String> {
    validate_id(id)?;
    let UpdateTaskInput {
        title,
        notes,
        planned_date,
        due_at_unix_ms,
        reminder_at_unix_ms,
        project_id,
        priority,
        recurrence_kind,
        recurrence_timezone,
    } = input;
    let (title, notes, planned_date, recurrence_timezone) = validate_input(
        title,
        notes,
        planned_date,
        due_at_unix_ms,
        reminder_at_unix_ms,
        recurrence_kind,
        recurrence_timezone,
    )?;
    let transaction = connection.transaction().map_err(database_error)?;
    let current = get_task(&transaction, id)?;
    if current.deleted_at_unix_ms.is_some() {
        return Err("task is in trash".to_string());
    }
    if project_id != current.project_id {
        crate::projects::validate_project_assignment(&transaction, project_id)?;
    }
    if find_successor(&transaction, id)?.is_some()
        && (current.recurrence_kind != recurrence_kind
            || current.recurrence_timezone != recurrence_timezone)
    {
        return Err("cannot change recurrence after a successor exists".to_string());
    }
    let reminder_fired_at_unix_ms = (current.reminder_at_unix_ms == reminder_at_unix_ms)
        .then_some(current.reminder_fired_at_unix_ms)
        .flatten();
    let recurrence_kind_db = recurrence_kind.to_db();
    let recurrence_dst_policy =
        (recurrence_kind != RecurrenceKind::None).then_some(RECURRENCE_DST_POLICY);
    let recurrence_series_id = (recurrence_kind != RecurrenceKind::None)
        .then_some(current.recurrence_series_id.unwrap_or(id));
    let reminder_template_at_unix_ms = if recurrence_kind == RecurrenceKind::None {
        None
    } else {
        let series_id = recurrence_series_id.expect("recurring tasks have a series id");
        let preserve_template = current.recurrence_kind == recurrence_kind
            && current.recurrence_timezone == recurrence_timezone
            && current.planned_date == planned_date
            && current.reminder_at_unix_ms == reminder_at_unix_ms;
        if preserve_template {
            recurrence_reminder_template(&transaction, series_id)?
                .unwrap_or(current.reminder_at_unix_ms)
        } else {
            reminder_at_unix_ms
        }
    };
    let recurrence_source_task_id = if recurrence_kind == RecurrenceKind::None {
        None
    } else {
        current.recurrence_source_task_id
    };
    transaction
        .execute(
            "
            UPDATE tasks
            SET title = ?1, notes = ?2, planned_date = ?3, due_at_unix_ms = ?4,
                reminder_at_unix_ms = ?5, reminder_fired_at_unix_ms = ?6,
                project_id = ?7, priority = ?8, recurrence_kind = ?9, recurrence_series_id = ?10,
                recurrence_timezone = ?11, recurrence_dst_policy = ?12,
                recurrence_source_task_id = ?13, updated_at_unix_ms = ?14
            WHERE id = ?15
            ",
            params![
                title,
                notes,
                planned_date,
                due_at_unix_ms,
                reminder_at_unix_ms,
                reminder_fired_at_unix_ms,
                project_id,
                priority.to_db(),
                recurrence_kind_db,
                recurrence_series_id,
                recurrence_timezone,
                recurrence_dst_policy,
                recurrence_source_task_id,
                now_unix_ms(),
                id
            ],
        )
        .map_err(database_error)?;
    if let Some(series_id) = recurrence_series_id {
        upsert_recurrence_reminder_template(&transaction, series_id, reminder_template_at_unix_ms)?;
    }
    let task = get_task(&transaction, id)?;
    enqueue_reminder_sync(&transaction, task.id, now_unix_ms())?;
    transaction.commit().map_err(database_error)?;
    Ok(task)
}

pub fn set_planned_date(
    connection: &mut Connection,
    id: i64,
    planned_date: Option<String>,
) -> Result<Task, String> {
    validate_id(id)?;
    validate_planned_date(planned_date.as_deref())?;
    let transaction = connection.transaction().map_err(database_error)?;
    let current = get_task(&transaction, id)?;
    if current.deleted_at_unix_ms.is_some() {
        return Err("task is in trash".to_string());
    }
    if current.completed_at_unix_ms.is_some() {
        return Err("completed tasks cannot be rescheduled".to_string());
    }
    if current.recurrence_kind != RecurrenceKind::None {
        return Err("recurring tasks cannot be rescheduled".to_string());
    }
    if current.planned_date == planned_date {
        transaction.commit().map_err(database_error)?;
        return Ok(current);
    }

    transaction
        .execute(
            "UPDATE tasks SET planned_date = ?1, updated_at_unix_ms = ?2 WHERE id = ?3",
            params![planned_date, now_unix_ms(), id],
        )
        .map_err(database_error)?;
    let task = get_task(&transaction, id)?;
    transaction.commit().map_err(database_error)?;
    Ok(task)
}

fn create_successor(
    connection: &Connection,
    current: &Task,
    now: i64,
) -> Result<Option<Task>, String> {
    if current.recurrence_kind == RecurrenceKind::None {
        return Ok(None);
    }
    let current_planned_date = current
        .planned_date
        .as_deref()
        .ok_or_else(|| "recurring tasks require plannedDate".to_string())?;
    let timezone_name = current
        .recurrence_timezone
        .as_deref()
        .ok_or_else(|| "recurring tasks require recurrenceTimezone".to_string())?;
    let timezone = parse_timezone(timezone_name)?;
    let next_planned_date = next_planned_date(current_planned_date, current.recurrence_kind)?;
    let due_at_unix_ms = shift_epoch_relative_to_planned_date(
        current.due_at_unix_ms,
        current_planned_date,
        &next_planned_date,
        timezone,
    )?;
    let recurrence_series_id = current.recurrence_series_id.unwrap_or(current.id);
    let reminder_template_at_unix_ms =
        recurrence_reminder_template(connection, recurrence_series_id)?
            .unwrap_or(current.reminder_at_unix_ms);
    let reminder_at_unix_ms = shift_epoch_relative_to_planned_date(
        reminder_template_at_unix_ms,
        current_planned_date,
        &next_planned_date,
        timezone,
    )?;
    let changed = connection
        .execute(
            "
            INSERT INTO tasks (
                title, notes, planned_date, due_at_unix_ms, reminder_at_unix_ms,
                reminder_fired_at_unix_ms, deleted_at_unix_ms, project_id, priority,
                recurrence_kind, recurrence_series_id, recurrence_source_task_id,
                recurrence_timezone, recurrence_dst_policy, completed_at_unix_ms,
                created_at_unix_ms, updated_at_unix_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, ?6, ?7, ?8, ?9, ?10, ?11, ?12, NULL, ?13, ?13)
            ON CONFLICT(recurrence_source_task_id) WHERE recurrence_source_task_id IS NOT NULL DO NOTHING
            ",
            params![
                current.title,
                current.notes,
                next_planned_date,
                due_at_unix_ms,
                reminder_at_unix_ms,
                current.project_id,
                current.priority.to_db(),
                current.recurrence_kind.to_db(),
                recurrence_series_id,
                current.id,
                timezone_name,
                current
                    .recurrence_dst_policy
                    .as_deref()
                    .unwrap_or(RECURRENCE_DST_POLICY),
                now
            ],
        )
        .map_err(database_error)?;
    if changed == 0 {
        return find_successor(connection, current.id)?
            .map(Some)
            .ok_or_else(|| {
                "recurrence successor conflict did not produce a successor".to_string()
            });
    }
    let successor = get_task(connection, connection.last_insert_rowid())?;
    upsert_recurrence_reminder_template(connection, recurrence_series_id, reminder_at_unix_ms)?;
    Ok(Some(successor))
}

pub fn set_completed(
    connection: &mut Connection,
    id: i64,
    completed: bool,
) -> Result<TaskCompletionMutation, String> {
    validate_id(id)?;
    let transaction = connection.transaction().map_err(database_error)?;
    let current = get_task(&transaction, id)?;
    if current.deleted_at_unix_ms.is_some() {
        return Err("task is in trash".to_string());
    }
    if current.completed_at_unix_ms.is_some() == completed {
        let next_task = if completed {
            find_successor(&transaction, id)?
        } else {
            None
        };
        transaction.commit().map_err(database_error)?;
        return Ok(TaskCompletionMutation {
            task: current,
            next_task,
        });
    }

    let now = now_unix_ms();
    let next_task = if completed {
        create_successor(&transaction, &current, now)?
    } else {
        None
    };
    transaction
        .execute(
            "
            UPDATE tasks
            SET completed_at_unix_ms = ?1, updated_at_unix_ms = ?2
            WHERE id = ?3
            ",
            params![completed.then_some(now), now, id],
        )
        .map_err(database_error)?;
    let task = get_task(&transaction, id)?;
    enqueue_reminder_sync(&transaction, task.id, now)?;
    if let Some(next_task) = next_task.as_ref() {
        enqueue_reminder_sync(&transaction, next_task.id, now)?;
    }
    transaction.commit().map_err(database_error)?;
    Ok(TaskCompletionMutation { task, next_task })
}

pub fn snooze(connection: &mut Connection, id: i64, until_unix_ms: i64) -> Result<Task, String> {
    validate_id(id)?;
    validate_epoch("untilUnixMs", Some(until_unix_ms))?;
    if until_unix_ms <= now_unix_ms() {
        return Err("untilUnixMs must be in the future".to_string());
    }
    let transaction = connection.transaction().map_err(database_error)?;
    let current = get_task(&transaction, id)?;
    if current.deleted_at_unix_ms.is_some() || current.completed_at_unix_ms.is_some() {
        return Err("only active tasks can be snoozed".to_string());
    }
    let now = now_unix_ms();
    transaction
        .execute(
            "
            UPDATE tasks
            SET reminder_at_unix_ms = ?1, reminder_fired_at_unix_ms = NULL, updated_at_unix_ms = ?2
            WHERE id = ?3
            ",
            params![until_unix_ms, now, id],
        )
        .map_err(database_error)?;
    let task = get_task(&transaction, id)?;
    enqueue_reminder_sync(&transaction, task.id, now)?;
    transaction.commit().map_err(database_error)?;
    Ok(task)
}

fn defer_to_tomorrow_at(
    connection: &mut Connection,
    id: i64,
    timezone_name: &str,
    now: i64,
) -> Result<Task, String> {
    validate_id(id)?;
    let transaction = connection.transaction().map_err(database_error)?;
    let current = get_task(&transaction, id)?;
    if current.deleted_at_unix_ms.is_some() || current.completed_at_unix_ms.is_some() {
        return Err("only active tasks can be deferred".to_string());
    }
    let timezone = match current.recurrence_kind {
        RecurrenceKind::None => parse_timezone(timezone_name)?,
        RecurrenceKind::Daily | RecurrenceKind::Weekly => {
            let stored_timezone = current
                .recurrence_timezone
                .as_deref()
                .ok_or_else(|| "recurring task is missing recurrenceTimezone".to_string())?;
            if timezone_name != stored_timezone {
                return Err("timezone must match the recurring task recurrenceTimezone".to_string());
            }
            parse_timezone(stored_timezone)?
        }
    };
    let tomorrow = local_date_in_timezone(timezone, now)?
        .checked_add_signed(Duration::days(1))
        .ok_or_else(|| "tomorrow is out of range".to_string())?;
    let due_at_unix_ms = shift_epoch_to_local_date(current.due_at_unix_ms, tomorrow, timezone)?;
    let reminder_at_unix_ms =
        shift_epoch_to_local_date(current.reminder_at_unix_ms, tomorrow, timezone)?;
    let planned_date = tomorrow.format("%Y-%m-%d").to_string();
    let recurrence_template = match current.recurrence_kind {
        RecurrenceKind::None => None,
        RecurrenceKind::Daily | RecurrenceKind::Weekly => {
            let current_planned_date = current
                .planned_date
                .as_deref()
                .ok_or_else(|| "recurring tasks require plannedDate".to_string())?;
            let series_id = current.recurrence_series_id.unwrap_or(current.id);
            let template = recurrence_reminder_template(&transaction, series_id)?
                .unwrap_or(current.reminder_at_unix_ms);
            Some((
                series_id,
                shift_epoch_relative_to_planned_date(
                    template,
                    current_planned_date,
                    &planned_date,
                    timezone,
                )?,
            ))
        }
    };
    transaction
        .execute(
            "
            UPDATE tasks
            SET planned_date = ?1, due_at_unix_ms = ?2, reminder_at_unix_ms = ?3,
                reminder_fired_at_unix_ms = NULL, updated_at_unix_ms = ?4
            WHERE id = ?5
            ",
            params![planned_date, due_at_unix_ms, reminder_at_unix_ms, now, id],
        )
        .map_err(database_error)?;
    if let Some((series_id, reminder_at_unix_ms)) = recurrence_template {
        upsert_recurrence_reminder_template(&transaction, series_id, reminder_at_unix_ms)?;
    }
    let task = get_task(&transaction, id)?;
    enqueue_reminder_sync(&transaction, task.id, now)?;
    transaction.commit().map_err(database_error)?;
    Ok(task)
}

pub fn defer_to_tomorrow(
    connection: &mut Connection,
    id: i64,
    timezone_name: String,
) -> Result<Task, String> {
    defer_to_tomorrow_at(connection, id, &timezone_name, now_unix_ms())
}

pub fn list_missed(connection: &Connection, now_unix_ms: i64) -> Result<Vec<i64>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id
            FROM tasks
            WHERE deleted_at_unix_ms IS NULL
              AND completed_at_unix_ms IS NULL
              AND reminder_at_unix_ms IS NOT NULL
              AND reminder_at_unix_ms < ?1
              AND reminder_fired_at_unix_ms IS NULL
            ORDER BY reminder_at_unix_ms ASC, id ASC
            ",
        )
        .map_err(database_error)?;
    let missed = statement
        .query_map([now_unix_ms], |row| row.get(0))
        .map_err(database_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(database_error)?;
    Ok(missed)
}

#[cfg(test)]
fn mark_reminder_fired(
    connection: &Connection,
    id: i64,
    fired_at_unix_ms: i64,
) -> Result<(), String> {
    mark_reminder_fired_changed(connection, id, fired_at_unix_ms).map(|_| ())
}

fn mark_reminder_fired_changed(
    connection: &Connection,
    id: i64,
    fired_at_unix_ms: i64,
) -> Result<bool, String> {
    validate_id(id)?;
    validate_epoch("reminderFiredAtUnixMs", Some(fired_at_unix_ms))?;
    let exists = find(connection, id)?.is_some();
    if !exists {
        return Err("task not found".to_string());
    }
    connection
        .execute(
            "
            UPDATE tasks
            SET reminder_fired_at_unix_ms = ?1
            WHERE id = ?2
              AND deleted_at_unix_ms IS NULL
              AND reminder_fired_at_unix_ms IS NULL
            ",
            params![fired_at_unix_ms, id],
        )
        .map(|changed| changed == 1)
        .map_err(database_error)
}

pub fn mark_reminder_fired_if_due(
    connection: &Connection,
    id: i64,
    fired_at_unix_ms: i64,
) -> Result<bool, String> {
    validate_id(id)?;
    validate_epoch("reminderFiredAtUnixMs", Some(fired_at_unix_ms))?;
    let task = get_task(connection, id)?;
    let Some(reminder_at_unix_ms) = task.reminder_at_unix_ms else {
        return Ok(false);
    };
    if task.deleted_at_unix_ms.is_some()
        || task.completed_at_unix_ms.is_some()
        || task.reminder_fired_at_unix_ms.is_some()
        || reminder_at_unix_ms > fired_at_unix_ms
    {
        return Ok(false);
    }
    mark_reminder_fired_changed(connection, id, fired_at_unix_ms)
}

pub fn list_deleted(connection: &Connection) -> Result<Vec<Task>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, title, notes, planned_date, due_at_unix_ms,
                   reminder_at_unix_ms, reminder_fired_at_unix_ms, deleted_at_unix_ms, project_id,
                   priority, recurrence_kind, recurrence_series_id, recurrence_source_task_id,
                   recurrence_timezone, recurrence_dst_policy,
                   completed_at_unix_ms, created_at_unix_ms, updated_at_unix_ms
            FROM tasks
            WHERE deleted_at_unix_ms IS NOT NULL
            ORDER BY deleted_at_unix_ms DESC, id DESC
            ",
        )
        .map_err(database_error)?;
    let tasks = statement
        .query_map([], task_from_row)
        .map_err(database_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(database_error)?;
    Ok(tasks)
}

pub fn delete(connection: &mut Connection, id: i64) -> Result<(), String> {
    validate_id(id)?;
    let transaction = connection.transaction().map_err(database_error)?;
    let current = get_task(&transaction, id)?;
    if current.deleted_at_unix_ms.is_some() {
        return Err("task already in trash".to_string());
    }
    let now = now_unix_ms();
    let changed = transaction
        .execute(
            "UPDATE tasks SET deleted_at_unix_ms = ?1, updated_at_unix_ms = ?1 WHERE id = ?2",
            params![now, id],
        )
        .map_err(database_error)?;
    if changed == 0 {
        return Err("task not found".to_string());
    }
    enqueue_reminder_cancel(&transaction, id, now)?;
    transaction.commit().map_err(database_error)
}

pub fn restore(connection: &mut Connection, id: i64) -> Result<Task, String> {
    validate_id(id)?;
    let transaction = connection.transaction().map_err(database_error)?;
    let current = get_task(&transaction, id)?;
    if current.deleted_at_unix_ms.is_none() {
        return Err("task is not in trash".to_string());
    }
    let now = now_unix_ms();
    transaction
        .execute(
            "UPDATE tasks SET deleted_at_unix_ms = NULL, updated_at_unix_ms = ?1 WHERE id = ?2",
            params![now, id],
        )
        .map_err(database_error)?;
    let task = get_task(&transaction, id)?;
    enqueue_reminder_sync(&transaction, task.id, now)?;
    transaction.commit().map_err(database_error)?;
    Ok(task)
}

pub fn permanently_delete(connection: &mut Connection, id: i64) -> Result<(), String> {
    validate_id(id)?;
    let transaction = connection.transaction().map_err(database_error)?;
    let current = get_task(&transaction, id)?;
    if current.deleted_at_unix_ms.is_none() {
        return Err("task is not in trash".to_string());
    }
    let now = now_unix_ms();
    enqueue_reminder_cancel(&transaction, id, now)?;
    let changed = transaction
        .execute(
            "DELETE FROM tasks WHERE id = ?1 AND deleted_at_unix_ms IS NOT NULL",
            [id],
        )
        .map_err(database_error)?;
    if changed == 0 {
        return Err("task not found".to_string());
    }
    transaction.commit().map_err(database_error)
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use chrono::Timelike;
    use serde_json::json;

    use super::*;

    fn connection() -> Connection {
        let connection = Connection::open_in_memory().expect("in-memory database should open");
        connection
            .execute_batch(
                "
                PRAGMA foreign_keys = ON;
                CREATE TABLE projects (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    archived_at_unix_ms INTEGER,
                    created_at_unix_ms INTEGER NOT NULL,
                    updated_at_unix_ms INTEGER NOT NULL
                );
                CREATE TABLE tasks (
                    id INTEGER PRIMARY KEY,
                    title TEXT NOT NULL,
                    notes TEXT NOT NULL DEFAULT '',
                    planned_date TEXT,
                    due_at_unix_ms INTEGER,
                    reminder_at_unix_ms INTEGER,
                    reminder_fired_at_unix_ms INTEGER,
                    deleted_at_unix_ms INTEGER,
                    project_id INTEGER REFERENCES projects(id),
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
                    CHECK (planned_date IS NULL OR length(planned_date) = 10),
                    CHECK (due_at_unix_ms IS NULL OR due_at_unix_ms BETWEEN 0 AND 8640000000000000),
                    CHECK (reminder_at_unix_ms IS NULL OR reminder_at_unix_ms BETWEEN 0 AND 8640000000000000),
                    CHECK (reminder_fired_at_unix_ms IS NULL OR reminder_fired_at_unix_ms BETWEEN 0 AND 8640000000000000),
                    CHECK (priority BETWEEN 0 AND 3),
                    CHECK (completed_at_unix_ms IS NULL OR completed_at_unix_ms >= 0)
                );
                CREATE TABLE recurrence_templates (
                    series_id INTEGER PRIMARY KEY,
                    reminder_at_unix_ms INTEGER
                );
                CREATE TABLE reminder_sync_jobs (
                    task_id INTEGER PRIMARY KEY,
                    intent TEXT NOT NULL,
                    generation INTEGER NOT NULL,
                    attempt_count INTEGER NOT NULL DEFAULT 0,
                    next_attempt_at_unix_ms INTEGER NOT NULL,
                    last_error TEXT,
                    updated_at_unix_ms INTEGER NOT NULL
                );
                CREATE TABLE reminder_deliveries (
                    id TEXT PRIMARY KEY,
                    task_id INTEGER NOT NULL,
                    generation INTEGER NOT NULL DEFAULT 0
                );
                CREATE UNIQUE INDEX idx_tasks_recurrence_source ON tasks (recurrence_source_task_id) WHERE recurrence_source_task_id IS NOT NULL;
                CREATE UNIQUE INDEX idx_tasks_recurrence_occurrence ON tasks (recurrence_series_id, planned_date) WHERE recurrence_series_id IS NOT NULL AND planned_date IS NOT NULL;
                ",
            )
            .expect("tasks table should be created");
        connection
    }

    fn input(title: &str) -> CreateTaskInput {
        CreateTaskInput {
            title: title.to_string(),
            notes: None,
            planned_date: None,
            due_at_unix_ms: None,
            reminder_at_unix_ms: None,
            project_id: None,
            priority: Priority::None,
            recurrence_kind: RecurrenceKind::None,
            recurrence_timezone: None,
        }
    }

    fn create_project(connection: &Connection, name: &str, archived: bool) -> i64 {
        connection
            .execute(
                "
                INSERT INTO projects (name, archived_at_unix_ms, created_at_unix_ms, updated_at_unix_ms)
                VALUES (?1, ?2, 1, 1)
                ",
                params![name, archived.then_some(1)],
            )
            .expect("project fixture should insert");
        connection.last_insert_rowid()
    }

    #[test]
    fn validation_trims_title_preserves_notes_and_rejects_invalid_input() {
        let mut connection = connection();
        let task = create(
            &mut connection,
            CreateTaskInput {
                title: "  任务  ".to_string(),
                notes: Some("  note  ".to_string()),
                planned_date: None,
                due_at_unix_ms: Some(0),
                reminder_at_unix_ms: None,
                project_id: None,
                priority: Priority::None,
                recurrence_kind: RecurrenceKind::None,
                recurrence_timezone: None,
            },
        )
        .expect("valid task should be created");
        assert_eq!(task.title, "任务");
        assert_eq!(task.notes, "  note  ");
        assert_eq!(task.due_at_unix_ms, Some(0));

        assert_eq!(
            create(&mut connection, input("   ")).unwrap_err(),
            "title must be between 1 and 200 characters"
        );
        assert_eq!(
            create(
                &mut connection,
                CreateTaskInput {
                    title: "任".repeat(201),
                    notes: None,
                    planned_date: None,
                    due_at_unix_ms: None,
                    reminder_at_unix_ms: None,
                    project_id: None,
                    priority: Priority::None,
                    recurrence_kind: RecurrenceKind::None,
                    recurrence_timezone: None,
                },
            )
            .unwrap_err(),
            "title must be between 1 and 200 characters"
        );
        assert_eq!(
            create(
                &mut connection,
                CreateTaskInput {
                    title: "ok".to_string(),
                    notes: Some("注".repeat(10_001)),
                    planned_date: None,
                    due_at_unix_ms: None,
                    reminder_at_unix_ms: None,
                    project_id: None,
                    priority: Priority::None,
                    recurrence_kind: RecurrenceKind::None,
                    recurrence_timezone: None,
                },
            )
            .unwrap_err(),
            "notes must be at most 10000 characters"
        );
        for due_at_unix_ms in [-1, MAX_JAVASCRIPT_DATE_UNIX_MS + 1] {
            assert_eq!(
                create(
                    &mut connection,
                    CreateTaskInput {
                        title: "ok".to_string(),
                        notes: None,
                        planned_date: None,
                        due_at_unix_ms: Some(due_at_unix_ms),
                        reminder_at_unix_ms: None,
                        project_id: None,
                        priority: Priority::None,
                        recurrence_kind: RecurrenceKind::None,
                        recurrence_timezone: None,
                    },
                )
                .unwrap_err(),
                "dueAtUnixMs must be a valid JavaScript date"
            );
        }
        assert_eq!(
            delete(&mut connection, 0).unwrap_err(),
            "task id must be greater than 0"
        );

        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
            .expect("task count should query");
        assert_eq!(count, 1);
    }

    #[test]
    fn active_projects_can_be_assigned_and_archived_or_unknown_projects_are_rejected() {
        let mut connection = connection();
        let active_project_id = create_project(&connection, "Active", false);
        let archived_project_id = create_project(&connection, "Archived", true);

        let mut assigned_input = input("assigned");
        assigned_input.project_id = Some(active_project_id);
        let assigned =
            create(&mut connection, assigned_input).expect("active project should assign");
        assert_eq!(assigned.project_id, Some(active_project_id));
        assert_eq!(
            find(&connection, assigned.id)
                .expect("task should be found")
                .expect("task should exist")
                .project_id,
            Some(active_project_id)
        );

        let mut archived_input = input("archived");
        archived_input.project_id = Some(archived_project_id);
        assert_eq!(
            create(&mut connection, archived_input).unwrap_err(),
            "project is archived"
        );

        let mut unknown_input = input("unknown");
        unknown_input.project_id = Some(999);
        assert_eq!(
            create(&mut connection, unknown_input).unwrap_err(),
            "project not found"
        );

        let mut update_input = UpdateTaskInput {
            title: assigned.title.clone(),
            notes: assigned.notes.clone(),
            planned_date: assigned.planned_date.clone(),
            due_at_unix_ms: assigned.due_at_unix_ms,
            reminder_at_unix_ms: assigned.reminder_at_unix_ms,
            project_id: Some(archived_project_id),
            priority: assigned.priority,
            recurrence_kind: assigned.recurrence_kind,
            recurrence_timezone: assigned.recurrence_timezone.clone(),
        };
        assert_eq!(
            update(&mut connection, assigned.id, update_input).unwrap_err(),
            "project is archived"
        );
        update_input = UpdateTaskInput {
            title: assigned.title,
            notes: assigned.notes,
            planned_date: assigned.planned_date,
            due_at_unix_ms: assigned.due_at_unix_ms,
            reminder_at_unix_ms: assigned.reminder_at_unix_ms,
            project_id: Some(999),
            priority: assigned.priority,
            recurrence_kind: assigned.recurrence_kind,
            recurrence_timezone: assigned.recurrence_timezone,
        };
        assert_eq!(
            update(&mut connection, assigned.id, update_input).unwrap_err(),
            "project not found"
        );

        connection
            .execute(
                "UPDATE tasks SET completed_at_unix_ms = 1 WHERE id = ?1",
                [assigned.id],
            )
            .expect("task should complete");
        connection
            .execute(
                "UPDATE projects SET archived_at_unix_ms = 1 WHERE id = ?1",
                [active_project_id],
            )
            .expect("project should archive");
        let preserved = update(
            &mut connection,
            assigned.id,
            UpdateTaskInput {
                title: "renamed archived task".to_string(),
                notes: "updated without reassignment".to_string(),
                planned_date: None,
                due_at_unix_ms: None,
                reminder_at_unix_ms: None,
                project_id: Some(active_project_id),
                priority: Priority::None,
                recurrence_kind: RecurrenceKind::None,
                recurrence_timezone: None,
            },
        )
        .expect("an existing archived assignment may be preserved");
        assert_eq!(preserved.project_id, Some(active_project_id));
        assert_eq!(preserved.title, "renamed archived task");
    }

    #[test]
    fn planned_date_rescheduling_is_narrow_persistent_and_idempotent() {
        let mut connection = connection();
        let project_id = create_project(&connection, "Planning", false);
        let mut task_input = input("plan me");
        task_input.notes = Some("keep every other field".to_string());
        task_input.due_at_unix_ms = Some(42);
        task_input.reminder_at_unix_ms = Some(43);
        task_input.project_id = Some(project_id);
        task_input.priority = Priority::High;
        let created = create(&mut connection, task_input).expect("task should create");
        connection
            .execute(
                "UPDATE tasks SET reminder_fired_at_unix_ms = 41 WHERE id = ?1",
                [created.id],
            )
            .expect("fixture should mark reminder fired");
        let before = get(&connection, created.id).expect("task should reload");
        let reminder_job_before: (String, i64, i64, i64, Option<String>, i64) = connection
            .query_row(
                "SELECT intent, generation, attempt_count, next_attempt_at_unix_ms, last_error, updated_at_unix_ms FROM reminder_sync_jobs WHERE task_id = ?1",
                [created.id],
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
            .expect("reminder job should exist");
        let assert_other_fields_unchanged = |task: &Task| {
            assert_eq!(task.title, before.title);
            assert_eq!(task.notes, before.notes);
            assert_eq!(task.due_at_unix_ms, before.due_at_unix_ms);
            assert_eq!(task.reminder_at_unix_ms, before.reminder_at_unix_ms);
            assert_eq!(
                task.reminder_fired_at_unix_ms,
                before.reminder_fired_at_unix_ms
            );
            assert_eq!(task.deleted_at_unix_ms, before.deleted_at_unix_ms);
            assert_eq!(task.project_id, before.project_id);
            assert_eq!(task.priority, before.priority);
            assert_eq!(task.recurrence_kind, before.recurrence_kind);
            assert_eq!(task.recurrence_series_id, before.recurrence_series_id);
            assert_eq!(
                task.recurrence_source_task_id,
                before.recurrence_source_task_id
            );
            assert_eq!(task.recurrence_timezone, before.recurrence_timezone);
            assert_eq!(task.recurrence_dst_policy, before.recurrence_dst_policy);
            assert_eq!(task.completed_at_unix_ms, before.completed_at_unix_ms);
            assert_eq!(task.created_at_unix_ms, before.created_at_unix_ms);
        };

        thread::sleep(Duration::from_millis(2));
        let scheduled =
            set_planned_date(&mut connection, created.id, Some("2026-08-10".to_string()))
                .expect("task should be scheduled");
        assert_eq!(scheduled.planned_date.as_deref(), Some("2026-08-10"));
        assert!(scheduled.updated_at_unix_ms > before.updated_at_unix_ms);
        assert_other_fields_unchanged(&scheduled);
        assert_eq!(
            get(&connection, created.id).expect("scheduled task should persist"),
            scheduled
        );

        let scheduled_updated_at = scheduled.updated_at_unix_ms;
        thread::sleep(Duration::from_millis(2));
        let unchanged =
            set_planned_date(&mut connection, created.id, Some("2026-08-10".to_string()))
                .expect("same date should be idempotent");
        assert_eq!(unchanged.updated_at_unix_ms, scheduled_updated_at);
        assert_other_fields_unchanged(&unchanged);

        thread::sleep(Duration::from_millis(2));
        let moved = set_planned_date(&mut connection, created.id, Some("2026-08-11".to_string()))
            .expect("task should move to another date");
        assert_eq!(moved.planned_date.as_deref(), Some("2026-08-11"));
        assert!(moved.updated_at_unix_ms > unchanged.updated_at_unix_ms);
        assert_other_fields_unchanged(&moved);

        thread::sleep(Duration::from_millis(2));
        let cleared = set_planned_date(&mut connection, created.id, None)
            .expect("task should return to the unscheduled pool");
        assert_eq!(cleared.planned_date, None);
        assert!(cleared.updated_at_unix_ms > moved.updated_at_unix_ms);
        assert_other_fields_unchanged(&cleared);
        assert_eq!(
            get(&connection, created.id).expect("cleared task should persist"),
            cleared
        );

        let reminder_job_after: (String, i64, i64, i64, Option<String>, i64) = connection
            .query_row(
                "SELECT intent, generation, attempt_count, next_attempt_at_unix_ms, last_error, updated_at_unix_ms FROM reminder_sync_jobs WHERE task_id = ?1",
                [created.id],
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
            .expect("reminder job should remain");
        assert_eq!(reminder_job_after, reminder_job_before);
    }

    #[test]
    fn planned_date_rescheduling_rejects_invalid_or_ineligible_tasks() {
        let mut connection = connection();
        let active = create(&mut connection, input("active")).expect("task should create");

        assert_eq!(
            set_planned_date(&mut connection, 0, None).unwrap_err(),
            "task id must be greater than 0"
        );
        assert_eq!(
            set_planned_date(&mut connection, 999, None).unwrap_err(),
            "task not found"
        );
        assert_eq!(
            set_planned_date(&mut connection, active.id, Some("2026-02-30".to_string()))
                .unwrap_err(),
            "plannedDate must be a valid YYYY-MM-DD date"
        );

        let completed = create(&mut connection, input("completed")).expect("task should create");
        set_completed(&mut connection, completed.id, true).expect("task should complete");
        assert_eq!(
            set_planned_date(
                &mut connection,
                completed.id,
                Some("2026-08-10".to_string())
            )
            .unwrap_err(),
            "completed tasks cannot be rescheduled"
        );

        let deleted = create(&mut connection, input("deleted")).expect("task should create");
        delete(&mut connection, deleted.id).expect("task should move to trash");
        assert_eq!(
            set_planned_date(&mut connection, deleted.id, Some("2026-08-10".to_string()))
                .unwrap_err(),
            "task is in trash"
        );

        for recurrence_kind in [RecurrenceKind::Daily, RecurrenceKind::Weekly] {
            let mut recurring = input("recurring");
            recurring.planned_date = Some("2026-08-10".to_string());
            recurring.recurrence_kind = recurrence_kind;
            recurring.recurrence_timezone = Some("UTC".to_string());
            let recurring =
                create(&mut connection, recurring).expect("recurring task should create");
            assert_eq!(
                set_planned_date(
                    &mut connection,
                    recurring.id,
                    Some("2026-08-11".to_string())
                )
                .unwrap_err(),
                "recurring tasks cannot be rescheduled"
            );
        }
    }

    #[test]
    fn activation_only_marks_due_unfired_reminders() {
        let mut connection = connection();

        let mut due_input = input("due");
        due_input.reminder_at_unix_ms = Some(10);
        let due = create(&mut connection, due_input).expect("due task should be created");
        assert!(!mark_reminder_fired_if_due(&connection, due.id, 9)
            .expect("early activation should be checked"));
        assert!(mark_reminder_fired_if_due(&connection, due.id, 10)
            .expect("due activation should be accepted"));
        assert_eq!(
            get(&connection, due.id)
                .expect("task should remain")
                .reminder_fired_at_unix_ms,
            Some(10)
        );
        assert!(!mark_reminder_fired_if_due(&connection, due.id, 11)
            .expect("repeat activation should be ignored"));
        mark_reminder_fired(&connection, due.id, 12)
            .expect("public fired command should remain idempotent");
        assert_eq!(
            get(&connection, due.id)
                .expect("task should remain")
                .reminder_fired_at_unix_ms,
            Some(10)
        );

        let mut future_input = input("future");
        future_input.reminder_at_unix_ms = Some(20);
        let future = create(&mut connection, future_input).expect("future task should be created");
        assert!(!mark_reminder_fired_if_due(&connection, future.id, 10)
            .expect("future activation should be ignored"));

        let mut completed_input = input("completed");
        completed_input.reminder_at_unix_ms = Some(10);
        let completed =
            create(&mut connection, completed_input).expect("completed task should be created");
        set_completed(&mut connection, completed.id, true).expect("task should complete");
        assert!(!mark_reminder_fired_if_due(&connection, completed.id, 10)
            .expect("completed activation should be ignored"));
    }

    #[test]
    fn task_serializes_with_the_public_command_contract() {
        let value = serde_json::to_value(Task {
            id: 1,
            title: "task".to_string(),
            notes: String::new(),
            planned_date: Some("2026-08-07".to_string()),
            due_at_unix_ms: Some(2),
            reminder_at_unix_ms: Some(6),
            reminder_fired_at_unix_ms: Some(7),
            deleted_at_unix_ms: None,
            project_id: Some(8),
            priority: Priority::High,
            recurrence_kind: RecurrenceKind::Weekly,
            recurrence_series_id: Some(1),
            recurrence_source_task_id: Some(0),
            recurrence_timezone: Some("Asia/Shanghai".to_string()),
            recurrence_dst_policy: Some(RECURRENCE_DST_POLICY.to_string()),
            completed_at_unix_ms: Some(3),
            created_at_unix_ms: 4,
            updated_at_unix_ms: 5,
        })
        .expect("task should serialize");
        assert_eq!(
            value,
            json!({
                "id": 1,
                "title": "task",
                "notes": "",
                "plannedDate": "2026-08-07",
                "dueAtUnixMs": 2,
                "reminderAtUnixMs": 6,
                "reminderFiredAtUnixMs": 7,
                "deletedAtUnixMs": null,
                "projectId": 8,
                "priority": "high",
                "recurrenceKind": "weekly",
                "recurrenceSeriesId": 1,
                "recurrenceSourceTaskId": 0,
                "recurrenceTimezone": "Asia/Shanghai",
                "recurrenceDstPolicy": "shift_forward_earlier",
                "completedAtUnixMs": 3,
                "createdAtUnixMs": 4,
                "updatedAtUnixMs": 5
            })
        );
    }

    #[test]
    fn crud_preserves_completion_and_status_changes_are_idempotent() {
        let mut connection = connection();
        let created = create(&mut connection, input("first")).expect("task should be created");
        assert_eq!(created.notes, "");

        let completed =
            set_completed(&mut connection, created.id, true).expect("task should complete");
        let first_completed_at = completed
            .task
            .completed_at_unix_ms
            .expect("completion timestamp should exist");
        let first_completed_updated_at = completed.task.updated_at_unix_ms;
        thread::sleep(Duration::from_millis(2));
        let completed_again = set_completed(&mut connection, created.id, true)
            .expect("repeat completion should succeed");
        assert_eq!(
            completed_again.task.completed_at_unix_ms,
            Some(first_completed_at)
        );
        assert_eq!(
            completed_again.task.updated_at_unix_ms,
            first_completed_updated_at
        );

        let updated = update(
            &mut connection,
            created.id,
            UpdateTaskInput {
                title: "changed".to_string(),
                notes: " details ".to_string(),
                planned_date: Some("2026-08-08".to_string()),
                due_at_unix_ms: Some(42),
                reminder_at_unix_ms: Some(43),
                project_id: None,
                priority: Priority::Medium,
                recurrence_kind: RecurrenceKind::None,
                recurrence_timezone: None,
            },
        )
        .expect("task should be updated");
        assert_eq!(updated.title, "changed");
        assert_eq!(updated.notes, " details ");
        assert_eq!(updated.completed_at_unix_ms, Some(first_completed_at));

        let restored =
            set_completed(&mut connection, created.id, false).expect("task should restore");
        assert_eq!(restored.task.completed_at_unix_ms, None);
        let restored_updated_at = restored.task.updated_at_unix_ms;
        thread::sleep(Duration::from_millis(2));
        let restored_again = set_completed(&mut connection, created.id, false)
            .expect("repeat restore should succeed");
        assert_eq!(restored_again.task.completed_at_unix_ms, None);
        assert_eq!(restored_again.task.updated_at_unix_ms, restored_updated_at);

        delete(&mut connection, created.id).expect("task should delete");
        assert_eq!(
            delete(&mut connection, created.id).unwrap_err(),
            "task already in trash"
        );
    }

    #[test]
    fn trash_lifecycle_hides_restores_and_permanently_deletes_tasks() {
        let mut connection = connection();
        let project_id = create_project(&connection, "Trash", false);
        let mut deleted_input = input("deleted");
        deleted_input.reminder_at_unix_ms = Some(10);
        deleted_input.project_id = Some(project_id);
        let deleted = create(&mut connection, deleted_input).expect("task should be created");
        let retained = create(&mut connection, input("retained")).expect("task should be created");

        delete(&mut connection, deleted.id).expect("task should move to trash");
        assert_eq!(
            list(&connection)
                .expect("active tasks should list")
                .into_iter()
                .map(|task| task.id)
                .collect::<Vec<_>>(),
            vec![retained.id]
        );
        assert_eq!(
            list_missed(&connection, 11).expect("missed reminders should list"),
            Vec::<i64>::new()
        );
        assert_eq!(
            list_deleted(&connection).expect("trash should list"),
            vec![get(&connection, deleted.id).expect("deleted task should remain")]
        );

        assert_eq!(
            restore(&mut connection, retained.id).unwrap_err(),
            "task is not in trash"
        );
        let restored = restore(&mut connection, deleted.id).expect("task should restore");
        assert_eq!(restored.deleted_at_unix_ms, None);
        assert_eq!(restored.project_id, Some(project_id));
        assert!(list(&connection)
            .expect("restored task should list")
            .iter()
            .any(|task| task.id == deleted.id));

        delete(&mut connection, restored.id).expect("restored task should move to trash");
        permanently_delete(&mut connection, restored.id)
            .expect("task should be permanently deleted");
        assert_eq!(get(&connection, restored.id).unwrap_err(), "task not found");
        assert!(list_deleted(&connection)
            .expect("trash should list")
            .is_empty());
    }

    #[test]
    fn recurring_completion_creates_one_successor_and_does_not_recreate_deleted_successors() {
        let mut connection = connection();
        let project_id = create_project(&connection, "Daily", false);
        let mut daily = input("daily");
        daily.planned_date = Some("2026-08-07".to_string());
        daily.project_id = Some(project_id);
        daily.recurrence_kind = RecurrenceKind::Daily;
        daily.recurrence_timezone = Some("America/New_York".to_string());
        let root = create(&mut connection, daily).expect("daily task should create");
        assert_eq!(root.recurrence_series_id, Some(root.id));
        assert_eq!(root.recurrence_source_task_id, None);

        let first_completion =
            set_completed(&mut connection, root.id, true).expect("daily task should complete");
        let successor = first_completion
            .next_task
            .expect("daily completion should materialize one successor");
        assert_eq!(successor.planned_date.as_deref(), Some("2026-08-08"));
        assert_eq!(successor.recurrence_kind, RecurrenceKind::Daily);
        assert_eq!(successor.recurrence_series_id, Some(root.id));
        assert_eq!(successor.recurrence_source_task_id, Some(root.id));
        assert_eq!(successor.project_id, Some(project_id));
        assert_eq!(successor.completed_at_unix_ms, None);
        assert_eq!(successor.deleted_at_unix_ms, None);
        assert_eq!(successor.reminder_fired_at_unix_ms, None);

        let repeated_completion = set_completed(&mut connection, root.id, true)
            .expect("repeat completion should be idempotent");
        assert_eq!(
            repeated_completion.next_task.map(|task| task.id),
            Some(successor.id)
        );
        let task_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
            .expect("task count should query");
        assert_eq!(task_count, 2);

        delete(&mut connection, successor.id).expect("successor should move to trash");
        permanently_delete(&mut connection, successor.id)
            .expect("successor should permanently delete");
        let completion_after_deletion = set_completed(&mut connection, root.id, true)
            .expect("completed root should remain idempotent");
        assert_eq!(completion_after_deletion.next_task, None);
        let task_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
            .expect("task count should query");
        assert_eq!(task_count, 1);
    }

    #[test]
    fn weekly_recurrence_uses_calendar_dates_and_preserves_wall_time() {
        let mut connection = connection();
        let timezone = parse_timezone("America/New_York").expect("timezone should parse");
        let source_due = resolved_epoch_in_timezone(
            timezone,
            NaiveDate::from_ymd_opt(2026, 3, 7)
                .expect("source date")
                .and_hms_opt(2, 30, 0)
                .expect("source time"),
        )
        .expect("source epoch should resolve");
        let mut weekly = input("weekly");
        weekly.planned_date = Some("2026-03-07".to_string());
        weekly.due_at_unix_ms = Some(source_due);
        weekly.recurrence_kind = RecurrenceKind::Weekly;
        weekly.recurrence_timezone = Some("America/New_York".to_string());
        let root = create(&mut connection, weekly).expect("weekly task should create");

        let successor = set_completed(&mut connection, root.id, true)
            .expect("weekly task should complete")
            .next_task
            .expect("weekly completion should create a successor");
        assert_eq!(successor.planned_date.as_deref(), Some("2026-03-14"));
        let successor_due = local_datetime_from_epoch(
            timezone,
            successor.due_at_unix_ms.expect("successor due time"),
        )
        .expect("successor due time should decode");
        assert_eq!(
            successor_due.date(),
            NaiveDate::from_ymd_opt(2026, 3, 14).unwrap()
        );
        assert_eq!(successor_due.time().hour(), 2);
        assert_eq!(successor_due.time().minute(), 30);

        let dst_gap_epoch = resolved_epoch_in_timezone(
            timezone,
            NaiveDate::from_ymd_opt(2026, 3, 8)
                .expect("dst date")
                .and_hms_opt(2, 30, 0)
                .expect("dst time"),
        )
        .expect("DST gap should shift forward");
        let dst_gap_local =
            local_datetime_from_epoch(timezone, dst_gap_epoch).expect("shifted time should decode");
        assert_eq!(dst_gap_local.time().hour(), 3);
        assert_eq!(dst_gap_local.time().minute(), 0);
    }

    #[test]
    fn snooze_and_defer_reset_fired_state_without_changing_other_fields() {
        let mut connection = connection();
        let timezone = parse_timezone("UTC").expect("timezone should parse");
        let due_at_unix_ms = resolved_epoch_in_timezone(
            timezone,
            NaiveDate::from_ymd_opt(2026, 8, 7)
                .expect("due date")
                .and_hms_opt(17, 30, 0)
                .expect("due time"),
        )
        .expect("due epoch should resolve");
        let reminder_at_unix_ms = resolved_epoch_in_timezone(
            timezone,
            NaiveDate::from_ymd_opt(2026, 8, 7)
                .expect("reminder date")
                .and_hms_opt(9, 15, 0)
                .expect("reminder time"),
        )
        .expect("reminder epoch should resolve");
        let mut task_input = input("move me");
        task_input.planned_date = Some("2026-08-07".to_string());
        task_input.due_at_unix_ms = Some(due_at_unix_ms);
        task_input.reminder_at_unix_ms = Some(reminder_at_unix_ms);
        let task = create(&mut connection, task_input).expect("task should create");
        connection
            .execute(
                "UPDATE tasks SET reminder_fired_at_unix_ms = 1 WHERE id = ?1",
                [task.id],
            )
            .expect("fixture should mark reminder fired");

        let deferred = defer_to_tomorrow_at(
            &mut connection,
            task.id,
            "UTC",
            resolved_epoch_in_timezone(
                timezone,
                NaiveDate::from_ymd_opt(2026, 8, 7)
                    .expect("now date")
                    .and_hms_opt(12, 0, 0)
                    .expect("now time"),
            )
            .expect("now epoch should resolve"),
        )
        .expect("task should defer");
        assert_eq!(deferred.planned_date.as_deref(), Some("2026-08-08"));
        assert_eq!(deferred.reminder_fired_at_unix_ms, None);
        assert_eq!(
            local_datetime_from_epoch(timezone, deferred.due_at_unix_ms.unwrap())
                .expect("deferred due should decode")
                .time()
                .hour(),
            17
        );
        assert_eq!(
            local_datetime_from_epoch(timezone, deferred.reminder_at_unix_ms.unwrap())
                .expect("deferred reminder should decode")
                .time()
                .hour(),
            9
        );

        let snoozed_until = now_unix_ms() + 60_000;
        let snoozed = snooze(&mut connection, task.id, snoozed_until).expect("task should snooze");
        assert_eq!(snoozed.reminder_at_unix_ms, Some(snoozed_until));
        assert_eq!(snoozed.reminder_fired_at_unix_ms, None);
        assert_eq!(snoozed.planned_date, deferred.planned_date);
        assert_eq!(snoozed.due_at_unix_ms, deferred.due_at_unix_ms);
    }

    #[test]
    fn recurring_snooze_does_not_change_the_successor_reminder_template() {
        let mut connection = connection();
        let timezone = parse_timezone("UTC").expect("timezone should parse");
        let original_reminder = resolved_epoch_in_timezone(
            timezone,
            NaiveDate::from_ymd_opt(2026, 8, 1)
                .expect("planned date")
                .and_hms_opt(9, 0, 0)
                .expect("reminder time"),
        )
        .expect("reminder should resolve");
        let mut recurring = input("snoozed daily");
        recurring.planned_date = Some("2026-08-01".to_string());
        recurring.reminder_at_unix_ms = Some(original_reminder);
        recurring.recurrence_kind = RecurrenceKind::Daily;
        recurring.recurrence_timezone = Some("UTC".to_string());
        let root = create(&mut connection, recurring).expect("recurring task should create");

        let snoozed_until = now_unix_ms() + 60 * 60 * 1_000;
        let snoozed = snooze(&mut connection, root.id, snoozed_until).expect("task should snooze");
        assert_eq!(snoozed.reminder_at_unix_ms, Some(snoozed_until));
        assert_eq!(
            recurrence_reminder_template(&connection, root.id),
            Ok(Some(Some(original_reminder)))
        );

        let successor = set_completed(&mut connection, root.id, true)
            .expect("completion should create a successor")
            .next_task
            .expect("successor should exist");
        assert_eq!(successor.planned_date.as_deref(), Some("2026-08-02"));
        let successor_reminder = local_datetime_from_epoch(
            timezone,
            successor
                .reminder_at_unix_ms
                .expect("successor reminder should exist"),
        )
        .expect("successor reminder should decode");
        assert_eq!(
            successor_reminder.date(),
            NaiveDate::from_ymd_opt(2026, 8, 2).unwrap()
        );
        assert_eq!(successor_reminder.time().hour(), 9);
        assert_eq!(successor_reminder.time().minute(), 0);
    }

    #[test]
    fn recurring_defer_uses_the_persisted_recurrence_timezone() {
        let mut connection = connection();
        let timezone = parse_timezone("America/New_York").expect("timezone should parse");
        let mut recurring = input("timezone stable");
        recurring.planned_date = Some("2026-08-07".to_string());
        recurring.reminder_at_unix_ms = Some(
            resolved_epoch_in_timezone(
                timezone,
                NaiveDate::from_ymd_opt(2026, 8, 7)
                    .expect("planned date")
                    .and_hms_opt(9, 0, 0)
                    .expect("reminder time"),
            )
            .expect("reminder should resolve"),
        );
        recurring.recurrence_kind = RecurrenceKind::Daily;
        recurring.recurrence_timezone = Some("America/New_York".to_string());
        let root = create(&mut connection, recurring).expect("recurring task should create");

        let now = Utc
            .with_ymd_and_hms(2026, 8, 8, 2, 0, 0)
            .single()
            .expect("UTC instant should resolve")
            .timestamp_millis();
        assert_eq!(
            defer_to_tomorrow_at(&mut connection, root.id, "Asia/Shanghai", now).unwrap_err(),
            "timezone must match the recurring task recurrenceTimezone"
        );
        let deferred = defer_to_tomorrow_at(&mut connection, root.id, "America/New_York", now)
            .expect("recurring task should defer using stored timezone");
        assert_eq!(deferred.planned_date.as_deref(), Some("2026-08-08"));
        let deferred_reminder = local_datetime_from_epoch(
            timezone,
            deferred
                .reminder_at_unix_ms
                .expect("deferred reminder should exist"),
        )
        .expect("deferred reminder should decode");
        assert_eq!(
            deferred_reminder.date(),
            NaiveDate::from_ymd_opt(2026, 8, 8).unwrap()
        );
        assert_eq!(deferred_reminder.time().hour(), 9);

        let successor = set_completed(&mut connection, root.id, true)
            .expect("deferred recurring task should complete")
            .next_task
            .expect("completion should create a successor");
        let successor_reminder = local_datetime_from_epoch(
            timezone,
            successor
                .reminder_at_unix_ms
                .expect("successor reminder should exist"),
        )
        .expect("successor reminder should decode");
        assert_eq!(successor.planned_date.as_deref(), Some("2026-08-09"));
        assert_eq!(
            successor_reminder.date(),
            NaiveDate::from_ymd_opt(2026, 8, 9).unwrap()
        );
        assert_eq!(successor_reminder.time().hour(), 9);
    }

    #[test]
    fn recurrence_can_be_removed_before_successor_exists() {
        let mut connection = connection();
        let mut recurring = input("one time later");
        recurring.planned_date = Some("2026-08-07".to_string());
        recurring.recurrence_kind = RecurrenceKind::Daily;
        recurring.recurrence_timezone = Some("UTC".to_string());
        let created = create(&mut connection, recurring).expect("recurring task should create");

        let updated = update(
            &mut connection,
            created.id,
            UpdateTaskInput {
                title: created.title,
                notes: created.notes,
                planned_date: created.planned_date,
                due_at_unix_ms: created.due_at_unix_ms,
                reminder_at_unix_ms: created.reminder_at_unix_ms,
                project_id: created.project_id,
                priority: created.priority,
                recurrence_kind: RecurrenceKind::None,
                recurrence_timezone: None,
            },
        )
        .expect("recurrence should be removable before a successor exists");
        assert_eq!(updated.recurrence_kind, RecurrenceKind::None);
        assert_eq!(updated.recurrence_series_id, None);
        assert_eq!(updated.recurrence_source_task_id, None);
        assert_eq!(updated.recurrence_timezone, None);
        assert_eq!(updated.recurrence_dst_policy, None);
    }

    #[test]
    fn list_uses_the_required_fixed_sort_order() {
        let connection = connection();
        connection.execute("INSERT INTO tasks (id, title, notes, planned_date, due_at_unix_ms, reminder_at_unix_ms, reminder_fired_at_unix_ms, priority, completed_at_unix_ms, created_at_unix_ms, updated_at_unix_ms) VALUES (1, 'undue-old', '', NULL, NULL, NULL, NULL, 0, NULL, 100, 100), (2, 'due-late', '', NULL, 30, NULL, NULL, 0, NULL, 200, 200), (3, 'due-early', '', NULL, 10, NULL, NULL, 0, NULL, 300, 300), (4, 'undue-new', '', NULL, NULL, NULL, NULL, 0, NULL, 400, 400), (5, 'complete-old', '', NULL, NULL, NULL, NULL, 0, 50, 500, 500), (6, 'complete-new', '', NULL, NULL, NULL, NULL, 0, 60, 600, 600)", []).expect("fixture should insert");
        let ids: Vec<i64> = list(&connection)
            .expect("tasks should list")
            .into_iter()
            .map(|task| task.id)
            .collect();
        assert_eq!(ids, vec![3, 2, 4, 1, 6, 5]);
    }
}
