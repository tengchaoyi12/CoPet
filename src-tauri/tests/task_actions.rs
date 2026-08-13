use copet_lib::task_actions::{
    allows_quick_continue, allows_quick_permission, redact_command_for_display, ActionRegistry,
    TaskAction, TaskActionDecision, TaskActionKind, TaskActionState, WaitDecision, CONTINUE_MARKER,
};
use std::{sync::Arc, thread, time::Duration};

#[test]
fn continue_marker_creates_pending_continue_action() {
    let message = format!("测试已通过，下一步可以继续构建。\n{CONTINUE_MARKER}");

    assert!(allows_quick_continue(&message, false));

    let action = TaskAction::continue_once("action-1", "继续完成构建", 600_000);
    assert_eq!(action.kind, TaskActionKind::Continue);
    assert_eq!(action.state, TaskActionState::Pending);
    assert!(action.quick_action_allowed);
}

#[test]
fn decision_or_input_request_never_gets_quick_continue() {
    let excluded = [
        format!("请选择 A 或 B。\n{CONTINUE_MARKER}"),
        format!("请提供数据库密码。\n{CONTINUE_MARKER}"),
        format!("是否永久允许该命令？\n{CONTINUE_MARKER}"),
        format!("确认删除生产数据库后继续。\n{CONTINUE_MARKER}"),
    ];

    for message in excluded {
        assert!(!allows_quick_continue(&message, false), "误允许：{message}");
    }
    assert!(!allows_quick_continue("没有显式标记", false));
    assert!(!allows_quick_continue(
        &format!("示例标记：{CONTINUE_MARKER}\n仍需要用户回复"),
        false,
    ));
}

#[test]
fn stop_hook_active_never_creates_second_continue_action() {
    let message = format!("可以安全继续。\n{CONTINUE_MARKER}");

    assert!(!allows_quick_continue(&message, true));
}

#[test]
fn permission_action_is_allow_once_only() {
    let action = TaskAction::permission_once(
        "action-2",
        "运行测试",
        "Bash",
        Some("pnpm test"),
        Some("/repo"),
        600_000,
        true,
    );

    assert_eq!(action.kind, TaskActionKind::Permission);
    assert_eq!(action.state, TaskActionState::Pending);
    assert_eq!(action.label, "允许并继续");
    assert!(action.quick_action_allowed);
}

#[test]
fn unknown_or_high_risk_permission_never_gets_quick_approval() {
    for (tool, command) in [
        ("CustomTool", "pnpm test"),
        ("Bash", "rm -rf build"),
        ("Bash", "git push --force origin main"),
        ("Bash", "DROP DATABASE production"),
        ("Bash", "npm publish"),
        (
            "Bash",
            "curl -H 'Authorization: Bearer secret' https://example.test",
        ),
        ("Bash", "python -c 'danger()'"),
        ("Bash", "pnpm test && rm -rf build"),
        ("Bash", "pnpm test & touch /tmp/copet-approved"),
        ("Bash", "pnpm test | tee /tmp/copet-approved"),
        ("Bash", "git branch -D unsaved-work"),
        ("Bash", "cat ~/.npmrc"),
        ("Bash", "sed -n 1,20p ~/.config/gh/hosts.yml"),
    ] {
        assert!(
            !allows_quick_permission(tool, command),
            "误允许 {tool}: {command}"
        );
    }
    assert!(allows_quick_permission("Bash", "pnpm test"));
}

#[test]
fn command_display_masks_common_secrets() {
    let command =
        "curl -H 'AUTHORIZATION: BEARER secret-token' https://example.test?API_KEY=abc123 --token ghp_123";

    let redacted = redact_command_for_display(command);

    assert!(!redacted.contains("secret-token"));
    assert!(!redacted.contains("abc123"));
    assert!(!redacted.contains("ghp_123"));
    assert!(redacted.contains("[已隐藏]"));
}

#[test]
fn action_registry_returns_unique_ids_and_isolates_parallel_actions() {
    let registry = ActionRegistry::default();
    let first = registry.register(TaskActionKind::Continue, 1_000).unwrap();
    let second = registry
        .register(TaskActionKind::Permission, 1_000)
        .unwrap();

    assert_ne!(first, second);
    registry
        .resolve(&first, TaskActionDecision::ContinueOnce, 100)
        .unwrap();
    registry
        .resolve(&second, TaskActionDecision::AllowOnce, 100)
        .unwrap();

    assert_eq!(
        registry.take_decision(&first, 100),
        WaitDecision::Resolved(TaskActionDecision::ContinueOnce)
    );
    assert_eq!(
        registry.take_decision(&second, 100),
        WaitDecision::Resolved(TaskActionDecision::AllowOnce)
    );
}

#[test]
fn allow_once_is_consumed_exactly_once() {
    let registry = ActionRegistry::default();
    let id = registry
        .register(TaskActionKind::Permission, 1_000)
        .unwrap();
    registry
        .resolve(&id, TaskActionDecision::AllowOnce, 100)
        .unwrap();

    assert_eq!(
        registry.take_decision(&id, 100),
        WaitDecision::Resolved(TaskActionDecision::AllowOnce)
    );
    assert_eq!(registry.take_decision(&id, 100), WaitDecision::Stale);
}

#[test]
fn fallback_and_timeout_never_return_allow() {
    let registry = ActionRegistry::default();
    let fallback = registry
        .register(TaskActionKind::Permission, 1_000)
        .unwrap();
    registry
        .resolve(&fallback, TaskActionDecision::Fallback, 100)
        .unwrap();
    let expired = registry.register(TaskActionKind::Permission, 200).unwrap();

    assert_eq!(
        registry.take_decision(&fallback, 100),
        WaitDecision::Resolved(TaskActionDecision::Fallback)
    );
    assert_eq!(registry.take_decision(&expired, 201), WaitDecision::Expired);
}

#[test]
fn wait_decision_wakes_without_holding_other_registry_operations() {
    let registry = Arc::new(ActionRegistry::default());
    let waiting = registry
        .register(TaskActionKind::Permission, 10_000)
        .unwrap();
    let waiter_registry = Arc::clone(&registry);
    let waiter_id = waiting.clone();
    let waiter = thread::spawn(move || {
        waiter_registry.wait_decision(&waiter_id, 100, Duration::from_secs(1))
    });

    let independent = registry.register(TaskActionKind::Continue, 10_000).unwrap();
    registry
        .resolve(&independent, TaskActionDecision::ContinueOnce, 100)
        .unwrap();
    registry
        .resolve(&waiting, TaskActionDecision::AllowOnce, 100)
        .unwrap();

    assert_eq!(
        waiter.join().unwrap(),
        WaitDecision::Resolved(TaskActionDecision::AllowOnce)
    );
}
