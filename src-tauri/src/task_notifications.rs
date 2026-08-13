use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::runtime_state::{agent_display_name, normalize_runtime_event, PetStateId, RuntimeEvent};
use crate::task_actions::TaskAction;

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
    #[serde(default)]
    pub action: Option<TaskAction>,
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

const MAX_STORED_NOTIFICATIONS: usize = 100;
const NOTIFICATION_RETENTION_MS: u64 = 30 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskNotificationStore {
    notifications: BTreeMap<String, TaskNotification>,
}

impl TaskNotificationStore {
    pub fn load(path: &Path, now_ms: u64) -> io::Result<Self> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => {
                eprintln!(
                    "[copet:task-notifications:load] 无法读取 {}：{error}",
                    path.display()
                );
                return Ok(Self::default());
            }
        };
        let mut store = match serde_json::from_slice::<Self>(&bytes) {
            Ok(store) => store,
            Err(error) => {
                eprintln!(
                    "[copet:task-notifications:load] 无法解析 {}：{error}",
                    path.display()
                );
                return Ok(Self::default());
            }
        };
        store.prune_for_persistence(now_ms);
        Ok(store)
    }

    pub fn save(&self, path: &Path, now_ms: u64) -> io::Result<()> {
        let mut persisted = self.clone();
        persisted.prune_for_persistence(now_ms);
        let bytes = serde_json::to_vec_pretty(&persisted).map_err(io::Error::other)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary_path = path.with_extension("json.tmp");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary_path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(temporary_path, path)
    }

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
        let action = existing.as_ref().and_then(|task| task.action.clone());
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
                action,
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

    pub fn get(&self, id: &str) -> Option<&TaskNotification> {
        self.notifications.get(id)
    }

    pub fn attach_action(&mut self, id: &str, action: TaskAction) -> bool {
        let Some(task) = self.notifications.get_mut(id) else {
            return false;
        };
        task.action = Some(action);
        true
    }

    pub fn dismiss(&mut self, id: &str) -> bool {
        self.notifications.remove(id).is_some()
    }

    pub fn clear_completed(&mut self) -> usize {
        let before = self.notifications.len();
        self.notifications
            .retain(|_, task| task.status != TaskStatus::Completed);
        before - self.notifications.len()
    }

    fn prune_for_persistence(&mut self, now_ms: u64) {
        for task in self.notifications.values_mut() {
            if let Some(action) = task.action.as_mut() {
                action.expire();
            }
        }
        let mut retained = self
            .notifications
            .values()
            .filter(|task| {
                task.unread
                    && task.status != TaskStatus::Running
                    && now_ms.saturating_sub(task.updated_at_ms) <= NOTIFICATION_RETENTION_MS
            })
            .cloned()
            .collect::<Vec<_>>();
        retained.sort_by(|left, right| {
            right
                .updated_at_ms
                .cmp(&left.updated_at_ms)
                .then_with(|| left.id.cmp(&right.id))
        });
        retained.truncate(MAX_STORED_NOTIFICATIONS);
        self.notifications = retained
            .into_iter()
            .map(|task| (task.id.clone(), task))
            .collect();
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
