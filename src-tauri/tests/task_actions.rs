use copet_lib::task_actions::{
    allows_quick_continue, redact_command_for_display, TaskAction, TaskActionKind,
    TaskActionState, CONTINUE_MARKER,
};

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
fn command_display_masks_common_secrets() {
    let command = "curl -H 'Authorization: Bearer secret-token' https://example.test?api_key=abc123";

    let redacted = redact_command_for_display(command);

    assert!(!redacted.contains("secret-token"));
    assert!(!redacted.contains("abc123"));
    assert!(redacted.contains("[已隐藏]"));
}
