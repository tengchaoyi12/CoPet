use copet_lib::{
    diagnostics::RotatingLog,
    runtime_server::{handle_http_request, RuntimeCore, RuntimeServerError, RuntimeToken},
    runtime_state::{normalize_runtime_event, PetStateId, RuntimeEvent},
    task_notifications::{AttentionKind, TaskNotificationStore, TaskStatus},
};
use serde_json::json;
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn runtime_event_accepts_codex_task_fields() {
    let snake_case: RuntimeEvent = serde_json::from_value(json!({
        "agent": "codex",
        "kind": "session.stop",
        "session_id": "thread-123",
        "turn_id": "turn-456",
        "task_title": "修复登录按钮",
        "summary": "已完成登录按钮修复"
    }))
    .unwrap();
    assert_eq!(snake_case.session_id.as_deref(), Some("thread-123"));
    assert_eq!(snake_case.turn_id.as_deref(), Some("turn-456"));
    assert_eq!(snake_case.task_title.as_deref(), Some("修复登录按钮"));
    assert_eq!(snake_case.summary.as_deref(), Some("已完成登录按钮修复"));

    let camel_case: RuntimeEvent = serde_json::from_value(json!({
        "agent": "codex",
        "kind": "session.stop",
        "sessionId": "thread-789",
        "turnId": "turn-999",
        "taskTitle": "整理文档",
        "summary": "文档已整理"
    }))
    .unwrap();
    assert_eq!(camel_case.session_id.as_deref(), Some("thread-789"));
    assert_eq!(camel_case.turn_id.as_deref(), Some("turn-999"));
    assert_eq!(camel_case.task_title.as_deref(), Some("整理文档"));
    assert_eq!(camel_case.summary.as_deref(), Some("文档已整理"));
}

#[test]
fn runtime_event_compacts_and_limits_task_text() {
    let event: RuntimeEvent = serde_json::from_value(json!({
        "agent": "codex",
        "kind": "session.stop",
        "taskTitle": format!("  修复   {}  ", "甲".repeat(80)),
        "summary": format!("  完成\n{}  ", "乙".repeat(240))
    }))
    .unwrap();

    let normalized = normalize_runtime_event(event);

    let title = normalized.task_title.unwrap();
    let summary = normalized.summary.unwrap();
    assert!(title.starts_with("修复 "));
    assert_eq!(title.chars().count(), 80);
    assert!(summary.starts_with("完成 "));
    assert_eq!(summary.chars().count(), 240);
}

#[test]
fn rotate_writes_a_fresh_runtime_token() {
    let temp = tempfile::tempdir().unwrap();
    let runtime_dir = temp.path().join("runtime");

    let first = RuntimeToken::rotate(&runtime_dir).unwrap();
    let second = RuntimeToken::rotate(&runtime_dir).unwrap();
    let on_disk = fs::read_to_string(runtime_dir.join("event-token")).unwrap();

    assert!(first.len() >= 32);
    assert_ne!(first, second);
    assert_eq!(on_disk, second);
}

#[test]
fn invalidate_removes_runtime_token_when_present() {
    let temp = tempfile::tempdir().unwrap();
    let runtime_dir = temp.path().join("runtime");
    RuntimeToken::rotate(&runtime_dir).unwrap();

    RuntimeToken::invalidate(&runtime_dir).unwrap();

    assert!(!runtime_dir.join("event-token").exists());
}

#[test]
fn write_endpoint_persists_current_event_endpoint() {
    let temp = tempfile::tempdir().unwrap();
    let runtime_dir = temp.path().join("runtime");

    RuntimeToken::write_endpoint(&runtime_dir, "http://127.0.0.1:1234/v1/events").unwrap();

    assert_eq!(
        fs::read_to_string(runtime_dir.join("event-endpoint")).unwrap(),
        "http://127.0.0.1:1234/v1/events"
    );
}

#[test]
fn runtime_core_accepts_authorized_events_and_updates_status() {
    let mut core = RuntimeCore::new("secret".to_string());

    let state = core
        .handle_event(
            Some("Bearer secret"),
            RuntimeEvent {
                agent: "codex".to_string(),
                kind: "tool.before".to_string(),
                tool: Some("Read".to_string()),
                tool_input: None,
                session_id: None,
                turn_id: None,
                task_title: None,
                summary: None,
                timestamp: None,
            },
            10,
        )
        .unwrap();

    assert_eq!(state.state, PetStateId::Review);
    assert_eq!(core.status().accepted_events, 1);
    assert_eq!(core.status().rejected_events, 0);
}

#[test]
fn runtime_core_normalizes_thinking_and_tracks_it_as_agent_activity() {
    let mut core = RuntimeCore::new("secret".to_string());

    let thinking = core
        .handle_event(
            Some("Bearer secret"),
            RuntimeEvent {
                agent: "codex".to_string(),
                kind: " Thinking ".to_string(),
                tool: None,
                tool_input: None,
                session_id: None,
                turn_id: None,
                task_title: None,
                summary: None,
                timestamp: None,
            },
            100,
        )
        .unwrap();

    assert_eq!(thinking.state, PetStateId::Thinking);
    assert_eq!(thinking.idle_after_ms, Some(30_100));
    assert_eq!(core.status().messages[0].text, "Thinking...");

    let stopped = core
        .handle_event(
            Some("Bearer secret"),
            RuntimeEvent {
                agent: "codex".to_string(),
                kind: "session.stop".to_string(),
                tool: None,
                tool_input: None,
                session_id: None,
                turn_id: None,
                task_title: None,
                summary: None,
                timestamp: None,
            },
            200,
        )
        .unwrap();

    assert_eq!(stopped.state, PetStateId::Waving);
    assert_eq!(core.status().messages[0].text, "Done.");
}

#[test]
fn runtime_core_tracks_latest_message_per_agent() {
    let mut core = RuntimeCore::new("secret".to_string());

    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "codex".to_string(),
            kind: "tool.before".to_string(),
            tool: Some("Read".to_string()),
            tool_input: Some(json!({ "file_path": "/repo/src/App.tsx" })),
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        100,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "claude-code".to_string(),
            kind: "tool.before".to_string(),
            tool: Some("Bash".to_string()),
            tool_input: Some(json!({ "command": "pnpm test:frontend" })),
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        200,
    )
    .unwrap();

    let status = core.status();
    let codex = status
        .messages
        .iter()
        .find(|message| message.agent == "codex")
        .unwrap();
    let claude = status
        .messages
        .iter()
        .find(|message| message.agent == "claude-code")
        .unwrap();

    assert_eq!(codex.display_name, "Codex");
    assert_eq!(codex.text, "Reading App.tsx");
    assert_eq!(codex.updated_at_ms, 100);
    assert_eq!(claude.display_name, "Claude Code");
    assert_eq!(claude.text, "Running pnpm test:frontend");
    assert_eq!(claude.updated_at_ms, 200);
}

#[test]
fn runtime_core_clears_messages_for_one_agent() {
    let mut core = RuntimeCore::new("secret".to_string());

    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "codex".to_string(),
            kind: "tool.before".to_string(),
            tool: Some("Read".to_string()),
            tool_input: Some(json!({ "file_path": "/repo/src/App.tsx" })),
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        100,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "cursor".to_string(),
            kind: "tool.before".to_string(),
            tool: Some("Bash".to_string()),
            tool_input: Some(json!({ "command": "pnpm build" })),
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        200,
    )
    .unwrap();

    let update = core.clear_agent_messages("codex");

    assert_eq!(update.messages.len(), 1);
    assert_eq!(update.messages[0].agent, "cursor");
    assert_eq!(core.status().messages, update.messages);
}

#[test]
fn runtime_core_includes_prompt_subject_in_user_prompt_message() {
    let mut core = RuntimeCore::new("secret".to_string());

    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "copilot".to_string(),
            kind: "user.prompt".to_string(),
            tool: None,
            tool_input: Some(json!({ "subject": "add copilot cli integration messages" })),
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        100,
    )
    .unwrap();

    let status = core.status();
    let message = status
        .messages
        .iter()
        .find(|message| message.agent == "copilot")
        .unwrap();

    assert_eq!(message.display_name, "Copilot CLI");
    assert_eq!(
        message.text,
        "Thinking: add copilot cli integration messages"
    );
}

#[test]
fn runtime_core_formats_copilot_official_tool_names() {
    let mut core = RuntimeCore::new("secret".to_string());

    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "copilot".to_string(),
            kind: "tool.before".to_string(),
            tool: Some("view".to_string()),
            tool_input: Some(json!({ "path": "/repo/src/App.tsx" })),
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        100,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "copilot".to_string(),
            kind: "tool.before".to_string(),
            tool: Some("web_fetch".to_string()),
            tool_input: Some(json!({ "url": "https://docs.github.com/copilot" })),
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        200,
    )
    .unwrap();

    let status = core.status();
    let message = status
        .messages
        .iter()
        .find(|message| message.agent == "copilot")
        .unwrap();

    assert_eq!(message.display_name, "Copilot CLI");
    assert_eq!(message.text, "Fetching docs.github.com");
}

#[test]
fn runtime_core_keeps_antigravity_tool_detail_when_post_tool_lacks_payload() {
    let mut core = RuntimeCore::new("secret".to_string());

    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "antigravity".to_string(),
            kind: "tool.before".to_string(),
            tool: Some("run_command".to_string()),
            tool_input: Some(json!({
                "command": "pnpm test:frontend src/tests/settings-workflows.spec.ts"
            })),
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        100,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "antigravity".to_string(),
            kind: "tool.after".to_string(),
            tool: None,
            tool_input: None,
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        200,
    )
    .unwrap();

    let status = core.status();
    let message = status
        .messages
        .iter()
        .find(|message| message.agent == "antigravity")
        .unwrap();

    assert_eq!(message.display_name, "Antigravity");
    assert_eq!(
        message.text,
        "Running pnpm test:frontend src/tests/settings-workflows.spec.ts"
    );
    assert_eq!(message.updated_at_ms, 100);
}

#[test]
fn runtime_core_does_not_create_message_for_stop_without_prior_agent_activity() {
    let mut core = RuntimeCore::new("secret".to_string());

    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "antigravity".to_string(),
            kind: "session.stop".to_string(),
            tool: None,
            tool_input: None,
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        100,
    )
    .unwrap();

    let status = core.status();

    assert_eq!(status.accepted_events, 1);
    assert_eq!(status.current_state.state, PetStateId::Idle);
    assert!(status
        .messages
        .iter()
        .all(|message| message.agent != "antigravity"));
}

#[test]
fn runtime_core_updates_existing_message_for_stop_after_agent_activity() {
    let mut core = RuntimeCore::new("secret".to_string());

    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "antigravity".to_string(),
            kind: "tool.before".to_string(),
            tool: Some("run_command".to_string()),
            tool_input: Some(json!({ "command": "git commit -m fix-antigravity" })),
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        100,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "antigravity".to_string(),
            kind: "session.stop".to_string(),
            tool: None,
            tool_input: None,
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        200,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "antigravity".to_string(),
            kind: "session.stop".to_string(),
            tool: None,
            tool_input: None,
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        300,
    )
    .unwrap();

    let status = core.status();
    let message = status
        .messages
        .iter()
        .find(|message| message.agent == "antigravity")
        .unwrap();

    assert_eq!(message.display_name, "Antigravity");
    assert_eq!(message.text, "Done.");
    assert_eq!(message.updated_at_ms, 200);
    assert_eq!(status.accepted_events, 3);
    assert_eq!(status.current_state.state, PetStateId::Waving);
}

#[test]
fn runtime_core_suppresses_antigravity_stop_after_prompt_when_cli_never_started() {
    let mut core = RuntimeCore::new("secret".to_string());

    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "antigravity".to_string(),
            kind: "user.prompt".to_string(),
            tool: None,
            tool_input: Some(json!({ "subject": "git commit" })),
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        100,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "antigravity".to_string(),
            kind: "session.stop".to_string(),
            tool: None,
            tool_input: None,
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        200,
    )
    .unwrap();

    let status = core.status();
    let message = status
        .messages
        .iter()
        .find(|message| message.agent == "antigravity")
        .unwrap();

    assert_eq!(message.text, "Thinking: git commit");
    assert_eq!(message.updated_at_ms, 100);
    assert_eq!(status.current_state.state, PetStateId::Jumping);
}

#[test]
fn runtime_core_normalizes_agent_aliases_and_raw_cli_event_kinds_for_messages() {
    let mut core = RuntimeCore::new("secret".to_string());

    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "claude".to_string(),
            kind: "PreToolUse".to_string(),
            tool: Some("Bash".to_string()),
            tool_input: Some(json!({ "command": "pnpm build" })),
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        100,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "open-code".to_string(),
            kind: "tool.execute.before".to_string(),
            tool: Some("Read".to_string()),
            tool_input: Some(json!({ "filePath": "/repo/src/App.tsx" })),
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        200,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "gemini".to_string(),
            kind: "BeforeTool".to_string(),
            tool: Some("Read".to_string()),
            tool_input: Some(json!({ "file_path": "/repo/src/lib.rs" })),
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        300,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "gemini".to_string(),
            kind: "BeforeAgent".to_string(),
            tool: None,
            tool_input: None,
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        400,
    )
    .unwrap();

    let status = core.status();
    let claude = status
        .messages
        .iter()
        .find(|message| message.agent == "claude-code")
        .unwrap();
    let opencode = status
        .messages
        .iter()
        .find(|message| message.agent == "opencode")
        .unwrap();
    let gemini = status
        .messages
        .iter()
        .find(|message| message.agent == "gemini")
        .unwrap();

    assert_eq!(claude.display_name, "Claude Code");
    assert_eq!(claude.text, "Running pnpm build");
    assert_eq!(opencode.display_name, "OpenCode");
    assert_eq!(opencode.text, "Reading App.tsx");
    assert_eq!(gemini.display_name, "Gemini");
    assert_eq!(gemini.text, "Thinking...");
    assert_eq!(status.current_state.state, PetStateId::Jumping);
}

#[test]
fn runtime_core_normalizes_cursor_and_pi_events() {
    let mut core = RuntimeCore::new("secret".to_string());

    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "cursor-agent".to_string(),
            kind: "preToolUse".to_string(),
            tool: Some("Shell".to_string()),
            tool_input: Some(json!({ "command": "pnpm build" })),
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        100,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "pi".to_string(),
            kind: "before_agent_start".to_string(),
            tool: None,
            tool_input: Some(json!({ "subject": "implement pi integration" })),
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        200,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "pi".to_string(),
            kind: "tool_call".to_string(),
            tool: Some("Read".to_string()),
            tool_input: Some(json!({ "filePath": "/repo/src/App.tsx" })),
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        600,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        RuntimeEvent {
            agent: "pi".to_string(),
            kind: "tool_result".to_string(),
            tool: Some("Read".to_string()),
            tool_input: Some(json!({ "filePath": "/repo/src/App.tsx" })),
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        900,
    )
    .unwrap();

    let status = core.status();
    let cursor = status
        .messages
        .iter()
        .find(|message| message.agent == "cursor")
        .unwrap();
    let pi = status
        .messages
        .iter()
        .find(|message| message.agent == "pi")
        .unwrap();

    assert_eq!(cursor.display_name, "Cursor");
    assert_eq!(cursor.text, "Running pnpm build");
    assert_eq!(pi.display_name, "Pi");
    assert_eq!(pi.text, "Read App.tsx");
    assert_eq!(status.current_state.state, PetStateId::Idle);
}

#[test]
fn runtime_core_rejects_missing_or_wrong_bearer_token() {
    let mut core = RuntimeCore::new("secret".to_string());

    let result = core.handle_event(
        Some("Bearer nope"),
        RuntimeEvent {
            agent: "codex".to_string(),
            kind: "tool.before".to_string(),
            tool: None,
            tool_input: None,
            session_id: None,
            turn_id: None,
            task_title: None,
            summary: None,
            timestamp: None,
        },
        10,
    );

    assert_eq!(result, Err(RuntimeServerError::Unauthorized));
    assert_eq!(core.status().accepted_events, 0);
    assert_eq!(core.status().rejected_events, 1);
}

#[test]
fn http_handler_accepts_event_posts() {
    let mut core = RuntimeCore::new("secret".to_string());
    let request = concat!(
        "POST /v1/events HTTP/1.1\r\n",
        "Host: 127.0.0.1\r\n",
        "Authorization: Bearer secret\r\n",
        "Content-Type: application/json\r\n",
        "Content-Length: 52\r\n",
        "\r\n",
        r#"{"agent":"codex","kind":"tool.before","tool":"Read"}"#
    );

    let response = handle_http_request(&mut core, request.as_bytes(), 100);

    assert_eq!(response.status_code, 202);
    assert!(response.body.contains("review"));
}

#[test]
fn http_handler_accepts_cli_style_snake_case_event_payloads() {
    let mut core = RuntimeCore::new("secret".to_string());
    let body = r#"{"agent":"opencode","kind":"tool.before","tool":"Read","tool_input":{"filePath":"/repo/src/App.tsx"},"session_id":"session-1"}"#;
    let request = format!(
        "POST /v1/events HTTP/1.1\r\n\
         Host: 127.0.0.1\r\n\
         Authorization: Bearer secret\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         \r\n\
         {}",
        body.len(),
        body
    );

    let response = handle_http_request(&mut core, request.as_bytes(), 100);

    assert_eq!(response.status_code, 202);
    assert!(response.body.contains("review"));
    let status = core.status();
    let message = status.messages.first().unwrap();
    assert_eq!(message.agent, "opencode");
    assert_eq!(message.text, "Reading App.tsx");
}

#[test]
fn http_handler_rejects_large_bodies_before_parsing() {
    let mut core = RuntimeCore::new("secret".to_string());
    let request = concat!(
        "POST /v1/events HTTP/1.1\r\n",
        "Host: 127.0.0.1\r\n",
        "Authorization: Bearer secret\r\n",
        "Content-Type: application/json\r\n",
        "Content-Length: 16385\r\n",
        "\r\n"
    );

    let response = handle_http_request(&mut core, request.as_bytes(), 100);

    assert_eq!(response.status_code, 413);
}

#[test]
fn runtime_event_log_rotates_under_synthetic_event_stream() {
    let temp = tempfile::tempdir().unwrap();
    let log_path = temp.path().join("agent-events.log");
    let logger = RotatingLog::new(&log_path, 512, 2);
    let mut core = RuntimeCore::new("secret".to_string()).with_logger(logger);

    for index in 0..200 {
        let _ = core.handle_event(
            Some("Bearer secret"),
            RuntimeEvent {
                agent: "codex".to_string(),
                kind: "tool.before".to_string(),
                tool: Some(format!("Tool{index}")),
                tool_input: None,
                session_id: Some("synthetic-session".to_string()),
                turn_id: None,
                task_title: None,
                summary: None,
                timestamp: Some(index),
            },
            1_000 + index,
        );
    }

    let current_size = fs::metadata(&log_path).unwrap().len();
    let rotated_size = fs::metadata(temp.path().join("agent-events.log.1"))
        .unwrap()
        .len();

    assert!(current_size <= 512);
    assert!(rotated_size <= 512);
    assert!(core.status().accepted_events > 0);
}

#[test]
fn codex_completed_turn_emits_attention_only_once() {
    let mut core = RuntimeCore::new("secret".to_string());
    let completed = codex_event("session.stop", "thread-1", "turn-1");

    core.handle_event(Some("Bearer secret"), completed.clone(), 100)
        .unwrap();
    let first = core.take_update();
    core.handle_event(Some("Bearer secret"), completed, 200)
        .unwrap();
    let duplicate = core.take_update();

    assert_eq!(first.notifications.len(), 1);
    assert_eq!(first.attention.unwrap().kind, AttentionKind::Completed);
    assert_eq!(duplicate.notifications.len(), 1);
    assert!(duplicate.attention.is_none());
    assert!(core.status().attention.is_none());
}

#[test]
fn codex_parallel_turns_keep_independent_notifications() {
    let mut core = RuntimeCore::new("secret".to_string());

    core.handle_event(
        Some("Bearer secret"),
        codex_event("session.stop", "thread-1", "turn-1"),
        100,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        codex_event("session.stop", "thread-2", "turn-2"),
        200,
    )
    .unwrap();

    let status = core.status();
    assert_eq!(status.notifications.len(), 2);
    assert_eq!(status.notifications[0].id, "codex:thread-2:turn-2");
    assert_eq!(status.notifications[1].id, "codex:thread-1:turn-1");
}

#[test]
fn codex_waiting_notification_overrides_completed_until_read() {
    let mut core = RuntimeCore::new("secret".to_string());
    core.handle_event(
        Some("Bearer secret"),
        codex_event("session.stop", "completed", "turn-1"),
        100,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        codex_event("permission.waiting", "waiting", "turn-2"),
        200,
    )
    .unwrap();

    assert_eq!(core.status().current_state.state, PetStateId::Waiting);
    core.mark_task_notification_read("codex:waiting:turn-2");
    assert_eq!(core.status().current_state.state, PetStateId::Waving);
}

#[test]
fn codex_unknown_event_is_rejected_without_incrementing_accepted_count() {
    let mut core = RuntimeCore::new("secret".to_string());
    let event = codex_event("session.teleported", "thread-1", "turn-1");

    let result = core.handle_event(Some("Bearer secret"), event, 100);

    assert_eq!(result, Err(RuntimeServerError::UnsupportedEvent));
    assert_eq!(core.status().accepted_events, 0);
    assert_eq!(core.status().rejected_events, 1);
}

#[test]
fn codex_http_unknown_event_returns_bad_request() {
    let mut core = RuntimeCore::new("secret".to_string());
    let body =
        r#"{"agent":"codex","kind":"session.teleported","sessionId":"thread-1","turnId":"turn-1"}"#;
    let request = format!(
        "POST /v1/events HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer secret\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );

    let response = handle_http_request(&mut core, request.as_bytes(), 100);

    assert_eq!(response.status_code, 400);
    assert!(response.body.contains("unsupported_event"));
}

#[test]
fn codex_runtime_relimits_task_text_before_exposing_notifications() {
    let mut core = RuntimeCore::new("secret".to_string());
    let mut event = codex_event("session.stop", "thread-1", "turn-1");
    event.task_title = Some(format!("  标题   {}  ", "甲".repeat(100)));
    event.summary = Some(format!("  摘要\n{}  ", "乙".repeat(300)));

    core.handle_event(Some("Bearer secret"), event, 100)
        .unwrap();

    let notification = &core.status().notifications[0];
    assert_eq!(notification.title.as_ref().unwrap().chars().count(), 80);
    assert_eq!(notification.summary.as_ref().unwrap().chars().count(), 240);
    assert!(!notification.title.as_ref().unwrap().contains("  "));
    assert!(!notification.summary.as_ref().unwrap().contains('\n'));
}

#[test]
fn failed_task_open_keeps_notification_unread() {
    let mut core = RuntimeCore::new("secret".to_string());
    core.handle_event(
        Some("Bearer secret"),
        codex_event("session.stop", "thread-1", "turn-1"),
        100,
    )
    .unwrap();

    let result = core.open_task_notification_with("codex:thread-1:turn-1", |_| {
        Err("Codex 无法打开".to_string())
    });

    assert_eq!(result, Err("Codex 无法打开".to_string()));
    assert!(core.status().notifications[0].unread);
}

#[test]
fn dismiss_task_notification_removes_only_selected_notification() {
    let mut core = RuntimeCore::new("secret".to_string());
    core.handle_event(
        Some("Bearer secret"),
        codex_event("session.stop", "thread-1", "turn-1"),
        100,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        codex_event("session.stop", "thread-2", "turn-2"),
        200,
    )
    .unwrap();

    let update = core
        .dismiss_task_notification("codex:thread-1:turn-1")
        .unwrap();

    assert_eq!(update.notifications.len(), 1);
    assert_eq!(update.notifications[0].id, "codex:thread-2:turn-2");
}

#[test]
fn clear_completed_task_notifications_keeps_waiting_and_failed() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("task-notifications.json");
    let now_ms = test_now_ms();
    let mut core = RuntimeCore::new("secret".to_string())
        .with_task_notifications(TaskNotificationStore::default(), path);
    core.handle_event(
        Some("Bearer secret"),
        codex_event("session.stop", "completed", "turn-1"),
        now_ms,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        codex_event("permission.waiting", "waiting", "turn-2"),
        now_ms + 1,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        codex_event("session.error", "failed", "turn-3"),
        now_ms + 2,
    )
    .unwrap();

    let update = core.clear_completed_task_notifications();

    assert_eq!(update.notifications.len(), 2);
    assert!(update
        .notifications
        .iter()
        .all(|item| item.status != TaskStatus::Completed));
    assert!(update
        .notifications
        .iter()
        .any(|item| item.status == TaskStatus::Waiting));
    assert!(update
        .notifications
        .iter()
        .any(|item| item.status == TaskStatus::Failed));
    assert!(update.attention.is_none());
}

#[test]
fn clear_completed_task_notifications_removes_only_legacy_completion_messages() {
    let mut completed = RuntimeCore::new("secret".to_string());
    completed
        .handle_event(
            Some("Bearer secret"),
            codex_event("user.prompt", "completed", "turn-1"),
            100,
        )
        .unwrap();
    completed
        .handle_event(
            Some("Bearer secret"),
            codex_event("session.stop", "completed", "turn-1"),
            200,
        )
        .unwrap();
    assert_eq!(completed.status().messages[0].text, "Done.");

    let update = completed.clear_completed_task_notifications();

    assert!(update.notifications.is_empty());
    assert!(update.messages.is_empty());

    let mut waiting = RuntimeCore::new("secret".to_string());
    waiting
        .handle_event(
            Some("Bearer secret"),
            codex_event("permission.waiting", "waiting", "turn-2"),
            300,
        )
        .unwrap();
    assert_eq!(
        waiting.clear_completed_task_notifications().messages[0].text,
        "Waiting for you..."
    );

    let mut failed = RuntimeCore::new("secret".to_string());
    failed
        .handle_event(
            Some("Bearer secret"),
            codex_event("session.error", "failed", "turn-3"),
            400,
        )
        .unwrap();
    assert_eq!(
        failed.clear_completed_task_notifications().messages[0].text,
        "Error."
    );
}

#[test]
fn clear_completed_task_notifications_persists_retained_notifications() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("task-notifications.json");
    let now_ms = test_now_ms();
    let mut core = RuntimeCore::new("secret".to_string())
        .with_task_notifications(TaskNotificationStore::default(), path.clone());
    core.handle_event(
        Some("Bearer secret"),
        codex_event("session.stop", "completed", "turn-1"),
        now_ms,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        codex_event("permission.waiting", "waiting", "turn-2"),
        now_ms + 1,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        codex_event("session.error", "failed", "turn-3"),
        now_ms + 2,
    )
    .unwrap();

    core.clear_completed_task_notifications();

    let restored = TaskNotificationStore::load(&path, now_ms + 3).unwrap();
    let visible = restored.visible();
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
fn clear_completed_task_notifications_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("task-notifications.json");
    let now_ms = test_now_ms();
    let mut core = RuntimeCore::new("secret".to_string())
        .with_task_notifications(TaskNotificationStore::default(), path.clone());
    core.handle_event(
        Some("Bearer secret"),
        codex_event("session.stop", "completed", "turn-1"),
        now_ms,
    )
    .unwrap();
    core.handle_event(
        Some("Bearer secret"),
        codex_event("session.error", "failed", "turn-2"),
        now_ms + 1,
    )
    .unwrap();

    let first = core.clear_completed_task_notifications();
    let persisted_after_first = fs::read(&path).unwrap();
    let second = core.clear_completed_task_notifications();

    assert_eq!(second, first);
    assert_eq!(fs::read(&path).unwrap(), persisted_after_first);
}

fn codex_event(kind: &str, session_id: &str, turn_id: &str) -> RuntimeEvent {
    RuntimeEvent {
        agent: "codex".to_string(),
        kind: kind.to_string(),
        tool: None,
        tool_input: None,
        session_id: Some(session_id.to_string()),
        turn_id: Some(turn_id.to_string()),
        task_title: None,
        summary: None,
        timestamp: None,
    }
}

fn test_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
