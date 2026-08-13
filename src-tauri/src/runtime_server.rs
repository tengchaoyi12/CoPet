use std::{
    collections::{HashMap, HashSet},
    fs, io,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    diagnostics::RotatingLog,
    runtime_state::{
        agent_display_name, canonical_event_kind, normalize_runtime_event, BoundedEventQueue,
        DerivedPetState, EventStateEngine, RuntimeEvent, TokenBucket,
    },
    task_actions::{
        allows_quick_continue, ActionRegistry, TaskAction, TaskActionDecision, TaskActionKind,
        WaitDecision,
    },
    task_notifications::{TaskAttention, TaskNotification, TaskNotificationStore},
    task_opener,
};

const MAX_EVENT_BODY_BYTES: usize = 16 * 1024;

pub struct RuntimeToken;

impl RuntimeToken {
    pub fn rotate(runtime_dir: &Path) -> io::Result<String> {
        fs::create_dir_all(runtime_dir)?;
        let mut bytes = [0_u8; 32];
        getrandom::getrandom(&mut bytes).map_err(|error| io::Error::other(error.to_string()))?;
        let token = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        fs::write(runtime_dir.join("event-token"), &token)?;
        Ok(token)
    }

    pub fn invalidate(runtime_dir: &Path) -> io::Result<()> {
        match fs::remove_file(runtime_dir.join("event-token")) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    pub fn write_endpoint(runtime_dir: &Path, endpoint: &str) -> io::Result<()> {
        fs::create_dir_all(runtime_dir)?;
        fs::write(runtime_dir.join("event-endpoint"), endpoint)?;
        Ok(())
    }
}

pub struct HttpResponse {
    pub status_code: u16,
    pub body: String,
}

pub fn handle_http_request(core: &mut RuntimeCore, request: &[u8], now_ms: u64) -> HttpResponse {
    match parse_http_request(request) {
        Ok(request) => {
            if request.content_length > MAX_EVENT_BODY_BYTES {
                return response(413, r#"{"error":"body_too_large"}"#);
            }

            if request.body.len() < request.content_length {
                return response(400, r#"{"error":"incomplete_body"}"#);
            }

            if request.method == "POST" && request.path == "/v1/actions" {
                if request.authorization.as_deref()
                    != Some(format!("Bearer {}", core.token).as_str())
                {
                    return response(401, r#"{"error":"unauthorized"}"#);
                }
                let action = match serde_json::from_slice::<ActionHookRequest>(&request.body) {
                    Ok(action) => action,
                    Err(_) => return response(400, r#"{"error":"invalid_json"}"#),
                };
                return match core.register_action(action, now_ms) {
                    Ok(registration) => response_json(202, &registration),
                    Err(error) => response_json(400, &serde_json::json!({ "error": error })),
                };
            }

            if request.method != "POST" || request.path != "/v1/events" {
                return response(404, r#"{"error":"not_found"}"#);
            }

            let event = match serde_json::from_slice::<RuntimeEvent>(&request.body) {
                Ok(event) => event,
                Err(_) => return response(400, r#"{"error":"invalid_json"}"#),
            };

            dev_log_runtime(
                "http.event.received",
                serde_json::json!({
                    "agent": &event.agent,
                    "kind": &event.kind,
                    "tool": &event.tool,
                    "sessionId": &event.session_id,
                    "authorized": request.authorization.is_some(),
                }),
            );

            match core.handle_event(request.authorization.as_deref(), event, now_ms) {
                Ok(state) => response_json(202, &state),
                Err(RuntimeServerError::Unauthorized) => {
                    response(401, r#"{"error":"unauthorized"}"#)
                }
                Err(RuntimeServerError::RateLimited) => {
                    response(429, r#"{"error":"rate_limited"}"#)
                }
                Err(RuntimeServerError::UnsupportedEvent) => {
                    response(400, r#"{"error":"unsupported_event"}"#)
                }
            }
        }
        Err(status) => response(status, r#"{"error":"bad_request"}"#),
    }
}

pub struct RuntimeManager {
    core: Arc<Mutex<RuntimeCore>>,
    port: u16,
    runtime_dir: std::path::PathBuf,
    shutdown: Arc<AtomicBool>,
    actions: Arc<ActionRegistry>,
}

impl RuntimeManager {
    pub fn start(
        runtime_dir: &Path,
        on_state: impl Fn(RuntimeUpdate) + Send + Sync + 'static,
    ) -> io::Result<Self> {
        let token = RuntimeToken::rotate(runtime_dir)?;
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let endpoint = format!("http://127.0.0.1:{port}/v1/events");
        RuntimeToken::write_endpoint(runtime_dir, &endpoint)?;
        let logger = RotatingLog::new(runtime_dir.join("agent-events.log"), 64 * 1024, 3);
        let notification_path = runtime_dir.join("task-notifications.json");
        let task_notifications = TaskNotificationStore::load(&notification_path, now_ms())?;
        let actions = Arc::new(ActionRegistry::default());
        let core = Arc::new(Mutex::new(
            RuntimeCore::new_with_actions(token, Arc::clone(&actions))
                .with_task_notifications(task_notifications, notification_path)
                .with_logger(logger),
        ));
        dev_log_runtime(
            "server.started",
            serde_json::json!({
                "endpoint": endpoint,
                "runtimeDir": runtime_dir.to_string_lossy(),
                "eventLog": runtime_dir.join("agent-events.log").to_string_lossy(),
            }),
        );
        let server_core = Arc::clone(&core);
        let on_state = Arc::new(on_state);
        let tick_core = Arc::clone(&core);
        let tick_on_state = Arc::clone(&on_state);
        let shutdown = Arc::new(AtomicBool::new(false));
        let server_shutdown = Arc::clone(&shutdown);
        let tick_shutdown = Arc::clone(&shutdown);

        thread::Builder::new()
            .name("copet-runtime-event-server".to_string())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    if server_shutdown.load(Ordering::Relaxed) {
                        // Drop closes the listener and releases the TCP port.
                        break;
                    }
                    let core = Arc::clone(&server_core);
                    let on_state = Arc::clone(&on_state);
                    thread::spawn(move || handle_connection(stream, core, on_state.as_ref()));
                }
            })?;

        thread::Builder::new()
            .name("copet-runtime-state-tick".to_string())
            .spawn(move || loop {
                if tick_shutdown.load(Ordering::Relaxed) {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
                let mut core = tick_core.lock().expect("runtime core poisoned");
                let previous = core.status().current_state;
                let next = core.advance_time(now_ms());
                if next != previous {
                    let status = core.status();
                    tick_on_state(RuntimeUpdate {
                        current_state: next,
                        messages: status.messages,
                        notifications: status.notifications,
                        attention: None,
                    });
                }
            })?;

        Ok(Self {
            core,
            port,
            runtime_dir: runtime_dir.to_path_buf(),
            shutdown,
            actions,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Signal both worker threads to stop, wake the blocking accept() via a
    /// self-connect, and clean up the on-disk endpoint and token files.
    ///
    /// Safe to call multiple times. Drop calls this automatically, but the
    /// quit handlers invoke it BEFORE `app.exit(0)` because Tauri 2 on macOS
    /// does not always reach `std::process::exit` after the tray fires the
    /// exit event — `NSApplication` can intercept the terminate and leave the
    /// process alive. Releasing the listener up front guarantees the OS port
    /// is freed even if the process lingers, so the next launch is not
    /// blocked by "address already in use".
    pub fn shutdown(&self) {
        if self.shutdown.swap(true, Ordering::Relaxed) {
            return; // already shut down by a prior call
        }
        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        self.actions.resolve_all_fallback();
        let _ = TcpStream::connect_timeout(&addr, Duration::from_millis(50));
        let _ = RuntimeToken::invalidate(&self.runtime_dir);
        let _ = fs::remove_file(self.runtime_dir.join("event-endpoint"));
    }

    pub fn snapshot(&self) -> RuntimeSnapshot {
        let status = self.core.lock().expect("runtime core poisoned").status();
        RuntimeSnapshot {
            port: self.port,
            endpoint: format!("http://127.0.0.1:{}/v1/events", self.port),
            current_state: status.current_state,
            messages: status.messages,
            notifications: status.notifications,
            attention: None,
            accepted_events: status.accepted_events,
            rejected_events: status.rejected_events,
        }
    }

    pub fn clear_agent_messages(&self, agent: &str) -> RuntimeUpdate {
        self.core
            .lock()
            .expect("runtime core poisoned")
            .clear_agent_messages(agent)
    }

    pub fn clear_completed_task_notifications(&self) -> RuntimeUpdate {
        self.core
            .lock()
            .expect("runtime core poisoned")
            .clear_completed_task_notifications()
    }

    pub fn open_task_notification(&self, id: &str) -> Result<RuntimeUpdate, String> {
        self.core
            .lock()
            .expect("runtime core poisoned")
            .open_task_notification_with(id, |session_id| {
                task_opener::open_codex_task(session_id).map_err(|error| error.to_string())
            })
    }

    pub fn dismiss_task_notification(&self, id: &str) -> Result<RuntimeUpdate, String> {
        self.core
            .lock()
            .expect("runtime core poisoned")
            .dismiss_task_notification(id)
    }
}

impl Drop for RuntimeManager {
    fn drop(&mut self) {
        // Idempotent: if the quit handler already called shutdown(), this is
        // a no-op. Otherwise the same cleanup path runs here.
        self.shutdown();
    }
}

pub struct RuntimeCore {
    token: String,
    actions: Arc<ActionRegistry>,
    engine: EventStateEngine,
    queue: BoundedEventQueue,
    bucket: TokenBucket,
    messages: Vec<AgentMessage>,
    active_agents: HashSet<String>,
    task_notifications: TaskNotificationStore,
    latest_attention: Option<TaskAttention>,
    notification_path: Option<PathBuf>,
    logger: Option<RotatingLog>,
    accepted_events: u64,
    rejected_events: u64,
}

impl RuntimeCore {
    pub fn new(token: String) -> Self {
        Self::new_with_actions(token, Arc::new(ActionRegistry::default()))
    }

    pub fn new_with_actions(token: String, actions: Arc<ActionRegistry>) -> Self {
        Self {
            token,
            actions,
            engine: EventStateEngine::new(),
            queue: BoundedEventQueue::new(50),
            bucket: TokenBucket::new(30, 60),
            messages: Vec::new(),
            active_agents: HashSet::new(),
            task_notifications: TaskNotificationStore::default(),
            latest_attention: None,
            notification_path: None,
            logger: None,
            accepted_events: 0,
            rejected_events: 0,
        }
    }

    fn register_action(
        &mut self,
        request: ActionHookRequest,
        now_ms: u64,
    ) -> Result<ActionRegistration, String> {
        if request.agent != "codex" {
            return Err("unsupported_agent".to_string());
        }
        let session_id = json_text(&request.hook_input, "session_id")
            .or_else(|| json_text(&request.hook_input, "sessionId"))
            .ok_or_else(|| "missing_session_id".to_string())?;
        let turn_id = json_text(&request.hook_input, "turn_id")
            .or_else(|| json_text(&request.hook_input, "turnId"))
            .ok_or_else(|| "missing_turn_id".to_string())?;
        let expires_at_ms = now_ms.saturating_add(ACTION_TTL_MS);
        let (kind, action, event_kind) = match request.kind.as_str() {
            "permission.waiting" => {
                let tool_name = json_text(&request.hook_input, "tool_name")
                    .or_else(|| json_text(&request.hook_input, "toolName"))
                    .unwrap_or("unknown");
                let tool_input = request.hook_input.get("tool_input").cloned();
                let command = tool_input
                    .as_ref()
                    .and_then(|value| json_text(value, "command"));
                let description = tool_input
                    .as_ref()
                    .and_then(|value| json_text(value, "description"))
                    .unwrap_or("执行当前工具操作");
                let cwd = json_text(&request.hook_input, "cwd");
                let quick_allowed = tool_name != "unknown" && command.is_some();
                let id = self
                    .actions
                    .register(TaskActionKind::Permission, expires_at_ms);
                let action = TaskAction::permission_once(
                    &id,
                    description,
                    tool_name,
                    command,
                    cwd,
                    expires_at_ms,
                    quick_allowed,
                );
                (TaskActionKind::Permission, action, "permission.waiting")
            }
            "session.stop" => {
                let message = json_text(&request.hook_input, "last_assistant_message")
                    .or_else(|| json_text(&request.hook_input, "lastAssistantMessage"))
                    .unwrap_or_default();
                let stop_active = request
                    .hook_input
                    .get("stop_hook_active")
                    .or_else(|| request.hook_input.get("stopHookActive"))
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                if !allows_quick_continue(message, stop_active) {
                    return Err("quick_continue_not_allowed".to_string());
                }
                let id = self
                    .actions
                    .register(TaskActionKind::Continue, expires_at_ms);
                let action = TaskAction::continue_once(&id, "继续完成当前任务", expires_at_ms);
                (TaskActionKind::Continue, action, "session.waiting")
            }
            _ => return Err("unsupported_action_kind".to_string()),
        };
        let id = action.id.clone();
        let event = RuntimeEvent {
            agent: "codex".to_string(),
            kind: event_kind.to_string(),
            tool: action.tool_name.clone(),
            tool_input: None,
            session_id: Some(session_id.to_string()),
            turn_id: Some(turn_id.to_string()),
            task_title: None,
            summary: Some(action.requested_action.clone()),
            timestamp: None,
        };
        self.handle_event(
            Some(format!("Bearer {}", self.token).as_str()),
            event,
            now_ms,
        )
        .map_err(|_| "action_event_rejected".to_string())?;
        let notification_id = format!("codex:{session_id}:{turn_id}");
        if !self
            .task_notifications
            .attach_action(&notification_id, action)
        {
            let _ = self
                .actions
                .resolve(&id, TaskActionDecision::Fallback, now_ms);
            return Err("notification_not_found".to_string());
        }
        self.save_task_notifications(now_ms);
        let _ = kind;
        Ok(ActionRegistration {
            action_id: id,
            expires_at_ms,
        })
    }

    pub fn with_logger(mut self, logger: RotatingLog) -> Self {
        self.logger = Some(logger);
        self
    }

    pub fn with_task_notifications(
        mut self,
        task_notifications: TaskNotificationStore,
        notification_path: PathBuf,
    ) -> Self {
        self.task_notifications = task_notifications;
        self.notification_path = Some(notification_path);
        self
    }

    pub fn handle_event(
        &mut self,
        authorization: Option<&str>,
        event: RuntimeEvent,
        now_ms: u64,
    ) -> Result<DerivedPetState, RuntimeServerError> {
        if authorization != Some(format!("Bearer {}", self.token).as_str()) {
            self.rejected_events += 1;
            self.log_event("rejected_unauthorized", &event, now_ms, None, None);
            dev_log_runtime(
                "event.rejected",
                serde_json::json!({
                    "reason": "unauthorized",
                    "agent": &event.agent,
                    "kind": &event.kind,
                    "tool": &event.tool,
                    "rejectedEvents": self.rejected_events,
                }),
            );
            return Err(RuntimeServerError::Unauthorized);
        }

        if !self.bucket.allow(now_ms) {
            self.rejected_events += 1;
            self.log_event("rejected_rate_limited", &event, now_ms, None, None);
            dev_log_runtime(
                "event.rejected",
                serde_json::json!({
                    "reason": "rate_limited",
                    "agent": &event.agent,
                    "kind": &event.kind,
                    "tool": &event.tool,
                    "rejectedEvents": self.rejected_events,
                }),
            );
            return Err(RuntimeServerError::RateLimited);
        }

        if canonical_event_kind(&event.kind).is_none() {
            self.rejected_events += 1;
            self.log_event("rejected_unsupported", &event, now_ms, None, None);
            return Err(RuntimeServerError::UnsupportedEvent);
        }

        let event = normalize_runtime_event(event);
        self.latest_attention = None;
        if event.agent == "codex" && event.session_id.is_some() {
            let result = self.task_notifications.apply(event.clone(), now_ms);
            self.latest_attention = result.attention;
            self.save_task_notifications(now_ms);
        }
        let suppress_event = self.should_suppress_event(&event);
        let message = if suppress_event {
            None
        } else {
            self.message_for_event(&event, now_ms)
        };
        if !suppress_event {
            if let Some(message) = message.clone() {
                self.upsert_message(message);
            }
        }
        let mut latest = self.engine.current();
        if suppress_event {
            self.accepted_events += 1;
        } else {
            self.queue.push(event.clone());
            while let Some(event) = self.queue.pop_front() {
                latest = self.engine.apply_event(event, now_ms);
                self.accepted_events += 1;
            }
            self.record_agent_activity(&event);
        }
        self.log_event("accepted", &event, now_ms, message.as_ref(), Some(&latest));
        dev_log_runtime(
            "event.accepted",
            serde_json::json!({
                "agent": &event.agent,
                "kind": &event.kind,
                "tool": &event.tool,
                "message": &message,
                "currentState": latest,
                "messages": &self.messages,
                "acceptedEvents": self.accepted_events,
                "rejectedEvents": self.rejected_events,
            }),
        );

        Ok(self.current_state())
    }

    pub fn status(&self) -> RuntimeStatus {
        RuntimeStatus {
            current_state: self.current_state(),
            messages: self.messages.clone(),
            notifications: self
                .task_notifications
                .visible()
                .into_iter()
                .cloned()
                .collect(),
            attention: None,
            accepted_events: self.accepted_events,
            rejected_events: self.rejected_events,
        }
    }

    pub fn clear_agent_messages(&mut self, agent: &str) -> RuntimeUpdate {
        self.messages.retain(|message| message.agent != agent);
        self.active_agents.remove(agent);
        self.runtime_update(None)
    }

    pub fn clear_completed_task_notifications(&mut self) -> RuntimeUpdate {
        let changed = self.task_notifications.clear_completed();
        self.messages.retain(|message| !message.is_completion);
        if changed > 0 {
            self.save_task_notifications(now_ms());
        }
        self.latest_attention = None;
        self.runtime_update(None)
    }

    pub fn take_update(&mut self) -> RuntimeUpdate {
        let attention = self.latest_attention.take();
        self.runtime_update(attention)
    }

    fn runtime_update(&self, attention: Option<TaskAttention>) -> RuntimeUpdate {
        let status = self.status();
        RuntimeUpdate {
            current_state: status.current_state,
            messages: status.messages,
            notifications: status.notifications,
            attention,
        }
    }

    pub fn mark_task_notification_read(&mut self, id: &str) -> bool {
        let changed = self.task_notifications.mark_read(id);
        if changed {
            self.save_task_notifications(now_ms());
        }
        changed
    }

    pub fn open_task_notification_with(
        &mut self,
        id: &str,
        open: impl FnOnce(Option<&str>) -> Result<(), String>,
    ) -> Result<RuntimeUpdate, String> {
        let session_id = self
            .task_notifications
            .get(id)
            .ok_or_else(|| "任务提醒不存在".to_string())?
            .session_id
            .clone();
        open(session_id.as_deref())?;
        self.mark_task_notification_read(id);
        self.latest_attention = None;
        Ok(self.take_update())
    }

    pub fn dismiss_task_notification(&mut self, id: &str) -> Result<RuntimeUpdate, String> {
        if !self.task_notifications.dismiss(id) {
            return Err("任务提醒不存在".to_string());
        }
        self.save_task_notifications(now_ms());
        self.latest_attention = None;
        Ok(self.take_update())
    }

    pub fn advance_time(&mut self, now_ms: u64) -> DerivedPetState {
        self.engine.advance_time(now_ms);
        self.current_state()
    }

    fn current_state(&self) -> DerivedPetState {
        let mut current = self.engine.current();
        if let Some(state) = self.task_notifications.dominant_pet_state() {
            current.state = state;
            current.idle_after_ms = None;
        }
        current
    }

    fn save_task_notifications(&self, now_ms: u64) {
        let Some(path) = self.notification_path.as_deref() else {
            return;
        };
        if let Err(error) = self.task_notifications.save(path, now_ms) {
            eprintln!(
                "[copet:task-notifications:save] 无法保存 {}：{error}",
                path.display()
            );
        }
    }

    fn log_event(
        &self,
        outcome: &str,
        event: &RuntimeEvent,
        now_ms: u64,
        message: Option<&AgentMessage>,
        state: Option<&DerivedPetState>,
    ) {
        let Some(logger) = &self.logger else {
            return;
        };

        let line = serde_json::json!({
            "timestampMs": now_ms,
            "outcome": outcome,
            "agent": &event.agent,
            "kind": &event.kind,
            "tool": &event.tool,
            "sessionId": &event.session_id,
            "message": message,
            "currentState": state,
        })
        .to_string();
        let _ = logger.append_line(&line);
    }

    fn upsert_message(&mut self, message: AgentMessage) {
        if let Some(existing) = self
            .messages
            .iter_mut()
            .find(|existing| existing.agent == message.agent)
        {
            *existing = message;
            return;
        }

        self.messages.push(message);
    }

    fn message_for_event(&self, event: &RuntimeEvent, now_ms: u64) -> Option<AgentMessage> {
        if self.should_suppress_event(event) {
            return None;
        }

        agent_message_for_event(event, now_ms)
    }

    fn should_suppress_event(&self, event: &RuntimeEvent) -> bool {
        is_session_stop_kind(&event.kind) && !self.active_agents.contains(&event.agent)
    }

    fn record_agent_activity(&mut self, event: &RuntimeEvent) {
        if is_agent_activity_start_event(event) {
            self.active_agents.insert(event.agent.clone());
            return;
        }

        if is_session_stop_kind(&event.kind) || event.kind == "session.error" {
            self.active_agents.remove(&event.agent);
        }
    }
}

const ACTION_TTL_MS: u64 = 10 * 60 * 1_000;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActionHookRequest {
    agent: String,
    kind: String,
    hook_input: serde_json::Value,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ActionRegistration {
    action_id: String,
    expires_at_ms: u64,
}

fn json_text<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeServerError {
    Unauthorized,
    RateLimited,
    UnsupportedEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub current_state: DerivedPetState,
    pub messages: Vec<AgentMessage>,
    pub notifications: Vec<TaskNotification>,
    pub attention: Option<TaskAttention>,
    pub accepted_events: u64,
    pub rejected_events: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessage {
    pub agent: String,
    pub display_name: String,
    pub text: String,
    pub updated_at_ms: u64,
    #[serde(skip)]
    is_completion: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeUpdate {
    pub current_state: DerivedPetState,
    pub messages: Vec<AgentMessage>,
    pub notifications: Vec<TaskNotification>,
    pub attention: Option<TaskAttention>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSnapshot {
    pub port: u16,
    pub endpoint: String,
    pub current_state: DerivedPetState,
    pub messages: Vec<AgentMessage>,
    pub notifications: Vec<TaskNotification>,
    pub attention: Option<TaskAttention>,
    pub accepted_events: u64,
    pub rejected_events: u64,
}

fn agent_message_for_event(event: &RuntimeEvent, now_ms: u64) -> Option<AgentMessage> {
    let text = format_agent_message(event)?;
    Some(AgentMessage {
        agent: event.agent.clone(),
        display_name: agent_display_name(&event.agent).to_string(),
        text,
        updated_at_ms: now_ms,
        is_completion: is_session_stop_kind(&event.kind),
    })
}

fn is_session_stop_kind(kind: &str) -> bool {
    matches!(kind, "session.stop" | "session.end")
}

fn is_agent_activity_start_kind(kind: &str) -> bool {
    matches!(
        kind,
        "user.prompt" | "thinking" | "tool.before" | "permission.waiting" | "session.waiting"
    )
}

fn is_agent_activity_start_event(event: &RuntimeEvent) -> bool {
    if event.agent == "antigravity" && event.kind == "user.prompt" {
        return false;
    }

    is_agent_activity_start_kind(&event.kind)
}

fn format_agent_message(event: &RuntimeEvent) -> Option<String> {
    match event.kind.as_str() {
        "user.prompt" | "thinking" => Some(
            subject_message("Thinking", event.tool_input.as_ref())
                .unwrap_or_else(|| "Thinking...".to_string()),
        ),
        "permission.waiting" | "session.waiting" => Some("Waiting for you...".to_string()),
        kind if is_session_stop_kind(kind) => Some("Done.".to_string()),
        "session.error" => Some(
            subject_message("Error", event.tool_input.as_ref())
                .unwrap_or_else(|| "Error.".to_string()),
        ),
        "tool.after" if event.tool.is_none() && event.tool_input.is_none() => None,
        "tool.before" | "tool.after" => {
            let tool_name = event.tool.as_deref().unwrap_or("tool");
            Some(format_tool_message(
                tool_name,
                event.tool_input.as_ref(),
                event.kind == "tool.after",
            ))
        }
        _ => None,
    }
}

fn subject_message(prefix: &str, tool_input: Option<&serde_json::Value>) -> Option<String> {
    string_field(tool_input, "subject")
        .or_else(|| string_field(tool_input, "prompt"))
        .map(compact_whitespace)
        .filter(|subject| !subject.is_empty())
        .map(|subject| format!("{prefix}: {}", clip(&subject, 56)))
}

fn format_tool_message(
    tool_name: &str,
    tool_input: Option<&serde_json::Value>,
    past: bool,
) -> String {
    match canonical_tool_kind(tool_name) {
        "read" => {
            if let Some(name) = path_subject(tool_input) {
                return if past {
                    format!("Read {name}")
                } else {
                    format!("Reading {name}")
                };
            }
            if past {
                "Read file".to_string()
            } else {
                "Reading file".to_string()
            }
        }
        "edit" | "write" => {
            if let Some(name) = path_subject(tool_input) {
                return if past {
                    format!("Edited {name}")
                } else {
                    format!("Editing {name}")
                };
            }
            if past {
                "Edited file".to_string()
            } else {
                "Editing file".to_string()
            }
        }
        "bash" => {
            if let Some(command) = string_field(tool_input, "command") {
                let command = compact_whitespace(command);
                let command = clip(&command, 56);
                return if past {
                    format!("Ran {command}")
                } else {
                    format!("Running {command}")
                };
            }
            if past {
                "Ran command".to_string()
            } else {
                "Running command".to_string()
            }
        }
        "grep" => {
            if let Some(pattern) = string_field(tool_input, "pattern") {
                let pattern = clip(pattern, 28);
                return if past {
                    format!("Searched \"{pattern}\"")
                } else {
                    format!("Searching \"{pattern}\"")
                };
            }
            if past {
                "Searched files".to_string()
            } else {
                "Searching files".to_string()
            }
        }
        "glob" => {
            if let Some(pattern) = string_field(tool_input, "pattern") {
                let pattern = clip(pattern, 28);
                return if past {
                    format!("Listed {pattern}")
                } else {
                    format!("Listing {pattern}")
                };
            }
            if let Some(name) = path_subject(tool_input) {
                return if past {
                    format!("Listed {name}")
                } else {
                    format!("Listing {name}")
                };
            }
            if past {
                "Listed files".to_string()
            } else {
                "Listing files".to_string()
            }
        }
        "webfetch" => {
            if let Some(url) = string_field(tool_input, "url") {
                if let Some(host) = url_host(url) {
                    let host = clip(&host, 28);
                    return if past {
                        format!("Fetched {host}")
                    } else {
                        format!("Fetching {host}")
                    };
                }
            }
            if past {
                "Fetched web".to_string()
            } else {
                "Searching web".to_string()
            }
        }
        "task" => {
            if !past {
                if let Some(description) = string_field(tool_input, "description")
                    .or_else(|| string_field(tool_input, "subject"))
                {
                    return format!("Spawning {}", clip(description, 28));
                }
            }
            if past {
                "Subagent done".to_string()
            } else {
                "Spawning subagent".to_string()
            }
        }
        _ => {
            let name = clip(tool_name, 28);
            if past {
                format!("Called {name}")
            } else {
                format!("Calling {name}")
            }
        }
    }
}

fn canonical_tool_kind(tool_name: &str) -> &'static str {
    match tool_name.to_ascii_lowercase().as_str() {
        "read" | "view" | "view_file" => "read",
        "edit" | "multiedit" | "replace_file_content" | "multi_replace_file_content" => "edit",
        "write" | "create" | "write_to_file" => "write",
        "bash" | "shell" | "run_command" => "bash",
        "grep" | "grep_search" => "grep",
        "glob" | "list_dir" | "find_by_name" => "glob",
        "web_fetch" | "webfetch" | "websearch" | "search_web" | "read_url_content" => "webfetch",
        "task" | "agent" | "invoke_subagent" | "define_subagent" | "manage_subagents"
        | "send_message" => "task",
        _ => "unknown",
    }
}

fn path_subject(tool_input: Option<&serde_json::Value>) -> Option<String> {
    string_field(tool_input, "file_path")
        .or_else(|| string_field(tool_input, "filePath"))
        .or_else(|| string_field(tool_input, "file"))
        .or_else(|| string_field(tool_input, "path"))
        .map(|path| clip(&basename(path), 40))
}

fn string_field<'a>(value: Option<&'a serde_json::Value>, key: &str) -> Option<&'a str> {
    value?.get(key)?.as_str()
}

fn compact_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn basename(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

fn clip(text: &str, max: usize) -> String {
    let mut chars = text.chars();
    let clipped: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        let mut shortened: String = clipped.chars().take(max.saturating_sub(1)).collect();
        shortened.push('…');
        shortened
    } else {
        clipped
    }
}

fn url_host(url: &str) -> Option<String> {
    let without_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    without_scheme
        .split(['/', '?', '#'])
        .next()
        .filter(|host| !host.is_empty())
        .map(ToString::to_string)
}

struct ParsedHttpRequest {
    method: String,
    path: String,
    authorization: Option<String>,
    content_length: usize,
    body: Vec<u8>,
}

fn parse_http_request(request: &[u8]) -> Result<ParsedHttpRequest, u16> {
    let header_end = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or(400_u16)?;
    let headers = std::str::from_utf8(&request[..header_end]).map_err(|_| 400_u16)?;
    let mut lines = headers.lines();
    let request_line = lines.next().ok_or(400_u16)?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().ok_or(400_u16)?.to_string();
    let path = request_parts.next().ok_or(400_u16)?.to_string();
    let mut header_map = HashMap::new();

    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        header_map.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }

    let content_length = header_map
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let body_start = header_end + 4;
    let body_end = (body_start + content_length).min(request.len());
    let body = request[body_start..body_end].to_vec();

    Ok(ParsedHttpRequest {
        method,
        path,
        authorization: header_map.get("authorization").cloned(),
        content_length,
        body,
    })
}

fn handle_connection(
    mut stream: TcpStream,
    core: Arc<Mutex<RuntimeCore>>,
    on_state: &(dyn Fn(RuntimeUpdate) + Send + Sync),
) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(750)));
    let mut buffer = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 4096];

    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                buffer.extend_from_slice(&chunk[..count]);
                if request_is_complete(&buffer) || buffer.len() > MAX_EVENT_BODY_BYTES + 4096 {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(_) => break,
        }
    }

    if let Ok(request) = parse_http_request(&buffer) {
        if request.method == "GET" {
            if let Some(id) = action_decision_id(&request.path) {
                let (authorized, actions) = {
                    let core = core.lock().expect("runtime core poisoned");
                    (
                        request.authorization.as_deref()
                            == Some(format!("Bearer {}", core.token).as_str()),
                        Arc::clone(&core.actions),
                    )
                };
                let response = if !authorized {
                    response(401, r#"{"error":"unauthorized"}"#)
                } else {
                    action_wait_response(actions.wait_decision(
                        id,
                        now_ms(),
                        Duration::from_millis(ACTION_TTL_MS.saturating_sub(1_000)),
                    ))
                };
                let _ = stream.write_all(&response.into_bytes());
                return;
            }
        }
    }

    let mut core = core.lock().expect("runtime core poisoned");
    let response = handle_http_request(&mut core, &buffer, now_ms());
    if response.status_code == 202 {
        let update = core.take_update();
        dev_log_runtime(
            "tauri.emit.pet-state-changed",
            serde_json::json!({
                "currentState": &update.current_state,
                "messages": &update.messages,
                "notifications": &update.notifications,
                "attention": &update.attention,
            }),
        );
        on_state(update);
    }
    let _ = stream.write_all(&response.into_bytes());
}

fn action_decision_id(path: &str) -> Option<&str> {
    path.strip_prefix("/v1/actions/")?
        .strip_suffix("/decision")?
        .split('/')
        .next()
        .filter(|id| !id.is_empty())
}

fn action_wait_response(decision: WaitDecision) -> HttpResponse {
    match decision {
        WaitDecision::Resolved(decision) => response_json(
            200,
            &serde_json::json!({ "state": "resolved", "decision": decision }),
        ),
        WaitDecision::Expired => response_json(
            200,
            &serde_json::json!({ "state": "expired", "decision": "fallback" }),
        ),
        WaitDecision::Stale => response(404, r#"{"error":"stale_action"}"#),
        WaitDecision::Pending => response_json(202, &serde_json::json!({ "state": "pending" })),
    }
}

fn request_is_complete(buffer: &[u8]) -> bool {
    let Some(header_end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let Ok(headers) = std::str::from_utf8(&buffer[..header_end]) else {
        return true;
    };
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);

    buffer.len() >= header_end + 4 + content_length
}

impl HttpResponse {
    fn into_bytes(self) -> Vec<u8> {
        let reason = match self.status_code {
            202 => "Accepted",
            400 => "Bad Request",
            401 => "Unauthorized",
            404 => "Not Found",
            413 => "Payload Too Large",
            429 => "Too Many Requests",
            _ => "OK",
        };
        format!(
            "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.status_code,
            reason,
            self.body.len(),
            self.body
        )
        .into_bytes()
    }
}

fn response(status_code: u16, body: &str) -> HttpResponse {
    HttpResponse {
        status_code,
        body: body.to_string(),
    }
}

fn response_json(status_code: u16, body: &impl serde::Serialize) -> HttpResponse {
    HttpResponse {
        status_code,
        body: serde_json::to_string(body).unwrap_or_else(|_| r#"{"error":"json"}"#.to_string()),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(debug_assertions)]
fn dev_log_runtime(stage: &str, payload: serde_json::Value) {
    eprintln!("[copet:runtime:{stage}] {payload}");
}

#[cfg(not(debug_assertions))]
fn dev_log_runtime(_stage: &str, _payload: serde_json::Value) {}
