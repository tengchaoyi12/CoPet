use serde::{Deserialize, Serialize};

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
