use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::runtime_state::{agent_display_name, normalize_runtime_event, PetStateId, RuntimeEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    Running,
    Waiting,
    Completed,
    Failed,
}

impl TaskStatus {
    fn priority(self) -> u8 {
        match self {
            Self::Waiting => 4,
            Self::Failed => 3,
            Self::Completed => 2,
            Self::Running => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AttentionKind {
    Waiting,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskNotification {
    pub id: String,
    pub agent: String,
    pub display_name: String,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub status: TaskStatus,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub unread: bool,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskAttention {
    pub id: String,
    pub kind: AttentionKind,
    pub occurred_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyTaskResult {
    pub attention: Option<TaskAttention>,
    pub dominant_state: Option<PetStateId>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskNotificationStore {
    notifications: BTreeMap<String, TaskNotification>,
}

impl TaskNotificationStore {
    pub fn apply(&mut self, event: RuntimeEvent, now_ms: u64) -> ApplyTaskResult {
        let event = normalize_runtime_event(event);
        let Some(status) = status_for_event(&event) else {
            return ApplyTaskResult {
                attention: None,
                dominant_state: self.dominant_pet_state(),
            };
        };
        let id = task_id(&event);
        let previous_status = self.notifications.get(&id).map(|task| task.status);
        let attention_kind = attention_kind(status);
        let attention = (previous_status != Some(status))
            .then_some(attention_kind)
            .flatten()
            .map(|kind| TaskAttention {
                id: id.clone(),
                kind,
                occurred_at_ms: now_ms,
            });

        let existing = self.notifications.remove(&id);
        let title = event
            .task_title
            .or_else(|| existing.as_ref().and_then(|task| task.title.clone()));
        let summary = event
            .summary
            .or_else(|| existing.as_ref().and_then(|task| task.summary.clone()));
        let unread = attention.is_some()
            || existing
                .as_ref()
                .is_some_and(|task| task.unread && task.status == status);
        self.notifications.insert(
            id.clone(),
            TaskNotification {
                id,
                agent: event.agent.clone(),
                display_name: agent_display_name(&event.agent).to_string(),
                session_id: event.session_id,
                turn_id: event.turn_id,
                status,
                title,
                summary,
                unread,
                updated_at_ms: now_ms,
            },
        );

        ApplyTaskResult {
            attention,
            dominant_state: self.dominant_pet_state(),
        }
    }

    pub fn visible(&self) -> Vec<&TaskNotification> {
        let mut visible = self
            .notifications
            .values()
            .filter(|task| task.unread || task.status == TaskStatus::Running)
            .collect::<Vec<_>>();
        visible.sort_by(|left, right| {
            right
                .status
                .priority()
                .cmp(&left.status.priority())
                .then_with(|| right.updated_at_ms.cmp(&left.updated_at_ms))
                .then_with(|| left.id.cmp(&right.id))
        });
        visible
    }

    pub fn dominant_status(&self) -> Option<TaskStatus> {
        self.visible().first().map(|task| task.status)
    }

    pub fn dominant_pet_state(&self) -> Option<PetStateId> {
        match self.dominant_status()? {
            TaskStatus::Waiting => Some(PetStateId::Waiting),
            TaskStatus::Failed => Some(PetStateId::Failed),
            TaskStatus::Completed => Some(PetStateId::Waving),
            TaskStatus::Running => Some(PetStateId::Running),
        }
    }

    pub fn mark_read(&mut self, id: &str) -> bool {
        let Some(task) = self.notifications.get_mut(id) else {
            return false;
        };
        task.unread = false;
        true
    }

    pub fn dismiss(&mut self, id: &str) -> bool {
        self.notifications.remove(id).is_some()
    }
}

fn status_for_event(event: &RuntimeEvent) -> Option<TaskStatus> {
    match event.kind.as_str() {
        "user.prompt" | "thinking" | "tool.before" | "tool.after" => Some(TaskStatus::Running),
        "permission.waiting" | "session.waiting" => Some(TaskStatus::Waiting),
        "session.stop" | "session.end" => Some(TaskStatus::Completed),
        "session.error" => Some(TaskStatus::Failed),
        _ => None,
    }
}

fn attention_kind(status: TaskStatus) -> Option<AttentionKind> {
    match status {
        TaskStatus::Waiting => Some(AttentionKind::Waiting),
        TaskStatus::Completed => Some(AttentionKind::Completed),
        TaskStatus::Failed => Some(AttentionKind::Failed),
        TaskStatus::Running => None,
    }
}

fn task_id(event: &RuntimeEvent) -> String {
    let session = event.session_id.as_deref().unwrap_or("unknown");
    let turn = event.turn_id.as_deref().unwrap_or("session");
    format!("{}:{session}:{turn}", event.agent)
}
