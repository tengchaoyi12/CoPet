use copet_lib::{
    runtime_state::{PetStateId, RuntimeEvent},
    task_actions::{TaskAction, TaskActionDecision, TaskActionState},
    task_notifications::{AttentionKind, TaskNotificationStore, TaskStatus},
};
use serde_json::json;
use std::fs;

fn event(
    kind: &str,
    session_id: Option<&str>,
    turn_id: Option<&str>,
    title: Option<&str>,
) -> RuntimeEvent {
    serde_json::from_value(json!({
        "agent": "codex",
        "kind": kind,
        "sessionId": session_id,
        "turnId": turn_id,
        "taskTitle": title,
        "summary": (kind == "session.stop").then_some("任务已完成")
    }))
    .unwrap()
}

#[test]
fn completed_turn_is_notified_once() {
    let mut store = TaskNotificationStore::default();
    store.apply(
        event(
            "user.prompt",
            Some("thread-1"),
            Some("turn-1"),
            Some("修复登录"),
        ),
        100,
    );

    let first = store.apply(
        event("session.stop", Some("thread-1"), Some("turn-1"), None),
        200,
    );
    let duplicate = store.apply(
        event("session.stop", Some("thread-1"), Some("turn-1"), None),
        300,
    );

    assert_eq!(first.attention.unwrap().kind, AttentionKind::Completed);
    assert!(duplicate.attention.is_none());
    assert_eq!(store.visible().len(), 1);
    assert_eq!(store.visible()[0].title.as_deref(), Some("修复登录"));
    assert_eq!(store.visible()[0].summary.as_deref(), Some("任务已完成"));
}

#[test]
fn priority_is_waiting_failed_completed_running_idle() {
    let mut store = TaskNotificationStore::default();
    store.apply(
        event("user.prompt", Some("thread-a"), Some("turn-a"), None),
        100,
    );
    store.apply(
        event("user.prompt", Some("thread-b"), Some("turn-b"), None),
        110,
    );
    store.apply(
        event("session.stop", Some("thread-b"), Some("turn-b"), None),
        120,
    );
    store.apply(
        event("user.prompt", Some("thread-c"), Some("turn-c"), None),
        130,
    );
    store.apply(
        event("session.error", Some("thread-c"), Some("turn-c"), None),
        140,
    );
    store.apply(
        event("user.prompt", Some("thread-d"), Some("turn-d"), None),
        150,
    );
    store.apply(
        event("permission.waiting", Some("thread-d"), Some("turn-d"), None),
        160,
    );

    assert_eq!(store.dominant_status(), Some(TaskStatus::Waiting));
    assert_eq!(store.dominant_pet_state(), Some(PetStateId::Waiting));

    store.mark_read("codex:thread-d:turn-d");
    assert_eq!(store.dominant_status(), Some(TaskStatus::Failed));
    store.mark_read("codex:thread-c:turn-c");
    assert_eq!(store.dominant_status(), Some(TaskStatus::Completed));
    store.mark_read("codex:thread-b:turn-b");
    assert_eq!(store.dominant_status(), Some(TaskStatus::Running));
}

#[test]
fn parallel_turns_remain_distinct_and_sorted_by_recency() {
    let mut store = TaskNotificationStore::default();
    store.apply(
        event("session.stop", Some("thread-1"), Some("turn-1"), None),
        100,
    );
    store.apply(
        event("session.stop", Some("thread-2"), Some("turn-2"), None),
        200,
    );

    let visible = store.visible();
    assert_eq!(visible.len(), 2);
    assert_eq!(visible[0].id, "codex:thread-2:turn-2");
    assert_eq!(visible[1].id, "codex:thread-1:turn-1");
}

#[test]
fn missing_turn_id_uses_stable_session_fallback_key() {
    let mut store = TaskNotificationStore::default();
    let first = store.apply(event("session.stop", Some("thread-1"), None, None), 100);
    let duplicate = store.apply(event("session.stop", Some("thread-1"), None, None), 200);

    assert_eq!(first.attention.unwrap().id, "codex:thread-1:session");
    assert!(duplicate.attention.is_none());
    assert_eq!(store.visible().len(), 1);
}

#[test]
fn dismiss_removes_only_the_selected_task() {
    let mut store = TaskNotificationStore::default();
    store.apply(
        event("session.stop", Some("thread-1"), Some("turn-1"), None),
        100,
    );
    store.apply(
        event("session.stop", Some("thread-2"), Some("turn-2"), None),
        200,
    );

    store.dismiss("codex:thread-1:turn-1");

    let visible = store.visible();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id, "codex:thread-2:turn-2");
}

#[test]
fn reload_restores_unread_notification_without_replaying_attention() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("task-notifications.json");
    let mut store = TaskNotificationStore::default();
    store.apply(
        event("session.stop", Some("thread-1"), Some("turn-1"), None),
        100,
    );
    store.save(&path, 100).unwrap();

    let mut restored = TaskNotificationStore::load(&path, 200).unwrap();

    assert_eq!(restored.visible().len(), 1);
    let duplicate = restored.apply(
        event("session.stop", Some("thread-1"), Some("turn-1"), None),
        300,
    );
    assert!(duplicate.attention.is_none());
}

#[test]
fn corrupt_persistence_file_recovers_as_empty_store() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("task-notifications.json");
    fs::write(&path, "not-json").unwrap();

    let restored = TaskNotificationStore::load(&path, 100).unwrap();

    assert!(restored.visible().is_empty());
}

#[test]
fn unreadable_persistence_path_recovers_as_empty_store() {
    let temp = tempfile::tempdir().unwrap();

    let restored = TaskNotificationStore::load(temp.path(), 100).unwrap();

    assert!(restored.visible().is_empty());
}

#[test]
fn read_and_dismissed_notifications_are_not_restored() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("task-notifications.json");
    let mut store = TaskNotificationStore::default();
    store.apply(
        event("session.stop", Some("thread-1"), Some("turn-1"), None),
        100,
    );
    store.apply(
        event("session.error", Some("thread-2"), Some("turn-2"), None),
        200,
    );
    store.mark_read("codex:thread-1:turn-1");
    store.dismiss("codex:thread-2:turn-2");
    store.save(&path, 300).unwrap();

    let restored = TaskNotificationStore::load(&path, 400).unwrap();

    assert!(restored.visible().is_empty());
}

#[test]
fn persistence_keeps_only_the_newest_one_hundred_notifications() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("task-notifications.json");
    let mut store = TaskNotificationStore::default();
    for index in 0..105 {
        store.apply(
            event(
                "session.stop",
                Some("thread"),
                Some(&format!("turn-{index}")),
                None,
            ),
            index,
        );
    }
    store.save(&path, 105).unwrap();

    let restored = TaskNotificationStore::load(&path, 105).unwrap();

    let visible = restored.visible();
    assert_eq!(visible.len(), 100);
    assert_eq!(visible[0].id, "codex:thread:turn-104");
    assert!(!visible.iter().any(|task| task.id == "codex:thread:turn-0"));
}

#[test]
fn persistence_discards_notifications_older_than_thirty_days() {
    const DAY_MS: u64 = 24 * 60 * 60 * 1_000;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("task-notifications.json");
    let mut store = TaskNotificationStore::default();
    store.apply(event("session.stop", Some("old"), Some("turn"), None), 0);
    store.apply(
        event("session.stop", Some("recent"), Some("turn"), None),
        31 * DAY_MS,
    );
    store.save(&path, 31 * DAY_MS).unwrap();

    let restored = TaskNotificationStore::load(&path, 31 * DAY_MS).unwrap();

    assert_eq!(restored.visible().len(), 1);
    assert_eq!(restored.visible()[0].id, "codex:recent:turn");
}

#[test]
fn clear_completed_removes_only_completed_notifications() {
    let mut store = TaskNotificationStore::default();
    store.apply(event("session.stop", Some("done-a"), None, None), 100);
    store.apply(event("session.stop", Some("done-b"), None, None), 110);
    store.apply(
        event("permission.waiting", Some("waiting"), None, None),
        120,
    );
    store.apply(event("session.error", Some("failed"), None, None), 130);

    assert_eq!(store.clear_completed(), 2);

    let visible = store.visible();
    assert_eq!(visible.len(), 2);
    assert!(visible
        .iter()
        .all(|item| item.status != TaskStatus::Completed));
    assert!(visible
        .iter()
        .any(|item| item.status == TaskStatus::Waiting));
    assert!(visible.iter().any(|item| item.status == TaskStatus::Failed));
}

#[test]
fn clear_completed_is_idempotent_without_completed_notifications() {
    let mut store = TaskNotificationStore::default();
    store.apply(
        event("permission.waiting", Some("waiting"), None, None),
        100,
    );
    store.apply(event("session.error", Some("failed"), None, None), 110);
    let before = store
        .visible()
        .into_iter()
        .map(|item| (item.id.clone(), item.status))
        .collect::<Vec<_>>();

    assert_eq!(store.clear_completed(), 0);

    let after = store
        .visible()
        .into_iter()
        .map(|item| (item.id.clone(), item.status))
        .collect::<Vec<_>>();
    assert_eq!(after, before);
}

#[test]
fn restored_notification_does_not_restore_executable_action() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("task-notifications.json");
    let mut store = TaskNotificationStore::default();
    store.apply(
        event("permission.waiting", Some("thread-1"), Some("turn-1"), None),
        100,
    );
    assert!(store.attach_action(
        "codex:thread-1:turn-1",
        TaskAction::permission_once(
            "action-1",
            "运行测试",
            "Bash",
            Some("pnpm test"),
            Some("/repo"),
            600_000,
            true,
        ),
    ));
    store.save(&path, 200).unwrap();

    let restored = TaskNotificationStore::load(&path, 300).unwrap();
    let action = restored.visible()[0].action.as_ref().unwrap();

    assert_eq!(action.state, TaskActionState::Expired);
    assert!(!action.quick_action_allowed);
}

#[test]
fn resolved_continue_action_transitions_only_matching_notification_to_running() {
    let mut store = TaskNotificationStore::default();
    store.apply(
        event("session.waiting", Some("thread-1"), Some("turn-1"), None),
        100,
    );
    store.apply(
        event("permission.waiting", Some("thread-2"), Some("turn-2"), None),
        100,
    );
    store.attach_action(
        "codex:thread-1:turn-1",
        TaskAction::continue_once("continue-1", "继续", 1_000),
    );
    store.attach_action(
        "codex:thread-2:turn-2",
        TaskAction::permission_once(
            "permission-2",
            "运行测试",
            "Bash",
            Some("pnpm test"),
            Some("/repo"),
            1_000,
            true,
        ),
    );

    store
        .transition_action("continue-1", TaskActionDecision::ContinueOnce)
        .unwrap();

    let first = store.get("codex:thread-1:turn-1").unwrap();
    let second = store.get("codex:thread-2:turn-2").unwrap();
    assert_eq!(first.status, TaskStatus::Running);
    assert!(first.action.is_none());
    assert_eq!(second.status, TaskStatus::Waiting);
    assert_eq!(second.action.as_ref().unwrap().id, "permission-2");
}
