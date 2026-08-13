use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Condvar, Mutex},
    time::{Duration, Instant},
};

pub const CONTINUE_MARKER: &str = "<!-- copet:continue -->";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskActionKind {
    Continue,
    Permission,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskActionState {
    Pending,
    Resolving,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskActionDecision {
    ContinueOnce,
    AllowOnce,
    Fallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitDecision {
    Pending,
    Resolved(TaskActionDecision),
    Expired,
    Stale,
}

#[derive(Debug)]
struct ActionEntry {
    kind: TaskActionKind,
    expires_at_ms: u64,
    decision: Option<TaskActionDecision>,
}

#[derive(Debug, Default)]
pub struct ActionRegistry {
    entries: Mutex<HashMap<String, ActionEntry>>,
    changed: Condvar,
}

impl ActionRegistry {
    pub fn register(&self, kind: TaskActionKind, expires_at_ms: u64) -> String {
        let mut bytes = [0_u8; 16];
        let _ = getrandom::getrandom(&mut bytes);
        let id: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        self.entries
            .lock()
            .expect("action registry poisoned")
            .insert(
                id.clone(),
                ActionEntry {
                    kind,
                    expires_at_ms,
                    decision: None,
                },
            );
        id
    }

    pub fn resolve(
        &self,
        id: &str,
        decision: TaskActionDecision,
        now_ms: u64,
    ) -> Result<(), String> {
        let mut entries = self.entries.lock().expect("action registry poisoned");
        let entry = entries
            .get_mut(id)
            .ok_or_else(|| "任务操作已失效".to_string())?;
        if now_ms > entry.expires_at_ms {
            entries.remove(id);
            return Err("任务操作已过期".to_string());
        }
        if entry.decision.is_some() {
            return Err("任务操作已处理".to_string());
        }
        let compatible = matches!(
            (entry.kind, decision),
            (TaskActionKind::Continue, TaskActionDecision::ContinueOnce)
                | (TaskActionKind::Permission, TaskActionDecision::AllowOnce)
                | (_, TaskActionDecision::Fallback)
        );
        if !compatible {
            return Err("任务操作类型不匹配".to_string());
        }
        entry.decision = Some(decision);
        self.changed.notify_all();
        Ok(())
    }

    pub fn take_decision(&self, id: &str, now_ms: u64) -> WaitDecision {
        let mut entries = self.entries.lock().expect("action registry poisoned");
        let Some(entry) = entries.get(id) else {
            return WaitDecision::Stale;
        };
        if now_ms > entry.expires_at_ms {
            entries.remove(id);
            return WaitDecision::Expired;
        }
        let Some(decision) = entry.decision else {
            return WaitDecision::Pending;
        };
        entries.remove(id);
        WaitDecision::Resolved(decision)
    }

    pub fn wait_decision(&self, id: &str, now_ms: u64, timeout: Duration) -> WaitDecision {
        let deadline = Instant::now() + timeout;
        let mut entries = self.entries.lock().expect("action registry poisoned");
        loop {
            let Some(entry) = entries.get(id) else {
                return WaitDecision::Stale;
            };
            if now_ms > entry.expires_at_ms {
                entries.remove(id);
                return WaitDecision::Expired;
            }
            if let Some(decision) = entry.decision {
                entries.remove(id);
                return WaitDecision::Resolved(decision);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                entries.remove(id);
                return WaitDecision::Expired;
            }
            let (next, wait) = self
                .changed
                .wait_timeout(entries, remaining)
                .expect("action registry poisoned");
            entries = next;
            if wait.timed_out() {
                entries.remove(id);
                return WaitDecision::Expired;
            }
        }
    }

    pub fn resolve_all_fallback(&self) {
        let mut entries = self.entries.lock().expect("action registry poisoned");
        for entry in entries.values_mut() {
            if entry.decision.is_none() {
                entry.decision = Some(TaskActionDecision::Fallback);
            }
        }
        self.changed.notify_all();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskAction {
    pub id: String,
    pub kind: TaskActionKind,
    pub state: TaskActionState,
    pub label: String,
    pub requested_action: String,
    pub tool_name: Option<String>,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub expires_at_ms: u64,
    pub quick_action_allowed: bool,
}

impl TaskAction {
    pub fn continue_once(
        id: impl Into<String>,
        requested_action: impl Into<String>,
        expires_at_ms: u64,
    ) -> Self {
        Self {
            id: id.into(),
            kind: TaskActionKind::Continue,
            state: TaskActionState::Pending,
            label: "继续执行".to_string(),
            requested_action: requested_action.into(),
            tool_name: None,
            command: None,
            cwd: None,
            expires_at_ms,
            quick_action_allowed: true,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn permission_once(
        id: impl Into<String>,
        requested_action: impl Into<String>,
        tool_name: impl Into<String>,
        command: Option<&str>,
        cwd: Option<&str>,
        expires_at_ms: u64,
        quick_action_allowed: bool,
    ) -> Self {
        Self {
            id: id.into(),
            kind: TaskActionKind::Permission,
            state: TaskActionState::Pending,
            label: "允许并继续".to_string(),
            requested_action: requested_action.into(),
            tool_name: Some(tool_name.into()),
            command: command.map(redact_command_for_display),
            cwd: cwd.map(str::to_string),
            expires_at_ms,
            quick_action_allowed,
        }
    }

    pub fn expire(&mut self) {
        self.state = TaskActionState::Expired;
        self.quick_action_allowed = false;
    }
}

pub fn allows_quick_continue(message: &str, stop_hook_active: bool) -> bool {
    if stop_hook_active || !message.contains(CONTINUE_MARKER) {
        return false;
    }

    let normalized = message.to_ascii_lowercase();
    let excluded = [
        "请选择",
        "选择 a",
        "选择 b",
        "a 或 b",
        "a/b",
        "请提供",
        "请输入",
        "需要你决定",
        "永久允许",
        "始终允许",
        "确认删除",
        "生产数据库",
        "password",
        "credential",
        "secret",
        "choose ",
        "select ",
        "provide ",
        "enter ",
        "permanent",
        "always allow",
    ];

    !excluded.iter().any(|needle| normalized.contains(needle))
}

pub fn redact_command_for_display(command: &str) -> String {
    let mut redacted = command.to_string();
    for prefix in [
        "Bearer ",
        "bearer ",
        "api_key=",
        "api-key=",
        "token=",
        "password=",
    ] {
        redacted = redact_value_after(&redacted, prefix);
    }
    redacted
}

fn redact_value_after(input: &str, prefix: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut remaining = input;
    while let Some(index) = remaining.find(prefix) {
        let value_start = index + prefix.len();
        output.push_str(&remaining[..value_start]);
        output.push_str("[已隐藏]");
        let value = &remaining[value_start..];
        let value_end = value
            .find(|character: char| {
                character.is_whitespace() || matches!(character, '\'' | '"' | '&')
            })
            .unwrap_or(value.len());
        remaining = &value[value_end..];
    }
    output.push_str(remaining);
    output
}
