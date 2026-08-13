use copet_lib::{
    runtime_server::RuntimeManager, runtime_state::PetStateId, task_notifications::TaskStatus,
};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

#[test]
fn runtime_manager_accepts_authorized_http_events_and_rejects_bad_tokens() {
    let temp = tempfile::tempdir().unwrap();
    let runtime_dir = temp.path().join("runtime");
    let observed_states = Arc::new(Mutex::new(Vec::new()));
    let observed_for_callback = Arc::clone(&observed_states);
    let manager = RuntimeManager::start(&runtime_dir, move |update| {
        observed_for_callback
            .lock()
            .unwrap()
            .push(update.current_state.state);
    })
    .unwrap();
    let endpoint = fs::read_to_string(runtime_dir.join("event-endpoint")).unwrap();
    let token = fs::read_to_string(runtime_dir.join("event-token")).unwrap();

    assert_eq!(
        endpoint,
        format!("http://127.0.0.1:{}/v1/events", manager.port())
    );

    let accepted = post_runtime_event(
        manager.port(),
        &token,
        r#"{"agent":"codex","kind":"tool.before","tool":"Read"}"#,
    );
    assert!(accepted.starts_with("HTTP/1.1 202 Accepted"));
    assert!(accepted.contains("\"state\":\"review\""));

    let rejected = post_runtime_event(
        manager.port(),
        "wrong-token",
        r#"{"agent":"codex","kind":"tool.before","tool":"Read"}"#,
    );
    assert!(rejected.starts_with("HTTP/1.1 401 Unauthorized"));

    let snapshot = manager.snapshot();
    assert_eq!(snapshot.accepted_events, 1);
    assert_eq!(snapshot.rejected_events, 1);
    assert_eq!(snapshot.current_state.state, PetStateId::Review);
    assert!(observed_states
        .lock()
        .unwrap()
        .contains(&PetStateId::Review));

    drop(manager);
    assert!(!runtime_dir.join("event-token").exists());
    assert!(!runtime_dir.join("event-endpoint").exists());
}

#[test]
fn explicit_shutdown_releases_port_before_drop() {
    let temp = tempfile::tempdir().unwrap();
    let runtime_dir = temp.path().join("runtime");
    let manager = RuntimeManager::start(&runtime_dir, |_| {}).unwrap();
    let port = manager.port();

    // Quit-handler path: call shutdown() explicitly before letting the manager
    // drop. The OS port and the on-disk endpoint files must already be released.
    manager.shutdown();
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut bound = false;
    while Instant::now() < deadline {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            bound = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        bound,
        "TCP port {port} should be re-bindable immediately after shutdown()"
    );
    assert!(!runtime_dir.join("event-token").exists());
    assert!(!runtime_dir.join("event-endpoint").exists());

    // Second shutdown() call must be a no-op: no panic, no spurious connect.
    manager.shutdown();
    drop(manager); // Drop's internal shutdown call is also idempotent.
}

#[test]
fn drop_releases_tcp_port_for_immediate_rebind() {
    let temp = tempfile::tempdir().unwrap();
    let runtime_dir = temp.path().join("runtime");
    let manager = RuntimeManager::start(&runtime_dir, |_| {}).unwrap();
    let port = manager.port();
    drop(manager);

    // After Drop, the worker thread observes the shutdown flag and the listener
    // is released. The wake-up self-connect in Drop is bounded at 50ms; we
    // allow generous CI slack here while polling for the port to free up.
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut bound = false;
    while Instant::now() < deadline {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            bound = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        bound,
        "TCP port {port} should be re-bindable after RuntimeManager drop"
    );
    assert!(!runtime_dir.join("event-token").exists());
    assert!(!runtime_dir.join("event-endpoint").exists());
}

#[test]
fn runtime_manager_clears_completed_task_notifications() {
    let temp = tempfile::tempdir().unwrap();
    let runtime_dir = temp.path().join("runtime");
    let manager = RuntimeManager::start(&runtime_dir, |_| {}).unwrap();
    let token = fs::read_to_string(runtime_dir.join("event-token")).unwrap();
    for body in [
        r#"{"agent":"codex","kind":"session.stop","sessionId":"completed","turnId":"turn-1"}"#,
        r#"{"agent":"codex","kind":"permission.waiting","sessionId":"waiting","turnId":"turn-2"}"#,
        r#"{"agent":"codex","kind":"session.error","sessionId":"failed","turnId":"turn-3"}"#,
    ] {
        let response = post_runtime_event(manager.port(), &token, body);
        assert!(response.starts_with("HTTP/1.1 202 Accepted"));
    }

    let update = manager.clear_completed_task_notifications();

    assert_eq!(update.notifications.len(), 2);
    assert!(update
        .notifications
        .iter()
        .all(|item| item.status != TaskStatus::Completed));
}

#[test]
fn waiting_for_action_decision_does_not_block_normal_event_ingestion() {
    let temp = tempfile::tempdir().unwrap();
    let runtime_dir = temp.path().join("runtime");
    let manager = RuntimeManager::start(&runtime_dir, |_| {}).unwrap();
    let token = fs::read_to_string(runtime_dir.join("event-token")).unwrap();
    let body = r#"{"agent":"codex","kind":"permission.waiting","hookInput":{"session_id":"thread-1","turn_id":"turn-1","run_id_suffix":"approval-1","tool_name":"Bash","tool_input":{"command":"pnpm test"},"cwd":"/repo"}}"#;
    let registered = request(manager.port(), &token, "POST", "/v1/actions", Some(body));
    let response_body = registered.split("\r\n\r\n").nth(1).unwrap();
    let action: serde_json::Value = serde_json::from_str(response_body).unwrap();
    let action_id = action["actionId"].as_str().unwrap().to_string();
    let port = manager.port();
    let wait_token = token.clone();
    let waiter = thread::spawn(move || {
        request(
            port,
            &wait_token,
            "GET",
            &format!("/v1/actions/{action_id}/decision"),
            None,
        )
    });

    let accepted = post_runtime_event(
        manager.port(),
        &token,
        r#"{"agent":"codex","kind":"tool.before","tool":"Read"}"#,
    );
    assert!(accepted.starts_with("HTTP/1.1 202 Accepted"));
    manager.shutdown();
    let decision = waiter.join().unwrap();
    assert!(decision.contains("fallback"));
}

#[test]
fn action_decision_route_rejects_extra_path_segments() {
    let temp = tempfile::tempdir().unwrap();
    let runtime_dir = temp.path().join("runtime");
    let manager = RuntimeManager::start(&runtime_dir, |_| {}).unwrap();
    let token = fs::read_to_string(runtime_dir.join("event-token")).unwrap();

    let response = request(
        manager.port(),
        &token,
        "GET",
        "/v1/actions/action-1/extra/decision",
        None,
    );

    assert!(response.starts_with("HTTP/1.1 404 Not Found"));
}

fn post_runtime_event(port: u16, token: &str, body: &str) -> String {
    request(port, token, "POST", "/v1/events", Some(body))
}

fn request(port: u16, token: &str, method: &str, path: &str, body: Option<&str>) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let body = body.unwrap_or("");
    let request = format!(
        "{method} {path} HTTP/1.1\r\n\
         Host: 127.0.0.1\r\n\
         Authorization: Bearer {token}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).unwrap();

    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}
