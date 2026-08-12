use copet_lib::{
    runtime_state::{PetStateId, RuntimeEvent},
    task_notifications::{AttentionKind, TaskNotificationStore, TaskStatus},
};
use serde_json::json;

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
