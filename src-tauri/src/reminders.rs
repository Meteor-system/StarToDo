#[cfg(test)]
use std::collections::{HashMap, HashSet};

use chrono::{Local, TimeZone};
use serde::{Deserialize, Serialize};

use crate::tasks::{Priority, Task};

pub const MAX_SCHEDULE_HORIZON_UNIX_MS: i64 = 365 * 24 * 60 * 60 * 1_000;
const MAX_NOTIFICATION_BODY_CHARS: usize = 1_024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DeliveryCapability {
    OsScheduled,
    SendNow,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReminderSpec {
    pub tag: String,
    pub due_at_unix_ms: i64,
    pub title: String,
    pub body: String,
    pub activation_uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledReminder {
    pub tag: String,
    pub due_at_unix_ms: i64,
    pub activation_uri: Option<String>,
}

#[cfg(test)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReminderDiff {
    pub to_schedule: Vec<ReminderSpec>,
    pub to_cancel: Vec<String>,
}

pub fn reminder_tag(task_id: i64) -> String {
    format!("task-{task_id}")
}

pub fn task_id_from_reminder_tag(tag: &str) -> Option<i64> {
    let value = tag.strip_prefix("task-")?;
    if value.is_empty() || !value.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    let id = value.parse::<i64>().ok().filter(|id| *id > 0)?;
    (reminder_tag(id) == tag).then_some(id)
}

pub fn should_schedule(task: &Task, now_unix_ms: i64) -> bool {
    let Some(reminder_at_unix_ms) = task.reminder_at_unix_ms else {
        return false;
    };
    task.deleted_at_unix_ms.is_none()
        && task.completed_at_unix_ms.is_none()
        && task.reminder_fired_at_unix_ms.is_none()
        && reminder_at_unix_ms > now_unix_ms
        && reminder_at_unix_ms <= now_unix_ms.saturating_add(MAX_SCHEDULE_HORIZON_UNIX_MS)
}

fn format_local_epoch(unix_ms: i64) -> Option<String> {
    Local
        .timestamp_millis_opt(unix_ms)
        .single()
        .map(|date_time| date_time.format("%m-%d %H:%M").to_string())
}

fn priority_label(priority: Priority) -> Option<&'static str> {
    match priority {
        Priority::None => None,
        Priority::Low => Some("低优先级"),
        Priority::Medium => Some("中优先级"),
        Priority::High => Some("高优先级"),
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut output = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        output.pop();
        output.push('…');
    }
    output
}

pub fn notification_body(task: &Task) -> String {
    let mut context = Vec::new();
    if let Some(due_at_unix_ms) = task.due_at_unix_ms {
        if let Some(formatted) = format_local_epoch(due_at_unix_ms) {
            context.push(format!("截止 {formatted}"));
        }
    }
    if let Some(priority) = priority_label(task.priority) {
        context.push(priority.to_string());
    }

    let note = task
        .notes
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    let mut body = context.join(" · ");
    if !note.is_empty() {
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(note);
    }
    if body.is_empty() {
        body = "点击查看任务".to_string();
    }
    truncate_chars(&body, MAX_NOTIFICATION_BODY_CHARS)
}

pub fn reminder_spec(task: &Task, now_unix_ms: i64) -> Option<ReminderSpec> {
    let due_at_unix_ms = task.reminder_at_unix_ms?;
    should_schedule(task, now_unix_ms).then(|| ReminderSpec {
        tag: reminder_tag(task.id),
        due_at_unix_ms,
        title: task.title.clone(),
        body: notification_body(task),
        activation_uri: None,
    })
}

#[cfg(test)]
pub fn diff_scheduled_items(
    tasks: &[Task],
    scheduled: &[ScheduledReminder],
    now_unix_ms: i64,
) -> ReminderDiff {
    let expected = tasks
        .iter()
        .filter_map(|task| reminder_spec(task, now_unix_ms).map(|spec| (spec.tag.clone(), spec)))
        .collect::<HashMap<_, _>>();

    let mut scheduled_by_tag = HashMap::new();
    let mut seen_tags = HashSet::new();
    let mut to_cancel = Vec::new();
    for item in scheduled {
        if !seen_tags.insert(item.tag.clone()) {
            to_cancel.push(item.tag.clone());
            continue;
        }
        scheduled_by_tag.insert(item.tag.clone(), item);
        match expected.get(&item.tag) {
            Some(spec) if spec.due_at_unix_ms == item.due_at_unix_ms => {}
            _ => to_cancel.push(item.tag.clone()),
        }
    }

    let mut to_schedule = expected
        .values()
        .filter(|spec| {
            scheduled_by_tag
                .get(&spec.tag)
                .is_none_or(|item| item.due_at_unix_ms != spec.due_at_unix_ms)
        })
        .cloned()
        .collect::<Vec<_>>();
    to_schedule.sort_by(|left, right| {
        left.due_at_unix_ms
            .cmp(&right.due_at_unix_ms)
            .then_with(|| left.tag.cmp(&right.tag))
    });
    to_cancel.sort();

    ReminderDiff {
        to_schedule,
        to_cancel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> Task {
        Task {
            id: 7,
            title: "发布版本".to_string(),
            notes: "第一行备注\n第二行".to_string(),
            planned_date: None,
            due_at_unix_ms: Some(1_735_000_000_000),
            reminder_at_unix_ms: Some(10_000),
            reminder_fired_at_unix_ms: None,
            deleted_at_unix_ms: None,
            project_id: None,
            priority: Priority::High,
            recurrence_kind: crate::tasks::RecurrenceKind::None,
            recurrence_series_id: None,
            recurrence_source_task_id: None,
            recurrence_timezone: None,
            recurrence_dst_policy: None,
            completed_at_unix_ms: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
        }
    }

    #[test]
    fn schedule_predicate_respects_completion_and_one_year_horizon() {
        let mut value = task();
        assert!(should_schedule(&value, 1));
        assert!(!should_schedule(&value, 10_000));
        value.completed_at_unix_ms = Some(2);
        assert!(!should_schedule(&value, 1));
        value.completed_at_unix_ms = None;
        value.reminder_fired_at_unix_ms = Some(3);
        assert!(!should_schedule(&value, 1));
        value.reminder_fired_at_unix_ms = None;
        value.reminder_at_unix_ms = Some(MAX_SCHEDULE_HORIZON_UNIX_MS + 2);
        assert!(!should_schedule(&value, 1));
    }

    #[test]
    fn reminder_tags_round_trip_and_reject_invalid_values() {
        assert_eq!(task_id_from_reminder_tag(&reminder_tag(7)), Some(7));
        assert_eq!(task_id_from_reminder_tag("task-0"), None);
        assert_eq!(task_id_from_reminder_tag("task-0007"), None);
        assert_eq!(task_id_from_reminder_tag("task-7-x"), None);
        assert_eq!(task_id_from_reminder_tag("other-7"), None);
    }

    #[test]
    fn body_contains_context_and_fallback() {
        let value = task();
        let body = notification_body(&value);
        assert!(body.contains("截止"));
        assert!(body.contains("高优先级"));
        assert!(body.contains("第一行备注"));

        let mut fallback = value;
        fallback.notes.clear();
        fallback.due_at_unix_ms = None;
        fallback.priority = Priority::None;
        assert_eq!(notification_body(&fallback), "点击查看任务");
    }

    #[test]
    fn diff_finds_missing_drifted_and_orphaned_reminders() {
        let mut second = task();
        second.id = 8;
        second.reminder_at_unix_ms = Some(20_000);
        let scheduled = vec![
            ScheduledReminder {
                tag: reminder_tag(7),
                due_at_unix_ms: 9_000,
                activation_uri: None,
            },
            ScheduledReminder {
                tag: "task-99".to_string(),
                due_at_unix_ms: 11_000,
                activation_uri: None,
            },
        ];
        let diff = diff_scheduled_items(&[task(), second], &scheduled, 1);
        assert_eq!(
            diff.to_cancel,
            vec!["task-7".to_string(), "task-99".to_string()]
        );
        assert_eq!(
            diff.to_schedule
                .iter()
                .map(|item| item.tag.as_str())
                .collect::<Vec<_>>(),
            vec!["task-7", "task-8"]
        );
    }
}
