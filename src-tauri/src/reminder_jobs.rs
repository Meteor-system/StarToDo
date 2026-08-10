use std::{collections::HashSet, sync::Mutex};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use url::Url;

use crate::{reminder_orchestrator::ReminderHost, reminders, tasks};

pub const ACTIVATION_CLAIM_LEASE_MS: i64 = 30_000;
const MAX_ACTIVATION_CONSUMER_ID_LENGTH: usize = 128;
const MAX_ACTIVATION_DELIVERY_ID_LENGTH: usize = 128;
const MIN_ACTIVATION_TOKEN_LENGTH: usize = 16;
const MAX_ACTIVATION_TOKEN_LENGTH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReminderIntent {
    Sync,
    Cancel,
}

impl ReminderIntent {
    fn as_db(self) -> &'static str {
        match self {
            Self::Sync => "sync",
            Self::Cancel => "cancel",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PendingActivation {
    pub id: i64,
    pub task_id: i64,
    pub received_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActivationAckResult {
    pub acknowledged_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReliabilityIncident {
    pub id: i64,
    pub kind: String,
    pub task_id: Option<i64>,
    pub operation: String,
    pub message: String,
    pub status: String,
    pub first_seen_at_unix_ms: i64,
    pub last_seen_at_unix_ms: i64,
    pub occurrence_count: i64,
    pub resolved_at_unix_ms: Option<i64>,
    pub acknowledged_at_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationActivation {
    pub delivery_id: String,
    pub token: String,
}

fn database_error(_: rusqlite::Error) -> String {
    "database operation failed".to_string()
}

fn validate_task_id(task_id: i64) -> Result<(), String> {
    if task_id > 0 {
        Ok(())
    } else {
        Err("task id must be greater than 0".to_string())
    }
}

fn is_ascii_identifier(value: &str, max_length: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn is_base64url_token(value: &str) -> bool {
    is_ascii_identifier(value, MAX_ACTIVATION_TOKEN_LENGTH)
        && value.len() >= MIN_ACTIVATION_TOKEN_LENGTH
        && value.len() % 4 != 1
}

fn validate_consumer_id(consumer_id: &str) -> Result<(), String> {
    if is_ascii_identifier(consumer_id, MAX_ACTIVATION_CONSUMER_ID_LENGTH) {
        Ok(())
    } else {
        Err("activation consumer id is invalid".to_string())
    }
}

pub fn upsert_intent_at(
    connection: &Connection,
    task_id: i64,
    intent: ReminderIntent,
    now_unix_ms: i64,
) -> Result<(), String> {
    validate_task_id(task_id)?;
    connection
        .execute(
            "
            INSERT INTO reminder_sync_jobs (
                task_id, intent, generation, attempt_count, next_attempt_at_unix_ms,
                last_error, updated_at_unix_ms
            ) VALUES (
                ?1,
                ?2,
                COALESCE((
                    SELECT MAX(generation)
                    FROM reminder_deliveries
                    WHERE task_id = ?1
                ), 0) + 1,
                0,
                ?3,
                NULL,
                ?3
            )
            ON CONFLICT(task_id) DO UPDATE SET
                intent = excluded.intent,
                generation = reminder_sync_jobs.generation + 1,
                attempt_count = 0,
                next_attempt_at_unix_ms = excluded.next_attempt_at_unix_ms,
                last_error = NULL,
                updated_at_unix_ms = excluded.updated_at_unix_ms
            ",
            params![task_id, intent.as_db(), now_unix_ms],
        )
        .map(|_| ())
        .map_err(database_error)
}

pub fn upsert_sync_intent_at(
    connection: &Connection,
    task_id: i64,
    now_unix_ms: i64,
) -> Result<(), String> {
    upsert_intent_at(connection, task_id, ReminderIntent::Sync, now_unix_ms)
}

pub fn upsert_cancel_intent_at(
    connection: &Connection,
    task_id: i64,
    now_unix_ms: i64,
) -> Result<(), String> {
    upsert_intent_at(connection, task_id, ReminderIntent::Cancel, now_unix_ms)
}

pub fn enqueue_sync_for_all(connection: &mut Connection, now_unix_ms: i64) -> Result<(), String> {
    let transaction = connection.transaction().map_err(database_error)?;
    let task_ids = {
        let mut statement = transaction
            .prepare("SELECT id FROM tasks ORDER BY id")
            .map_err(database_error)?;
        let task_ids = statement
            .query_map([], |row| row.get::<_, i64>(0))
            .map_err(database_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(database_error)?;
        task_ids
    };
    for task_id in task_ids {
        transaction
            .execute(
                "
                INSERT INTO reminder_sync_jobs (
                    task_id, intent, generation, attempt_count, next_attempt_at_unix_ms,
                    last_error, updated_at_unix_ms
                ) VALUES (
                    ?1,
                    'sync',
                    COALESCE((
                        SELECT MAX(generation)
                        FROM reminder_deliveries
                        WHERE task_id = ?1
                    ), 0),
                    0,
                    ?2,
                    NULL,
                    ?2
                )
                ON CONFLICT(task_id) DO NOTHING
                ",
                params![task_id, now_unix_ms],
            )
            .map_err(database_error)?;
    }
    transaction.commit().map_err(database_error)
}

pub fn parse_notification_activation(url: &Url) -> Option<NotificationActivation> {
    if url.scheme() != "startodo"
        || url.host_str() != Some("reminder")
        || url.path() != "/open"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return None;
    }

    let mut delivery_id = None;
    let mut token = None;
    for query_part in url.query()?.split('&') {
        let (key, value) = query_part.split_once('=')?;
        if value.contains('=') {
            return None;
        }
        match key {
            "delivery" if delivery_id.is_none() => delivery_id = Some(value),
            "token" if token.is_none() => token = Some(value),
            _ => return None,
        }
    }
    let delivery_id = delivery_id?;
    let token = token?;
    if !is_ascii_identifier(delivery_id, MAX_ACTIVATION_DELIVERY_ID_LENGTH)
        || !is_base64url_token(token)
    {
        return None;
    }

    Some(NotificationActivation {
        delivery_id: delivery_id.to_string(),
        token: token.to_string(),
    })
}

pub fn consume_notification_activation(
    connection: &mut Connection,
    activation: &NotificationActivation,
    now_unix_ms: i64,
) -> Result<Option<i64>, String> {
    let transaction = connection.transaction().map_err(database_error)?;
    let delivery = transaction
        .query_row(
            "
            SELECT task_id, reminder_at_unix_ms, state, activation_token_hash
            FROM reminder_deliveries
            WHERE id = ?1
            ",
            [&activation.delivery_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?;
    let Some((task_id, reminder_at_unix_ms, state, stored_hash)) = delivery else {
        return Ok(None);
    };
    if state != "scheduled" || reminder_at_unix_ms > now_unix_ms {
        return Ok(None);
    }

    let supplied_hash = Sha256::digest(activation.token.as_bytes());
    if stored_hash.len() != supplied_hash.len()
        || stored_hash.ct_eq(supplied_hash.as_slice()).unwrap_u8() != 1
    {
        return Ok(None);
    }

    let current_reminder_matches = tasks::find(&transaction, task_id)?
        .is_some_and(|task| task.reminder_at_unix_ms == Some(reminder_at_unix_ms));
    if !current_reminder_matches
        || !tasks::mark_reminder_fired_if_due(&transaction, task_id, now_unix_ms)?
    {
        transaction
            .execute(
                "
                UPDATE reminder_deliveries
                SET state = 'superseded'
                WHERE id = ?1 AND state = 'scheduled'
                ",
                [&activation.delivery_id],
            )
            .map_err(database_error)?;
        transaction.commit().map_err(database_error)?;
        return Ok(None);
    }

    let activated = transaction
        .execute(
            "
            UPDATE reminder_deliveries
            SET state = 'activated', activated_at_unix_ms = ?1
            WHERE id = ?2 AND state = 'scheduled'
            ",
            params![now_unix_ms, activation.delivery_id],
        )
        .map_err(database_error)?;
    if activated != 1 {
        return Err("reminder delivery changed before activation was recorded".to_string());
    }
    transaction
        .execute(
            "
            INSERT INTO activation_inbox (delivery_id, task_id, received_at_unix_ms)
            VALUES (?1, ?2, ?3)
            ",
            params![activation.delivery_id, task_id, now_unix_ms],
        )
        .map_err(database_error)?;
    upsert_cancel_intent_at(&transaction, task_id, now_unix_ms)?;
    transaction.commit().map_err(database_error)?;
    Ok(Some(task_id))
}

pub fn claim_pending_activations(
    connection: &mut Connection,
    consumer_id: &str,
    limit: usize,
    now_unix_ms: i64,
) -> Result<Vec<PendingActivation>, String> {
    validate_consumer_id(consumer_id)?;
    let limit = limit.clamp(1, 100) as i64;
    let transaction = connection.transaction().map_err(database_error)?;
    let candidates = {
        let mut statement = transaction
            .prepare(
                "
                SELECT id, task_id, received_at_unix_ms
                FROM activation_inbox
                WHERE acknowledged_at_unix_ms IS NULL
                  AND (
                    claimed_by IS NULL
                    OR claim_expires_at_unix_ms IS NULL
                    OR claim_expires_at_unix_ms <= ?1
                    OR claimed_by = ?2
                  )
                ORDER BY received_at_unix_ms ASC, id ASC
                LIMIT ?3
                ",
            )
            .map_err(database_error)?;
        let candidates = statement
            .query_map(params![now_unix_ms, consumer_id, limit], |row| {
                Ok(PendingActivation {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    received_at_unix_ms: row.get(2)?,
                })
            })
            .map_err(database_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(database_error)?;
        candidates
    };
    let claim_expires_at_unix_ms = now_unix_ms.saturating_add(ACTIVATION_CLAIM_LEASE_MS);
    let mut claimed = Vec::new();
    for candidate in candidates {
        let changed = transaction
            .execute(
                "
                UPDATE activation_inbox
                SET claimed_by = ?1, claim_expires_at_unix_ms = ?2
                WHERE id = ?3
                  AND acknowledged_at_unix_ms IS NULL
                  AND (
                    claimed_by IS NULL
                    OR claim_expires_at_unix_ms IS NULL
                    OR claim_expires_at_unix_ms <= ?4
                    OR claimed_by = ?1
                  )
                ",
                params![
                    consumer_id,
                    claim_expires_at_unix_ms,
                    candidate.id,
                    now_unix_ms
                ],
            )
            .map_err(database_error)?;
        if changed == 1 {
            claimed.push(candidate);
        }
    }
    transaction.commit().map_err(database_error)?;
    Ok(claimed)
}

pub fn acknowledge_pending_activations(
    connection: &mut Connection,
    consumer_id: &str,
    ids: &[i64],
    now_unix_ms: i64,
) -> Result<ActivationAckResult, String> {
    validate_consumer_id(consumer_id)?;
    let mut unique_ids = Vec::new();
    let mut seen_ids = HashSet::new();
    for id in ids.iter().copied().filter(|id| *id > 0) {
        if seen_ids.insert(id) {
            unique_ids.push(id);
        }
    }
    if unique_ids.is_empty() {
        return Ok(ActivationAckResult {
            acknowledged_ids: Vec::new(),
        });
    }
    let transaction = connection.transaction().map_err(database_error)?;
    let mut acknowledged_ids = Vec::new();
    for id in unique_ids {
        let changed = transaction
            .execute(
                "
                UPDATE activation_inbox
                SET acknowledged_at_unix_ms = ?1
                WHERE id = ?2
                  AND acknowledged_at_unix_ms IS NULL
                  AND claimed_by = ?3
                  AND claim_expires_at_unix_ms > ?1
                ",
                params![now_unix_ms, id, consumer_id],
            )
            .map_err(database_error)?;
        if changed == 1 {
            acknowledged_ids.push(id);
        }
    }
    transaction.commit().map_err(database_error)?;
    Ok(ActivationAckResult { acknowledged_ids })
}

#[derive(Debug, Clone)]
struct ReminderJob {
    task_id: i64,
    intent: ReminderIntent,
    generation: i64,
    attempt_count: i64,
}

#[derive(Debug, Clone)]
struct DeliveryIdentity {
    id: String,
    generation: i64,
    reminder_at_unix_ms: i64,
    activation_uri_hash: Vec<u8>,
}

#[derive(Debug)]
enum PreparedAction {
    Noop,
    InspectSchedule {
        delivery: DeliveryIdentity,
    },
    Schedule {
        delivery: DeliveryIdentity,
        spec: reminders::ReminderSpec,
    },
    Cancel,
}

#[derive(Debug, Default)]
pub struct JobDrainReport {
    pub scheduled: usize,
    pub cancelled: usize,
    pub missed_task_ids: Vec<i64>,
    pub warning: Option<String>,
}

fn intent_from_db(value: String) -> Result<ReminderIntent, String> {
    match value.as_str() {
        "sync" => Ok(ReminderIntent::Sync),
        "cancel" => Ok(ReminderIntent::Cancel),
        _ => Err("reminder job contains an invalid intent".to_string()),
    }
}

fn next_ready_job(
    connection: &Connection,
    now_unix_ms: i64,
) -> Result<Option<ReminderJob>, String> {
    let row = connection
        .query_row(
            "
            SELECT task_id, intent, generation, attempt_count
            FROM reminder_sync_jobs
            WHERE next_attempt_at_unix_ms <= ?1
            ORDER BY next_attempt_at_unix_ms ASC, updated_at_unix_ms ASC, task_id ASC
            LIMIT 1
            ",
            [now_unix_ms],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?;
    row.map(|(task_id, intent, generation, attempt_count)| {
        Ok(ReminderJob {
            task_id,
            intent: intent_from_db(intent)?,
            generation,
            attempt_count,
        })
    })
    .transpose()
}

fn job_is_current(connection: &Connection, job: &ReminderJob) -> Result<bool, String> {
    connection
        .query_row(
            "
            SELECT EXISTS(
                SELECT 1
                FROM reminder_sync_jobs
                WHERE task_id = ?1 AND intent = ?2 AND generation = ?3
            )
            ",
            params![job.task_id, job.intent.as_db(), job.generation],
            |row| row.get(0),
        )
        .map_err(database_error)
}

fn random_urlsafe(byte_count: usize) -> Result<String, String> {
    let mut bytes = vec![0; byte_count];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("secure random generation failed: {error}"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn activation_uri(delivery_id: &str, token: &str) -> String {
    format!("startodo://reminder/open?delivery={delivery_id}&token={token}")
}

fn activation_uri_hash(activation_uri: &str) -> Vec<u8> {
    Sha256::digest(activation_uri.as_bytes()).to_vec()
}

fn create_pending_delivery(
    connection: &Connection,
    job: &ReminderJob,
    task: &tasks::Task,
    now_unix_ms: i64,
) -> Result<(DeliveryIdentity, reminders::ReminderSpec), String> {
    let mut spec = reminders::reminder_spec(task, now_unix_ms)
        .ok_or_else(|| "task does not have a schedulable reminder".to_string())?;
    let delivery_id = random_urlsafe(16)?;
    let token = random_urlsafe(32)?;
    let activation_uri = activation_uri(&delivery_id, &token);
    let token_hash = Sha256::digest(token.as_bytes());
    let activation_uri_hash = activation_uri_hash(&activation_uri);

    connection
        .execute(
            "
            UPDATE reminder_deliveries
            SET state = 'superseded'
            WHERE task_id = ?1
              AND state IN ('pending_schedule', 'scheduled', 'pending_cancel')
            ",
            [task.id],
        )
        .map_err(database_error)?;
    connection
        .execute(
            "
            INSERT INTO reminder_deliveries (
                id, task_id, reminder_at_unix_ms, generation, activation_token_hash,
                activation_uri_hash, state, created_at_unix_ms, scheduled_at_unix_ms,
                activated_at_unix_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending_schedule', ?7, NULL, NULL)
            ",
            params![
                &delivery_id,
                task.id,
                spec.due_at_unix_ms,
                job.generation,
                token_hash.as_slice(),
                &activation_uri_hash,
                now_unix_ms,
            ],
        )
        .map_err(database_error)?;
    spec.activation_uri = Some(activation_uri);
    Ok((
        DeliveryIdentity {
            id: delivery_id,
            generation: job.generation,
            reminder_at_unix_ms: spec.due_at_unix_ms,
            activation_uri_hash,
        },
        spec,
    ))
}

fn task_has_missed_reminder(task: &tasks::Task, now_unix_ms: i64) -> bool {
    task.deleted_at_unix_ms.is_none()
        && task.completed_at_unix_ms.is_none()
        && task.reminder_fired_at_unix_ms.is_none()
        && task
            .reminder_at_unix_ms
            .is_some_and(|reminder_at_unix_ms| reminder_at_unix_ms < now_unix_ms)
}

fn incident_dedupe_key(task_id: i64, operation: &str) -> String {
    format!("reminder-host:{task_id}:{operation}")
}

fn upsert_incident(
    connection: &Connection,
    task_id: i64,
    operation: &str,
    message: &str,
    now_unix_ms: i64,
) -> Result<(), String> {
    connection
        .execute(
            "
            INSERT INTO reliability_incidents (
                kind, dedupe_key, task_id, operation, message, status,
                first_seen_at_unix_ms, last_seen_at_unix_ms, occurrence_count,
                resolved_at_unix_ms, acknowledged_at_unix_ms
            ) VALUES ('reminder_host', ?1, ?2, ?3, ?4, 'open', ?5, ?5, 1, NULL, NULL)
            ON CONFLICT(dedupe_key) DO UPDATE SET
                task_id = excluded.task_id,
                message = excluded.message,
                status = 'open',
                last_seen_at_unix_ms = excluded.last_seen_at_unix_ms,
                occurrence_count = reliability_incidents.occurrence_count + 1,
                resolved_at_unix_ms = NULL,
                acknowledged_at_unix_ms = NULL
            ",
            params![
                incident_dedupe_key(task_id, operation),
                task_id,
                operation,
                message,
                now_unix_ms,
            ],
        )
        .map(|_| ())
        .map_err(database_error)
}

fn resolve_incident(
    connection: &Connection,
    task_id: i64,
    operation: &str,
    now_unix_ms: i64,
) -> Result<(), String> {
    connection
        .execute(
            "
            UPDATE reliability_incidents
            SET status = 'resolved', resolved_at_unix_ms = ?1
            WHERE dedupe_key = ?2 AND status IN ('open', 'acknowledged')
            ",
            params![now_unix_ms, incident_dedupe_key(task_id, operation)],
        )
        .map(|_| ())
        .map_err(database_error)
}

fn mark_live_deliveries_pending_cancel(
    connection: &Connection,
    task_id: i64,
) -> Result<(), String> {
    connection
        .execute(
            "
            UPDATE reminder_deliveries
            SET state = 'pending_cancel'
            WHERE task_id = ?1
              AND state IN ('pending_schedule', 'scheduled')
            ",
            [task_id],
        )
        .map(|_| ())
        .map_err(database_error)
}

fn find_delivery_to_inspect(
    connection: &Connection,
    task_id: i64,
    reminder_at_unix_ms: i64,
    generation: i64,
) -> Result<Option<DeliveryIdentity>, String> {
    connection
        .query_row(
            "
            SELECT id, generation, reminder_at_unix_ms, activation_uri_hash
            FROM reminder_deliveries
            WHERE task_id = ?1
              AND reminder_at_unix_ms = ?2
              AND generation = ?3
              AND state IN ('pending_schedule', 'scheduled')
              AND activation_uri_hash IS NOT NULL
              AND length(activation_uri_hash) = 32
            LIMIT 1
            ",
            params![task_id, reminder_at_unix_ms, generation],
            |row| {
                Ok(DeliveryIdentity {
                    id: row.get(0)?,
                    generation: row.get(1)?,
                    reminder_at_unix_ms: row.get(2)?,
                    activation_uri_hash: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(database_error)
}

fn scheduled_item_matches_delivery(
    item: &reminders::ScheduledReminder,
    task_id: i64,
    delivery: &DeliveryIdentity,
) -> bool {
    if item.tag != reminders::reminder_tag(task_id)
        || item.due_at_unix_ms != delivery.reminder_at_unix_ms
    {
        return false;
    }
    let Some(scheduled_activation_uri) = item.activation_uri.as_deref() else {
        return false;
    };
    let Ok(url) = Url::parse(scheduled_activation_uri) else {
        return false;
    };
    let Some(activation) = parse_notification_activation(&url) else {
        return false;
    };
    activation_uri_hash(&activation_uri(&activation.delivery_id, &activation.token))
        == delivery.activation_uri_hash
}

fn replace_unconfirmed_delivery(
    connection: &mut Connection,
    job: &ReminderJob,
    delivery: &DeliveryIdentity,
    now_unix_ms: i64,
) -> Result<Option<PreparedAction>, String> {
    let transaction = connection.transaction().map_err(database_error)?;
    if !job_is_current(&transaction, job)? {
        transaction.commit().map_err(database_error)?;
        return Ok(None);
    }
    let Some(task) = tasks::find(&transaction, job.task_id)? else {
        transaction.commit().map_err(database_error)?;
        return Ok(None);
    };
    if !reminders::should_schedule(&task, now_unix_ms)
        || task.reminder_at_unix_ms != Some(delivery.reminder_at_unix_ms)
    {
        transaction.commit().map_err(database_error)?;
        return Ok(None);
    }
    let changed = transaction
        .execute(
            "
            UPDATE reminder_deliveries
            SET state = 'superseded'
            WHERE id = ?1
              AND task_id = ?2
              AND generation = ?3
              AND reminder_at_unix_ms = ?4
              AND activation_uri_hash = ?5
              AND state IN ('pending_schedule', 'scheduled')
            ",
            params![
                &delivery.id,
                job.task_id,
                delivery.generation,
                delivery.reminder_at_unix_ms,
                delivery.activation_uri_hash.as_slice(),
            ],
        )
        .map_err(database_error)?;
    if changed != 1 {
        transaction.commit().map_err(database_error)?;
        return Ok(None);
    }
    let (delivery, spec) = create_pending_delivery(&transaction, job, &task, now_unix_ms)?;
    transaction.commit().map_err(database_error)?;
    Ok(Some(PreparedAction::Schedule { delivery, spec }))
}

fn prepare_action(
    connection: &mut Connection,
    job: &ReminderJob,
    now_unix_ms: i64,
) -> Result<PreparedAction, String> {
    let transaction = connection.transaction().map_err(database_error)?;
    if !job_is_current(&transaction, job)? {
        transaction.commit().map_err(database_error)?;
        return Ok(PreparedAction::Noop);
    }

    let action = match job.intent {
        ReminderIntent::Cancel => {
            mark_live_deliveries_pending_cancel(&transaction, job.task_id)?;
            PreparedAction::Cancel
        }
        ReminderIntent::Sync => match tasks::find(&transaction, job.task_id)? {
            Some(task) if reminders::should_schedule(&task, now_unix_ms) => {
                let reminder_at_unix_ms = task
                    .reminder_at_unix_ms
                    .expect("schedulable tasks have a reminder time");
                match find_delivery_to_inspect(
                    &transaction,
                    task.id,
                    reminder_at_unix_ms,
                    job.generation,
                )? {
                    Some(delivery) => PreparedAction::InspectSchedule { delivery },
                    None => {
                        let (delivery, spec) =
                            create_pending_delivery(&transaction, job, &task, now_unix_ms)?;
                        PreparedAction::Schedule { delivery, spec }
                    }
                }
            }
            Some(task) => {
                if task_has_missed_reminder(&task, now_unix_ms) {
                    upsert_incident(
                        &transaction,
                        job.task_id,
                        "expired",
                        "reminder elapsed before a scheduled delivery could be confirmed",
                        now_unix_ms,
                    )?;
                }
                mark_live_deliveries_pending_cancel(&transaction, job.task_id)?;
                PreparedAction::Cancel
            }
            None => {
                mark_live_deliveries_pending_cancel(&transaction, job.task_id)?;
                PreparedAction::Cancel
            }
        },
    };
    transaction.commit().map_err(database_error)?;
    Ok(action)
}

fn retry_delay_unix_ms(attempt_count: i64) -> i64 {
    let exponent = attempt_count.saturating_sub(1).min(6) as u32;
    30_000_i64
        .saturating_mul(1_i64 << exponent)
        .min(30 * 60 * 1_000)
}

fn complete_action(
    connection: &mut Connection,
    job: &ReminderJob,
    action: &PreparedAction,
    now_unix_ms: i64,
) -> Result<bool, String> {
    let transaction = connection.transaction().map_err(database_error)?;
    let is_current = job_is_current(&transaction, job)?;
    match action {
        PreparedAction::Noop => {}
        PreparedAction::InspectSchedule { delivery }
        | PreparedAction::Schedule { delivery, .. }
            if is_current =>
        {
            let changed = transaction
                .execute(
                    "
                    UPDATE reminder_deliveries
                    SET state = 'scheduled',
                        scheduled_at_unix_ms = COALESCE(scheduled_at_unix_ms, ?1)
                    WHERE id = ?2
                      AND task_id = ?3
                      AND generation = ?4
                      AND reminder_at_unix_ms = ?5
                      AND activation_uri_hash = ?6
                      AND state IN ('pending_schedule', 'scheduled')
                    ",
                    params![
                        now_unix_ms,
                        &delivery.id,
                        job.task_id,
                        delivery.generation,
                        delivery.reminder_at_unix_ms,
                        delivery.activation_uri_hash.as_slice(),
                    ],
                )
                .map_err(database_error)?;
            if changed != 1 {
                return Err("reminder delivery changed before schedule confirmation".to_string());
            }
            resolve_incident(&transaction, job.task_id, "schedule", now_unix_ms)?;
            resolve_incident(&transaction, job.task_id, "expired", now_unix_ms)?;
        }
        PreparedAction::Schedule { delivery, .. } => {
            transaction
                .execute(
                    "
                    UPDATE reminder_deliveries
                    SET state = 'superseded'
                    WHERE id = ?1
                      AND task_id = ?2
                      AND generation = ?3
                      AND reminder_at_unix_ms = ?4
                      AND activation_uri_hash = ?5
                      AND state = 'pending_schedule'
                    ",
                    params![
                        &delivery.id,
                        job.task_id,
                        delivery.generation,
                        delivery.reminder_at_unix_ms,
                        delivery.activation_uri_hash.as_slice(),
                    ],
                )
                .map_err(database_error)?;
        }
        PreparedAction::InspectSchedule { .. } => {}
        PreparedAction::Cancel if is_current => {
            transaction
                .execute(
                    "
                    UPDATE reminder_deliveries
                    SET state = 'cancelled'
                    WHERE task_id = ?1 AND state = 'pending_cancel'
                    ",
                    [job.task_id],
                )
                .map_err(database_error)?;
            resolve_incident(&transaction, job.task_id, "cancel", now_unix_ms)?;
        }
        PreparedAction::Cancel => {}
    }
    if is_current {
        transaction
            .execute(
                "
                DELETE FROM reminder_sync_jobs
                WHERE task_id = ?1 AND intent = ?2 AND generation = ?3
                ",
                params![job.task_id, job.intent.as_db(), job.generation],
            )
            .map_err(database_error)?;
    }
    transaction.commit().map_err(database_error)?;
    Ok(is_current)
}

fn fail_action(
    connection: &mut Connection,
    job: &ReminderJob,
    action: &PreparedAction,
    error: &str,
    now_unix_ms: i64,
) -> Result<bool, String> {
    let transaction = connection.transaction().map_err(database_error)?;
    let is_current = job_is_current(&transaction, job)?;
    if is_current {
        let attempt_count = job.attempt_count.saturating_add(1);
        transaction
            .execute(
                "
                UPDATE reminder_sync_jobs
                SET attempt_count = ?1,
                    next_attempt_at_unix_ms = ?2,
                    last_error = ?3,
                    updated_at_unix_ms = ?4
                WHERE task_id = ?5 AND intent = ?6 AND generation = ?7
                ",
                params![
                    attempt_count,
                    now_unix_ms.saturating_add(retry_delay_unix_ms(attempt_count)),
                    error,
                    now_unix_ms,
                    job.task_id,
                    job.intent.as_db(),
                    job.generation,
                ],
            )
            .map_err(database_error)?;
        let operation = match action {
            PreparedAction::InspectSchedule { .. } | PreparedAction::Schedule { .. } => "schedule",
            PreparedAction::Cancel | PreparedAction::Noop => "cancel",
        };
        upsert_incident(&transaction, job.task_id, operation, error, now_unix_ms)?;
    }
    transaction.commit().map_err(database_error)?;
    Ok(is_current)
}

pub fn drain_ready_jobs(
    database: &Mutex<Connection>,
    orchestration_lock: &Mutex<()>,
    host: &dyn ReminderHost,
    now_unix_ms: i64,
) -> Result<JobDrainReport, String> {
    let _orchestration_guard = orchestration_lock
        .lock()
        .map_err(|_| "reminder orchestration lock is poisoned".to_string())?;
    let missed_task_ids = {
        let connection = database
            .lock()
            .map_err(|_| "database lock is poisoned".to_string())?;
        tasks::list_missed(&connection, now_unix_ms)?
    };
    let mut report = JobDrainReport {
        missed_task_ids,
        ..JobDrainReport::default()
    };
    let mut warnings = Vec::new();

    for _ in 0..100 {
        let job = {
            let connection = database
                .lock()
                .map_err(|_| "database lock is poisoned".to_string())?;
            next_ready_job(&connection, now_unix_ms)?
        };
        let Some(job) = job else {
            break;
        };
        let mut action = {
            let mut connection = database
                .lock()
                .map_err(|_| "database lock is poisoned".to_string())?;
            prepare_action(&mut connection, &job, now_unix_ms)?
        };

        if let PreparedAction::InspectSchedule { delivery } = &action {
            match host.list() {
                Ok(items)
                    if items.iter().any(|item| {
                        scheduled_item_matches_delivery(item, job.task_id, delivery)
                    }) =>
                {
                    let mut connection = database
                        .lock()
                        .map_err(|_| "database lock is poisoned".to_string())?;
                    complete_action(&mut connection, &job, &action, now_unix_ms)?;
                    continue;
                }
                Ok(_) => {
                    let replacement = {
                        let mut connection = database
                            .lock()
                            .map_err(|_| "database lock is poisoned".to_string())?;
                        replace_unconfirmed_delivery(&mut connection, &job, delivery, now_unix_ms)?
                    };
                    let Some(replacement) = replacement else {
                        let message = format!(
                            "schedule {}: reminder delivery changed while being inspected",
                            reminders::reminder_tag(job.task_id)
                        );
                        let current = {
                            let mut connection = database
                                .lock()
                                .map_err(|_| "database lock is poisoned".to_string())?;
                            fail_action(&mut connection, &job, &action, &message, now_unix_ms)?
                        };
                        if current {
                            warnings.push(message);
                        }
                        continue;
                    };
                    action = replacement;
                }
                Err(error) => {
                    let message =
                        format!("schedule {}: {error}", reminders::reminder_tag(job.task_id));
                    let current = {
                        let mut connection = database
                            .lock()
                            .map_err(|_| "database lock is poisoned".to_string())?;
                        fail_action(&mut connection, &job, &action, &message, now_unix_ms)?
                    };
                    if current {
                        warnings.push(message);
                    }
                    continue;
                }
            }
        }

        let result = match &action {
            PreparedAction::Noop => Ok(()),
            PreparedAction::InspectSchedule { .. } => Ok(()),
            PreparedAction::Schedule { spec, .. } => host.schedule(spec.clone()),
            PreparedAction::Cancel => host.cancel(&reminders::reminder_tag(job.task_id)),
        };
        match result {
            Ok(()) => {
                let current = {
                    let mut connection = database
                        .lock()
                        .map_err(|_| "database lock is poisoned".to_string())?;
                    complete_action(&mut connection, &job, &action, now_unix_ms)?
                };
                if current {
                    match action {
                        PreparedAction::Schedule { .. } => report.scheduled += 1,
                        PreparedAction::Cancel => report.cancelled += 1,
                        PreparedAction::InspectSchedule { .. } | PreparedAction::Noop => {}
                    }
                }
            }
            Err(error) => {
                let operation = match action {
                    PreparedAction::InspectSchedule { .. } | PreparedAction::Schedule { .. } => {
                        "schedule"
                    }
                    PreparedAction::Cancel | PreparedAction::Noop => "cancel",
                };
                let message = format!(
                    "{operation} {}: {error}",
                    reminders::reminder_tag(job.task_id)
                );
                let current = {
                    let mut connection = database
                        .lock()
                        .map_err(|_| "database lock is poisoned".to_string())?;
                    fail_action(&mut connection, &job, &action, &message, now_unix_ms)?
                };
                if current {
                    warnings.push(message);
                }
            }
        }
    }

    report.warning = (!warnings.is_empty()).then(|| warnings.join("; "));
    Ok(report)
}

pub fn enqueue_cancels_for_orphaned_tags(
    connection: &mut Connection,
    tags: &[String],
    now_unix_ms: i64,
) -> Result<(), String> {
    let transaction = connection.transaction().map_err(database_error)?;
    for tag in tags {
        let Some(task_id) = reminders::task_id_from_reminder_tag(tag) else {
            continue;
        };
        if tasks::find(&transaction, task_id)?.is_none() {
            upsert_cancel_intent_at(&transaction, task_id, now_unix_ms)?;
        }
    }
    transaction.commit().map_err(database_error)
}

pub fn next_retry_at(connection: &Connection) -> Result<Option<i64>, String> {
    connection
        .query_row(
            "SELECT MIN(next_attempt_at_unix_ms) FROM reminder_sync_jobs",
            [],
            |row| row.get(0),
        )
        .map_err(database_error)
}

pub fn list_reliability_incidents(
    connection: &Connection,
    include_resolved: bool,
) -> Result<Vec<ReliabilityIncident>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, kind, task_id, operation, message, status,
                   first_seen_at_unix_ms, last_seen_at_unix_ms, occurrence_count,
                   resolved_at_unix_ms, acknowledged_at_unix_ms
            FROM reliability_incidents
            WHERE ?1 OR status = 'open'
            ORDER BY last_seen_at_unix_ms DESC, id DESC
            ",
        )
        .map_err(database_error)?;
    let incidents = statement
        .query_map([include_resolved], |row| {
            Ok(ReliabilityIncident {
                id: row.get(0)?,
                kind: row.get(1)?,
                task_id: row.get(2)?,
                operation: row.get(3)?,
                message: row.get(4)?,
                status: row.get(5)?,
                first_seen_at_unix_ms: row.get(6)?,
                last_seen_at_unix_ms: row.get(7)?,
                occurrence_count: row.get(8)?,
                resolved_at_unix_ms: row.get(9)?,
                acknowledged_at_unix_ms: row.get(10)?,
            })
        })
        .map_err(database_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(database_error)?;
    Ok(incidents)
}

pub fn acknowledge_reliability_incidents(
    connection: &mut Connection,
    ids: &[i64],
    now_unix_ms: i64,
) -> Result<(), String> {
    let ids = ids
        .iter()
        .copied()
        .filter(|id| *id > 0)
        .collect::<HashSet<_>>();
    if ids.is_empty() {
        return Ok(());
    }
    let transaction = connection.transaction().map_err(database_error)?;
    for id in ids {
        transaction
            .execute(
                "
                UPDATE reliability_incidents
                SET status = 'acknowledged', acknowledged_at_unix_ms = ?1
                WHERE id = ?2 AND status = 'open'
                ",
                params![now_unix_ms, id],
            )
            .map_err(database_error)?;
    }
    transaction.commit().map_err(database_error)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rusqlite::Connection;

    use super::*;
    use crate::{
        apply_migrations,
        reminder_orchestrator::{test_support::FakeReminderHost, ReminderHost},
    };

    fn database() -> Mutex<Connection> {
        let mut connection = Connection::open_in_memory().expect("in-memory database should open");
        apply_migrations(&mut connection).expect("schema should migrate");
        Mutex::new(connection)
    }

    fn create_reminded_task(database: &Mutex<Connection>, reminder_after_ms: i64) -> tasks::Task {
        let reminder_at_unix_ms = tasks::now_unix_ms().saturating_add(reminder_after_ms);
        tasks::create(
            &mut database.lock().expect("database lock"),
            tasks::CreateTaskInput {
                title: "remind me".to_string(),
                notes: None,
                planned_date: None,
                due_at_unix_ms: None,
                reminder_at_unix_ms: Some(reminder_at_unix_ms),
                project_id: None,
                priority: tasks::Priority::None,
                recurrence_kind: tasks::RecurrenceKind::None,
                recurrence_timezone: None,
            },
        )
        .expect("task should create")
    }

    fn scheduled_activation(host: &FakeReminderHost) -> NotificationActivation {
        let activation_uri = host
            .scheduled
            .lock()
            .expect("host schedule lock")
            .first()
            .and_then(|item| item.activation_uri.as_deref())
            .expect("scheduled reminder should include an activation URI")
            .to_string();
        parse_notification_activation(
            &Url::parse(&activation_uri).expect("activation URI should parse"),
        )
        .expect("activation URI should be canonical")
    }

    #[test]
    fn pending_schedule_delivery_cannot_activate() {
        let database = database();
        let task = create_reminded_task(&database, 2_000);
        let now = tasks::now_unix_ms();
        let reminder_at = task
            .reminder_at_unix_ms
            .expect("test task should have a reminder time");
        let job = {
            let connection = database.lock().expect("database lock");
            next_ready_job(&connection, now)
                .expect("job should query")
                .expect("new task should queue a sync job")
        };
        let action = {
            let mut connection = database.lock().expect("database lock");
            prepare_action(&mut connection, &job, now).expect("schedule action should prepare")
        };
        let PreparedAction::Schedule { spec, .. } = action else {
            panic!("new task should prepare a schedule action");
        };
        let activation_uri = spec
            .activation_uri
            .as_deref()
            .expect("pending delivery should have an activation URI");
        let activation = parse_notification_activation(
            &Url::parse(activation_uri).expect("activation URI should parse"),
        )
        .expect("activation URI should be canonical");

        assert_eq!(
            consume_notification_activation(
                &mut database.lock().expect("database lock"),
                &activation,
                reminder_at.saturating_add(1),
            )
            .expect("pending activation should be checked"),
            None
        );
        assert_eq!(
            tasks::find(&database.lock().expect("database lock"), task.id)
                .expect("task should load")
                .expect("task should exist")
                .reminder_fired_at_unix_ms,
            None
        );
        let state: String = database
            .lock()
            .expect("database lock")
            .query_row(
                "SELECT state FROM reminder_deliveries WHERE task_id = ?1",
                [task.id],
                |row| row.get(0),
            )
            .expect("delivery state should query");
        assert_eq!(state, "pending_schedule");
    }

    #[test]
    fn activation_requires_a_valid_current_capability_and_is_consumed_once() {
        let database = database();
        let orchestration_lock = Mutex::new(());
        let host = FakeReminderHost::default();
        let task = create_reminded_task(&database, 2_000);
        let now = tasks::now_unix_ms();
        let reminder_at = task
            .reminder_at_unix_ms
            .expect("test task should have a reminder time");
        let activation_now = reminder_at.saturating_add(1);
        let scheduled = drain_ready_jobs(&database, &orchestration_lock, &host, now)
            .expect("initial delivery should schedule");
        assert_eq!(scheduled.scheduled, 1);
        let activation = scheduled_activation(&host);

        let forged = NotificationActivation {
            delivery_id: activation.delivery_id.clone(),
            token: "AAAAAAAAAAAAAAAA".to_string(),
        };
        assert_eq!(
            consume_notification_activation(
                &mut database.lock().expect("database lock"),
                &forged,
                activation_now,
            )
            .expect("forged activation should be checked"),
            None
        );
        assert_eq!(
            tasks::find(&database.lock().expect("database lock"), task.id)
                .expect("task should load")
                .expect("task should exist")
                .reminder_fired_at_unix_ms,
            None
        );

        assert_eq!(
            consume_notification_activation(
                &mut database.lock().expect("database lock"),
                &activation,
                activation_now,
            )
            .expect("valid activation should consume"),
            Some(task.id)
        );
        assert_eq!(
            consume_notification_activation(
                &mut database.lock().expect("database lock"),
                &activation,
                activation_now.saturating_add(1),
            )
            .expect("replayed activation should be checked"),
            None
        );
        assert_eq!(
            tasks::find(&database.lock().expect("database lock"), task.id)
                .expect("task should load")
                .expect("task should exist")
                .reminder_fired_at_unix_ms,
            Some(activation_now)
        );

        let claimed = claim_pending_activations(
            &mut database.lock().expect("database lock"),
            "ui-one",
            10,
            activation_now.saturating_add(1),
        )
        .expect("first consumer should claim inbox activation");
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].task_id, task.id);
        assert!(claim_pending_activations(
            &mut database.lock().expect("database lock"),
            "ui-two",
            10,
            activation_now.saturating_add(2),
        )
        .expect("second consumer should inspect lease")
        .is_empty());
        let rejected = acknowledge_pending_activations(
            &mut database.lock().expect("database lock"),
            "ui-two",
            &[claimed[0].id],
            activation_now.saturating_add(2),
        )
        .expect("wrong consumer acknowledgement should be ignored");
        assert!(rejected.acknowledged_ids.is_empty());
        assert_eq!(
            claim_pending_activations(
                &mut database.lock().expect("database lock"),
                "ui-one",
                10,
                activation_now.saturating_add(3),
            )
            .expect("owner should retain claim")
            .len(),
            1
        );
        let acknowledged = acknowledge_pending_activations(
            &mut database.lock().expect("database lock"),
            "ui-one",
            &[claimed[0].id],
            activation_now.saturating_add(3),
        )
        .expect("claim owner should acknowledge activation");
        assert_eq!(acknowledged.acknowledged_ids, vec![claimed[0].id]);
        assert!(claim_pending_activations(
            &mut database.lock().expect("database lock"),
            "ui-one",
            10,
            activation_now.saturating_add(4),
        )
        .expect("acknowledged activation should not replay")
        .is_empty());

        let intent: String = database
            .lock()
            .expect("database lock")
            .query_row(
                "SELECT intent FROM reminder_sync_jobs WHERE task_id = ?1",
                [task.id],
                |row| row.get(0),
            )
            .expect("activation should enqueue cancellation");
        assert_eq!(intent, "cancel");
    }

    #[test]
    fn activation_lease_expiry_allows_reclaim_and_rejects_previous_ack() {
        let database = database();
        let orchestration_lock = Mutex::new(());
        let host = FakeReminderHost::default();
        let task = create_reminded_task(&database, 2_000);
        let now = tasks::now_unix_ms();
        let reminder_at = task
            .reminder_at_unix_ms
            .expect("test task should have a reminder time");
        let activation_now = reminder_at.saturating_add(1);
        drain_ready_jobs(&database, &orchestration_lock, &host, now)
            .expect("initial delivery should schedule");
        let activation = scheduled_activation(&host);
        assert_eq!(
            consume_notification_activation(
                &mut database.lock().expect("database lock"),
                &activation,
                activation_now,
            )
            .expect("activation should be consumed"),
            Some(task.id)
        );

        let first_claim_at = activation_now.saturating_add(1);
        let claimed_by_first = claim_pending_activations(
            &mut database.lock().expect("database lock"),
            "ui-first",
            1,
            first_claim_at,
        )
        .expect("first consumer should claim activation");
        assert_eq!(claimed_by_first.len(), 1);
        let activation_id = claimed_by_first[0].id;
        let lease_expiry = first_claim_at.saturating_add(ACTIVATION_CLAIM_LEASE_MS);

        assert!(claim_pending_activations(
            &mut database.lock().expect("database lock"),
            "ui-second",
            1,
            lease_expiry.saturating_sub(1),
        )
        .expect("second consumer should inspect the active lease")
        .is_empty());

        let reclaimed = claim_pending_activations(
            &mut database.lock().expect("database lock"),
            "ui-second",
            1,
            lease_expiry,
        )
        .expect("second consumer should reclaim an expired lease");
        assert_eq!(reclaimed.len(), 1);
        assert_eq!(reclaimed[0].id, activation_id);

        let rejected = acknowledge_pending_activations(
            &mut database.lock().expect("database lock"),
            "ui-first",
            &[activation_id],
            lease_expiry,
        )
        .expect("previous consumer acknowledgement should be safely ignored");
        assert!(rejected.acknowledged_ids.is_empty());
        assert_eq!(
            database
                .lock()
                .expect("database lock")
                .query_row(
                    "SELECT acknowledged_at_unix_ms FROM activation_inbox WHERE id = ?1",
                    [activation_id],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .expect("activation acknowledgement should query"),
            None
        );

        let acknowledged = acknowledge_pending_activations(
            &mut database.lock().expect("database lock"),
            "ui-second",
            &[activation_id],
            lease_expiry.saturating_add(1),
        )
        .expect("new consumer should acknowledge the reclaimed activation");
        assert_eq!(acknowledged.acknowledged_ids, vec![activation_id]);
        assert!(claim_pending_activations(
            &mut database.lock().expect("database lock"),
            "ui-third",
            1,
            lease_expiry.saturating_add(2),
        )
        .expect("acknowledged activation should not replay")
        .is_empty());
    }

    #[test]
    fn stale_delivery_cannot_fire_a_replaced_reminder() {
        let database = database();
        let orchestration_lock = Mutex::new(());
        let host = FakeReminderHost::default();
        let task = create_reminded_task(&database, 2_000);
        let now = tasks::now_unix_ms();
        let stale_reminder_at = task
            .reminder_at_unix_ms
            .expect("test task should have a reminder time");
        let current_reminder_at = stale_reminder_at.saturating_add(2_000);
        drain_ready_jobs(&database, &orchestration_lock, &host, now)
            .expect("initial delivery should schedule");
        let stale_activation = scheduled_activation(&host);

        tasks::update(
            &mut database.lock().expect("database lock"),
            task.id,
            tasks::UpdateTaskInput {
                title: task.title.clone(),
                notes: task.notes.clone(),
                planned_date: task.planned_date.clone(),
                due_at_unix_ms: task.due_at_unix_ms,
                reminder_at_unix_ms: Some(current_reminder_at),
                project_id: task.project_id,
                priority: task.priority,
                recurrence_kind: task.recurrence_kind,
                recurrence_timezone: task.recurrence_timezone.clone(),
            },
        )
        .expect("task should update reminder time");

        assert_eq!(
            consume_notification_activation(
                &mut database.lock().expect("database lock"),
                &stale_activation,
                stale_reminder_at.saturating_add(1),
            )
            .expect("stale activation should be checked"),
            None
        );
        let current = tasks::find(&database.lock().expect("database lock"), task.id)
            .expect("task should load")
            .expect("task should exist");
        assert_eq!(current.reminder_at_unix_ms, Some(current_reminder_at));
        assert_eq!(current.reminder_fired_at_unix_ms, None);
    }

    #[test]
    fn failed_schedule_is_retried_from_the_persistent_outbox() {
        let database = database();
        let orchestration_lock = Mutex::new(());
        let host = FakeReminderHost::default();
        let task = create_reminded_task(&database, 100_000);
        let now = tasks::now_unix_ms();
        *host.fail_schedule.lock().expect("host failure lock") = Some("injected".to_string());

        let failed = drain_ready_jobs(&database, &orchestration_lock, &host, now)
            .expect("failed host call should produce a report");
        assert!(failed.warning.unwrap_or_default().contains("injected"));
        let retry_at = next_retry_at(&database.lock().expect("database lock"))
            .expect("retry time should query")
            .expect("failed job should remain queued");
        assert!(retry_at > now);
        let open_incidents =
            list_reliability_incidents(&database.lock().expect("database lock"), false)
                .expect("incident should list");
        assert_eq!(open_incidents.len(), 1);
        acknowledge_reliability_incidents(
            &mut database.lock().expect("database lock"),
            &[open_incidents[0].id],
            now.saturating_add(1),
        )
        .expect("incident acknowledgement should persist");
        let acknowledged =
            list_reliability_incidents(&database.lock().expect("database lock"), true)
                .expect("acknowledged incident should list");
        assert_eq!(acknowledged[0].status, "acknowledged");

        *host.fail_schedule.lock().expect("host failure lock") = None;
        let recovered = drain_ready_jobs(&database, &orchestration_lock, &host, retry_at)
            .expect("queued job should retry");
        assert_eq!(recovered.scheduled, 1);
        assert_eq!(
            next_retry_at(&database.lock().expect("database lock"))
                .expect("retry time should query"),
            None
        );
        let incidents = list_reliability_incidents(&database.lock().expect("database lock"), true)
            .expect("incident should list");
        assert_eq!(incidents.len(), 1);
        assert_eq!(incidents[0].task_id, Some(task.id));
        assert_eq!(incidents[0].status, "resolved");
    }

    #[test]
    fn restart_after_host_schedule_confirmation_reuses_the_existing_capability() {
        let database = database();
        let orchestration_lock = Mutex::new(());
        let host = FakeReminderHost::default();
        let task = create_reminded_task(&database, 2_000);
        let now = tasks::now_unix_ms();
        let job = {
            let connection = database.lock().expect("database lock");
            next_ready_job(&connection, now)
                .expect("job should query")
                .expect("new task should queue a sync job")
        };
        let action = {
            let mut connection = database.lock().expect("database lock");
            prepare_action(&mut connection, &job, now).expect("schedule action should prepare")
        };
        let PreparedAction::Schedule { spec, .. } = action else {
            panic!("new task should prepare a schedule action");
        };
        host.schedule(spec)
            .expect("host schedule should simulate success before a crash");

        let recovered = drain_ready_jobs(&database, &orchestration_lock, &host, now)
            .expect("restart worker should inspect existing host schedule");
        assert_eq!(recovered.scheduled, 0);
        assert_eq!(
            host.actions(),
            vec![
                crate::reminder_orchestrator::test_support::HostAction::Schedule(
                    reminders::reminder_tag(task.id)
                ),
                crate::reminder_orchestrator::test_support::HostAction::List,
            ]
        );
        let state: String = database
            .lock()
            .expect("database lock")
            .query_row(
                "SELECT state FROM reminder_deliveries WHERE task_id = ?1",
                [task.id],
                |row| row.get(0),
            )
            .expect("delivery state should query");
        assert_eq!(state, "scheduled");
        let pending_jobs: i64 = database
            .lock()
            .expect("database lock")
            .query_row(
                "SELECT COUNT(*) FROM reminder_sync_jobs WHERE task_id = ?1",
                [task.id],
                |row| row.get(0),
            )
            .expect("pending job count should query");
        assert_eq!(pending_jobs, 0);
    }
}
