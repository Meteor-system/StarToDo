#[cfg(test)]
use std::sync::Mutex;

#[cfg(test)]
use rusqlite::Connection;

use crate::reminders;
#[cfg(test)]
use crate::tasks;

/// The synchronous boundary around the Windows notification sidecar. Keeping this
/// trait independent of `AppHandle` makes the reconciliation policy testable.
pub trait ReminderHost: Send + Sync {
    fn list(&self) -> Result<Vec<reminders::ScheduledReminder>, String>;
    fn schedule(&self, spec: reminders::ReminderSpec) -> Result<(), String>;
    fn cancel(&self, tag: &str) -> Result<(), String>;
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestrationReport {
    pub scheduled: usize,
    pub cancelled: usize,
    pub missed_task_ids: Vec<i64>,
    pub warning: Option<String>,
}

#[cfg(test)]
/// Serializes the complete list/diff/cancel/schedule pass. The database mutex is
/// intentionally released before every host invocation; a later mutation waits on
/// this lock and then performs a fresh pass from the committed database state.
pub fn reconcile(
    database: &Mutex<Connection>,
    orchestration_lock: &Mutex<()>,
    host: &dyn ReminderHost,
    now_unix_ms: i64,
) -> Result<OrchestrationReport, String> {
    let _orchestration_guard = orchestration_lock
        .lock()
        .map_err(|_| "reminder orchestration lock is poisoned".to_string())?;

    let (task_list, missed_task_ids) = {
        let connection = database
            .lock()
            .map_err(|_| "database lock is poisoned".to_string())?;
        (
            tasks::list(&connection)?,
            tasks::list_missed(&connection, now_unix_ms)?,
        )
    };

    let scheduled = match host.list() {
        Ok(items) => items,
        Err(error) => {
            return Ok(OrchestrationReport {
                scheduled: 0,
                cancelled: 0,
                missed_task_ids,
                warning: Some(format!("reminder reconciliation failed: {error}")),
            });
        }
    };

    let diff = reminders::diff_scheduled_items(&task_list, &scheduled, now_unix_ms);
    let mut scheduled_count = 0;
    let mut cancelled_count = 0;
    let mut warnings = Vec::new();

    for tag in diff.to_cancel {
        match host.cancel(&tag) {
            Ok(()) => cancelled_count += 1,
            Err(error) => warnings.push(format!("cancel {tag}: {error}")),
        }
    }
    for spec in diff.to_schedule {
        let tag = spec.tag.clone();
        match host.schedule(spec) {
            Ok(()) => scheduled_count += 1,
            Err(error) => warnings.push(format!("schedule {tag}: {error}")),
        }
    }

    Ok(OrchestrationReport {
        scheduled: scheduled_count,
        cancelled: cancelled_count,
        missed_task_ids,
        warning: (!warnings.is_empty()).then(|| warnings.join("; ")),
    })
}

#[cfg(test)]
/// Synchronizes one task from the latest committed database state.
///
/// The database guard is released before the host side effect so a mutation can
/// commit while an older orchestration is waiting on the host; its follow-up pass
/// will then read and apply the newer state.
pub fn sync_latest_task_reminder(
    database: &Mutex<Connection>,
    orchestration_lock: &Mutex<()>,
    host: &dyn ReminderHost,
    task_id: i64,
    now_unix_ms: i64,
) -> Result<Option<String>, String> {
    let _orchestration_guard = orchestration_lock
        .lock()
        .map_err(|_| "reminder orchestration lock is poisoned".to_string())?;

    let task = {
        let connection = database
            .lock()
            .map_err(|_| "database lock is poisoned".to_string())?;
        tasks::find(&connection, task_id)?
    };
    let tag = reminders::reminder_tag(task_id);
    let spec = task
        .as_ref()
        .and_then(|task| reminders::reminder_spec(task, now_unix_ms));

    match spec {
        Some(spec) => Ok(host.schedule(spec).err()),
        None => Ok(host.cancel(&tag).err()),
    }
}

#[cfg(test)]
pub mod test_support {
    use std::sync::{Arc, Barrier, Mutex};

    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum HostAction {
        List,
        Schedule(String),
        Cancel(String),
    }

    #[derive(Default)]
    pub struct FakeReminderHost {
        pub scheduled: Mutex<Vec<reminders::ScheduledReminder>>,
        pub actions: Mutex<Vec<HostAction>>,
        pub fail_list: Mutex<Option<String>>,
        pub fail_schedule: Mutex<Option<String>>,
        pub fail_cancel: Mutex<Option<String>>,
        pub list_started_barrier: Mutex<Option<Arc<Barrier>>>,
        pub list_continue_barrier: Mutex<Option<Arc<Barrier>>>,
    }

    impl FakeReminderHost {
        pub fn set_list_barriers(&self, started: Arc<Barrier>, continue_after: Arc<Barrier>) {
            *self
                .list_started_barrier
                .lock()
                .expect("fake host barrier lock") = Some(started);
            *self
                .list_continue_barrier
                .lock()
                .expect("fake host barrier lock") = Some(continue_after);
        }

        pub fn actions(&self) -> Vec<HostAction> {
            self.actions.lock().expect("fake host action lock").clone()
        }
    }

    impl ReminderHost for FakeReminderHost {
        fn list(&self) -> Result<Vec<reminders::ScheduledReminder>, String> {
            self.actions
                .lock()
                .expect("fake host action lock")
                .push(HostAction::List);
            let started = self
                .list_started_barrier
                .lock()
                .expect("fake host barrier lock")
                .take();
            let continue_after = self
                .list_continue_barrier
                .lock()
                .expect("fake host barrier lock")
                .take();
            if let Some(barrier) = started {
                barrier.wait();
            }
            if let Some(barrier) = continue_after {
                barrier.wait();
            }
            if let Some(error) = self
                .fail_list
                .lock()
                .expect("fake host failure lock")
                .clone()
            {
                return Err(error);
            }
            Ok(self
                .scheduled
                .lock()
                .expect("fake host schedule lock")
                .clone())
        }

        fn schedule(&self, spec: reminders::ReminderSpec) -> Result<(), String> {
            self.actions
                .lock()
                .expect("fake host action lock")
                .push(HostAction::Schedule(spec.tag.clone()));
            if let Some(error) = self
                .fail_schedule
                .lock()
                .expect("fake host failure lock")
                .clone()
            {
                return Err(error);
            }
            self.scheduled
                .lock()
                .expect("fake host schedule lock")
                .retain(|item| item.tag != spec.tag);
            self.scheduled
                .lock()
                .expect("fake host schedule lock")
                .push(reminders::ScheduledReminder {
                    tag: spec.tag,
                    due_at_unix_ms: spec.due_at_unix_ms,
                    activation_uri: spec.activation_uri,
                });
            Ok(())
        }

        fn cancel(&self, tag: &str) -> Result<(), String> {
            self.actions
                .lock()
                .expect("fake host action lock")
                .push(HostAction::Cancel(tag.to_string()));
            if let Some(error) = self
                .fail_cancel
                .lock()
                .expect("fake host failure lock")
                .clone()
            {
                return Err(error);
            }
            self.scheduled
                .lock()
                .expect("fake host schedule lock")
                .retain(|item| item.tag != tag);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Barrier, Mutex},
        thread,
    };

    use rusqlite::Connection;

    use super::{
        reconcile,
        test_support::{FakeReminderHost, HostAction},
    };
    use crate::{apply_migrations, reminders, tasks};

    const NOW: i64 = 1_000;

    fn database() -> Mutex<Connection> {
        let mut connection = Connection::open_in_memory().expect("in-memory database should open");
        apply_migrations(&mut connection).expect("schema should migrate");
        Mutex::new(connection)
    }

    fn create_reminded_task(database: &Mutex<Connection>, reminder_at_unix_ms: i64) -> i64 {
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
        .id
    }

    fn update_reminder(database: &Mutex<Connection>, id: i64, reminder_at_unix_ms: Option<i64>) {
        tasks::update(
            &mut database.lock().expect("database lock"),
            id,
            tasks::UpdateTaskInput {
                title: "remind me".to_string(),
                notes: String::new(),
                planned_date: None,
                due_at_unix_ms: None,
                reminder_at_unix_ms,
                project_id: None,
                priority: tasks::Priority::None,
                recurrence_kind: tasks::RecurrenceKind::None,
                recurrence_timezone: None,
            },
        )
        .expect("task should update");
    }

    #[test]
    fn update_then_update_converges_to_the_latest_reminder() {
        let database = database();
        let lock = Mutex::new(());
        let host = FakeReminderHost::default();
        let id = create_reminded_task(&database, 2_000);
        reconcile(&database, &lock, &host, NOW).expect("first reconcile");
        update_reminder(&database, id, Some(3_000));
        reconcile(&database, &lock, &host, NOW).expect("second reconcile");
        update_reminder(&database, id, Some(4_000));
        reconcile(&database, &lock, &host, NOW).expect("third reconcile");

        assert_eq!(
            *host.scheduled.lock().expect("host schedule lock"),
            vec![reminders::ScheduledReminder {
                tag: reminders::reminder_tag(id),
                due_at_unix_ms: 4_000,
                activation_uri: None,
            }]
        );
    }

    #[test]
    fn update_then_delete_cancels_the_latest_scheduled_reminder() {
        let database = database();
        let lock = Mutex::new(());
        let host = FakeReminderHost::default();
        let id = create_reminded_task(&database, 2_000);
        reconcile(&database, &lock, &host, NOW).expect("first reconcile");
        update_reminder(&database, id, Some(3_000));
        reconcile(&database, &lock, &host, NOW).expect("second reconcile");
        tasks::delete(&mut database.lock().expect("database lock"), id)
            .expect("task should delete");
        reconcile(&database, &lock, &host, NOW).expect("delete reconcile");

        assert!(host
            .scheduled
            .lock()
            .expect("host schedule lock")
            .is_empty());
        assert!(host
            .actions()
            .contains(&HostAction::Cancel(reminders::reminder_tag(id))));
    }

    #[test]
    fn single_task_sync_uses_latest_state_and_cancels_after_fired() {
        let database = database();
        let lock = Mutex::new(());
        let host = FakeReminderHost::default();
        let id = create_reminded_task(&database, 2_000);

        assert_eq!(
            super::sync_latest_task_reminder(&database, &lock, &host, id, NOW).expect("schedule"),
            None
        );
        assert!(tasks::mark_reminder_fired_if_due(
            &database.lock().expect("database lock"),
            id,
            2_000
        )
        .expect("first fired mark"));
        assert_eq!(
            super::sync_latest_task_reminder(&database, &lock, &host, id, 2_001).expect("cancel"),
            None
        );

        assert!(host
            .scheduled
            .lock()
            .expect("host schedule lock")
            .is_empty());
        assert_eq!(
            host.actions(),
            vec![
                HostAction::Schedule(reminders::reminder_tag(id)),
                HostAction::Cancel(reminders::reminder_tag(id))
            ]
        );
    }

    #[test]
    fn single_task_sync_cancels_after_permanent_delete() {
        let database = database();
        let lock = Mutex::new(());
        let host = FakeReminderHost::default();
        let id = create_reminded_task(&database, 2_000);

        super::sync_latest_task_reminder(&database, &lock, &host, id, NOW).expect("schedule");
        tasks::delete(&mut database.lock().expect("database lock"), id).expect("delete");
        tasks::permanently_delete(&mut database.lock().expect("database lock"), id)
            .expect("permanent delete");
        super::sync_latest_task_reminder(&database, &lock, &host, id, NOW)
            .expect("cancel missing task");

        assert!(host
            .scheduled
            .lock()
            .expect("host schedule lock")
            .is_empty());
        assert_eq!(
            host.actions(),
            vec![
                HostAction::Schedule(reminders::reminder_tag(id)),
                HostAction::Cancel(reminders::reminder_tag(id))
            ]
        );
    }

    #[test]
    fn failure_then_next_reconcile_converges() {
        let database = database();
        let lock = Mutex::new(());
        let host = FakeReminderHost::default();
        let id = create_reminded_task(&database, 2_000);
        *host.fail_schedule.lock().expect("host failure lock") = Some("injected".to_string());
        let failed =
            reconcile(&database, &lock, &host, NOW).expect("failed reconcile reports warning");
        assert!(failed.warning.unwrap_or_default().contains("injected"));
        assert!(host
            .scheduled
            .lock()
            .expect("host schedule lock")
            .is_empty());

        *host.fail_schedule.lock().expect("host failure lock") = None;
        reconcile(&database, &lock, &host, NOW).expect("retry reconcile");
        assert_eq!(
            host.scheduled
                .lock()
                .expect("host schedule lock")
                .as_slice(),
            [reminders::ScheduledReminder {
                tag: reminders::reminder_tag(id),
                due_at_unix_ms: 2_000,
                activation_uri: None,
            }]
        );
    }

    #[test]
    fn fired_reminder_has_no_follow_up_host_actions() {
        let database = database();
        let lock = Mutex::new(());
        let host = FakeReminderHost::default();
        let id = create_reminded_task(&database, NOW);
        assert!(tasks::mark_reminder_fired_if_due(
            &database.lock().expect("database lock"),
            id,
            NOW
        )
        .expect("first fired mark"));
        assert!(!tasks::mark_reminder_fired_if_due(
            &database.lock().expect("database lock"),
            id,
            NOW + 1
        )
        .expect("repeated fired mark"));

        reconcile(&database, &lock, &host, NOW + 1).expect("reconcile fired task");
        assert_eq!(host.actions(), vec![HostAction::List]);
    }

    #[test]
    fn mutation_after_blocked_reconcile_runs_a_fresh_latest_state_pass() {
        let database = Arc::new(database());
        let lock = Arc::new(Mutex::new(()));
        let host = Arc::new(FakeReminderHost::default());
        let id = create_reminded_task(&database, 2_000);
        let started = Arc::new(Barrier::new(2));
        let continue_after = Arc::new(Barrier::new(2));
        host.set_list_barriers(started.clone(), continue_after.clone());

        let database_for_first = database.clone();
        let lock_for_first = lock.clone();
        let host_for_first = host.clone();
        let first = thread::spawn(move || {
            reconcile(
                &database_for_first,
                &lock_for_first,
                host_for_first.as_ref(),
                NOW,
            )
            .expect("first reconcile")
        });
        started.wait();
        update_reminder(&database, id, Some(3_000));

        let database_for_second = database.clone();
        let lock_for_second = lock.clone();
        let host_for_second = host.clone();
        let second = thread::spawn(move || {
            reconcile(
                &database_for_second,
                &lock_for_second,
                host_for_second.as_ref(),
                NOW,
            )
            .expect("second reconcile")
        });
        continue_after.wait();
        first.join().expect("first thread should finish");
        second.join().expect("second thread should finish");

        assert_eq!(
            host.scheduled
                .lock()
                .expect("host schedule lock")
                .as_slice(),
            [reminders::ScheduledReminder {
                tag: reminders::reminder_tag(id),
                due_at_unix_ms: 3_000,
                activation_uri: None,
            }]
        );
    }
}
