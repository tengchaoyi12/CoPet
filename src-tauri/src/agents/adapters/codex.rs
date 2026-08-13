use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};
use toml_edit::{value, DocumentMut, Item, Table};

use super::super::{
    hook_command, install_json_hooks, json_config_has_copet_hooks, read_json_object_optional,
    remove_json_hooks, write_atomic, write_json_atomic, AdapterError, AgentManager, CliAdapter,
    HookEvent, HELPER_NAME,
};

pub(super) static ADAPTER: CodexCliAdapter = CodexCliAdapter;

/// Codex 适配器
///
/// 该适配器负责与 Codex 集成：
/// - 修改文件: `~/.codex/hooks.json`
/// - 改动内容: 在该 JSON 文件中管理 `hooks` 列表。
///   CoPet 会向其中添加自定义钩子，以便在 Codex 执行任务（如提示提交、工具调用前后等）时，
///   触发 CoPet 的事件上报逻辑。
pub(super) struct CodexCliAdapter;

const EVENTS: &[HookEvent] = &[
    HookEvent {
        cli_event: "UserPromptSubmit",
        matcher: None,
        kind: "user.prompt",
    },
    HookEvent {
        cli_event: "PreToolUse",
        matcher: Some("*"),
        kind: "tool.before",
    },
    HookEvent {
        cli_event: "PostToolUse",
        matcher: Some("*"),
        kind: "tool.after",
    },
    HookEvent {
        cli_event: "PermissionRequest",
        matcher: Some("*"),
        kind: "permission.waiting",
    },
    HookEvent {
        cli_event: "Stop",
        matcher: None,
        kind: "session.stop",
    },
];

impl CliAdapter for CodexCliAdapter {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn display_name(&self) -> &'static str {
        "Codex"
    }

    fn config_path(&self, home: &Path) -> PathBuf {
        home.join(".codex").join("hooks.json")
    }

    fn is_installed(
        &self,
        _manager: &AgentManager,
        config_path: &Path,
    ) -> Result<bool, AdapterError> {
        codex_hooks_are_current(config_path)
    }

    fn install(&self, manager: &AgentManager) -> Result<(), AdapterError> {
        let hooks_path = self.config_path(manager.home());
        install_json_hooks(manager, self.id(), &hooks_path, EVENTS, 1)?;
        update_codex_config_toml(manager.home(), |document, config_path| {
            set_features_hooks_true(document, config_path)?;
            apply_trusted_hashes(document, &hooks_path, config_path)
        })
    }

    fn refresh(&self, manager: &AgentManager) -> Result<(), AdapterError> {
        let hooks_path = self.config_path(manager.home());
        refresh_codex_hooks_in_place(manager, &hooks_path)?;
        update_codex_config_toml(manager.home(), |document, config_path| {
            set_features_hooks_true(document, config_path)?;
            apply_trusted_hashes(document, &hooks_path, config_path)
        })
    }

    fn uninstall(&self, manager: &AgentManager) -> Result<(), AdapterError> {
        let hooks_path = self.config_path(manager.home());
        remove_json_hooks(manager, self.id(), &hooks_path)?;
        update_codex_config_toml(manager.home(), |document, _config_path| {
            remove_copet_trusted_hashes(document, &hooks_path);
            Ok(())
        })
    }

    fn executable_names(&self) -> &'static [&'static str] {
        &["codex"]
    }
}

fn refresh_codex_hooks_in_place(manager: &AgentManager, path: &Path) -> Result<(), AdapterError> {
    manager.backup_file("codex", path)?;
    let mut value = read_json_object_optional(path)?.unwrap_or_else(|| serde_json::json!({}));
    let object = value
        .as_object_mut()
        .ok_or_else(|| AdapterError::InvalidJson(path.to_path_buf()))?;
    let hooks = object
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| AdapterError::InvalidJson(path.to_path_buf()))?;

    for event in EVENTS {
        let timeout = if matches!(event.kind, "permission.waiting" | "session.stop") {
            600
        } else {
            1
        };
        let command = hook_command("codex", &manager.helper_path(), event.kind);
        let groups = hooks
            .entry(event.cli_event)
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .ok_or_else(|| AdapterError::InvalidJson(path.to_path_buf()))?;
        let mut refreshed = false;
        for group in groups.iter_mut() {
            let Some(handlers) = group
                .get_mut("hooks")
                .and_then(serde_json::Value::as_array_mut)
            else {
                continue;
            };
            let Some(handler) = handlers.iter_mut().find(|handler| {
                handler
                    .get("command")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|candidate| {
                        candidate.contains(HELPER_NAME)
                            && candidate.contains(&format!(" codex {}", event.kind))
                    })
            }) else {
                continue;
            };
            handler["type"] = serde_json::json!("command");
            handler["command"] = serde_json::json!(command);
            handler["timeout"] = serde_json::json!(timeout);
            handler["statusMessage"] = serde_json::json!("Updating CoPet");
            refreshed = true;
            break;
        }
        if !refreshed {
            let mut group = serde_json::json!({
                "hooks": [{
                    "type": "command",
                    "command": command,
                    "timeout": timeout,
                    "statusMessage": "Updating CoPet"
                }]
            });
            if let Some(matcher) = event.matcher {
                group["matcher"] = serde_json::json!(matcher);
            }
            groups.push(group);
        }
    }

    write_json_atomic(path, &value)
}

fn codex_hooks_are_current(path: &Path) -> Result<bool, AdapterError> {
    if !json_config_has_copet_hooks(path, "codex", EVENTS)? {
        return Ok(false);
    }
    let Some(value) = read_json_object_optional(path)? else {
        return Ok(false);
    };
    let Some(hooks) = value.get("hooks").and_then(serde_json::Value::as_object) else {
        return Ok(false);
    };

    Ok(EVENTS.iter().all(|event| {
        let expected_timeout = if matches!(event.kind, "permission.waiting" | "session.stop") {
            600
        } else {
            1
        };
        hooks
            .get(event.cli_event)
            .and_then(serde_json::Value::as_array)
            .is_some_and(|groups| {
                groups.iter().any(|group| {
                    if event.matcher.is_some()
                        && group.get("matcher").and_then(serde_json::Value::as_str) != event.matcher
                    {
                        return false;
                    }
                    group
                        .get("hooks")
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|handlers| {
                            handlers.iter().any(|handler| {
                                handler
                                    .get("command")
                                    .and_then(serde_json::Value::as_str)
                                    .is_some_and(|command| {
                                        command.contains(HELPER_NAME)
                                            && command.contains(&format!(" codex {}", event.kind))
                                    })
                                    && handler.get("timeout").and_then(serde_json::Value::as_u64)
                                        == Some(expected_timeout)
                            })
                        })
                })
            })
    }))
}

fn codex_config_path(home: &Path) -> PathBuf {
    home.join(".codex").join("config.toml")
}

/// Read ~/.codex/config.toml, let `update` mutate the parsed document,
/// write atomically only if the serialized output differs.
fn update_codex_config_toml<F>(home: &Path, update: F) -> Result<(), AdapterError>
where
    F: FnOnce(&mut DocumentMut, &Path) -> Result<(), AdapterError>,
{
    let path = codex_config_path(home);
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let mut document = if content.is_empty() {
        DocumentMut::new()
    } else {
        content
            .parse::<DocumentMut>()
            .map_err(|_| AdapterError::InvalidToml(path.clone()))?
    };
    update(&mut document, &path)?;
    let next = document.to_string();
    if next != content {
        write_atomic(&path, next.as_bytes())?;
    }
    Ok(())
}

/// Set [features].hooks = true, creating the table if needed, and drop the
/// deprecated `codex_hooks` feature flag if a previous install wrote it.
fn set_features_hooks_true(
    document: &mut DocumentMut,
    config_path: &Path,
) -> Result<(), AdapterError> {
    let features = document
        .entry("features")
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| AdapterError::InvalidToml(config_path.to_path_buf()))?;
    features.insert("hooks", value(true));
    features.remove("codex_hooks");
    Ok(())
}

/// Compact description of a single CoPet-owned Codex hook handler.
/// Mirrors the inputs Codex feeds into command_hook_hash.
struct CoPetCodexHandler<'a> {
    event_label: &'a str, // "pre_tool_use" / "user_prompt_submit" / ...
    matcher: Option<&'a str>,
    command: &'a str,
    timeout_sec: u64, // already .max(1)
    status_message: Option<&'a str>,
    // r#async stays false (CoPet never writes async)
}

/// Vendored from openai/codex-rs/hooks/src/engine/discovery.rs:538 (NormalizedHookIdentity)
/// + config/src/hook_config.rs (HookHandlerConfig::Command / MatcherGroup).
///   Field renames must mirror the source serde tags exactly so the
///   canonical JSON keys match Codex byte-for-byte.
#[derive(Serialize)]
struct NormalizedHookIdentity<'a> {
    event_name: &'a str,
    #[serde(flatten)]
    group: VendoredMatcherGroup<'a>,
}

#[derive(Serialize)]
struct VendoredMatcherGroup<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    matcher: Option<&'a str>,
    hooks: [VendoredHookHandlerConfig<'a>; 1],
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum VendoredHookHandlerConfig<'a> {
    #[serde(rename = "command")]
    Command {
        command: &'a str,
        #[serde(rename = "commandWindows", skip_serializing_if = "Option::is_none")]
        command_windows: Option<&'a str>,
        #[serde(rename = "timeout", skip_serializing_if = "Option::is_none")]
        timeout_sec: Option<u64>,
        r#async: bool,
        #[serde(rename = "statusMessage", skip_serializing_if = "Option::is_none")]
        status_message: Option<&'a str>,
    },
}

/// Replicates command_hook_hash → version_for_toml from openai/codex-rs.
fn compute_trusted_hash(handler: &CoPetCodexHandler) -> Result<String, AdapterError> {
    let identity = NormalizedHookIdentity {
        event_name: handler.event_label,
        group: VendoredMatcherGroup {
            matcher: handler.matcher,
            hooks: [VendoredHookHandlerConfig::Command {
                command: handler.command,
                command_windows: None,
                timeout_sec: Some(handler.timeout_sec.max(1)),
                r#async: false,
                status_message: handler.status_message,
            }],
        },
    };
    let toml_value = toml::Value::try_from(&identity)
        .map_err(|error| AdapterError::HookHash(error.to_string()))?;
    let json_value = serde_json::to_value(&toml_value)
        .map_err(|error| AdapterError::HookHash(error.to_string()))?;
    let canonical = canonical_json(&json_value);
    let bytes = serde_json::to_vec(&canonical)
        .map_err(|error| AdapterError::HookHash(error.to_string()))?;
    let digest = Sha256::digest(&bytes);
    let hex = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    Ok(format!("sha256:{hex}"))
}

/// Vendored from openai/codex-rs/config/src/fingerprint.rs:51.
/// Recursively sorts object keys; arrays and scalars unchanged.
fn canonical_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut sorted = serde_json::Map::new();
            for key in keys {
                if let Some(child) = map.get(key) {
                    sorted.insert(key.clone(), canonical_json(child));
                }
            }
            serde_json::Value::Object(sorted)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(canonical_json).collect())
        }
        other => other.clone(),
    }
}

/// Mirrors hook_key from openai/codex-rs/hooks/src/lib.rs:91.
fn hook_state_key(
    hooks_file_abs_path: &Path,
    event_label: &str,
    group_index: usize,
    handler_index: usize,
) -> String {
    format!(
        "{}:{event_label}:{group_index}:{handler_index}",
        hooks_file_abs_path.display()
    )
}

/// Codex hook_event_key_label snake-case label for each Codex `cli_event` CoPet uses.
fn cli_event_to_label(cli_event: &str) -> Option<&'static str> {
    match cli_event {
        "PreToolUse" => Some("pre_tool_use"),
        "PermissionRequest" => Some("permission_request"),
        "PostToolUse" => Some("post_tool_use"),
        "PreCompact" => Some("pre_compact"),
        "PostCompact" => Some("post_compact"),
        "SessionStart" => Some("session_start"),
        "UserPromptSubmit" => Some("user_prompt_submit"),
        "Stop" => Some("stop"),
        _ => None, // Notification etc. — Codex doesn't recognize, skip
    }
}

/// Read the just-written hooks.json, derive one trust entry per handler,
/// and merge into `[hooks.state."<key>"].trusted_hash`. Leaves unrelated
/// state entries intact.
fn apply_trusted_hashes(
    document: &mut DocumentMut,
    hooks_file_abs_path: &Path,
    config_path: &Path,
) -> Result<(), AdapterError> {
    let hooks_content = std::fs::read(hooks_file_abs_path)?;
    let hooks_json: serde_json::Value = serde_json::from_slice(&hooks_content)
        .map_err(|_| AdapterError::InvalidJson(hooks_file_abs_path.to_path_buf()))?;

    let hooks_table = hooks_json
        .get("hooks")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();

    // Ensure [hooks] and [hooks.state] tables exist in DocumentMut.
    let hooks_doc_table = document
        .entry("hooks")
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| AdapterError::InvalidToml(config_path.to_path_buf()))?;
    // Allow [hooks] to dot into [hooks.state] cleanly.
    hooks_doc_table.set_implicit(true);

    let state_table = hooks_doc_table
        .entry("state")
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| AdapterError::InvalidToml(config_path.to_path_buf()))?;

    for (cli_event, groups) in hooks_table.iter() {
        let Some(event_label) = cli_event_to_label(cli_event.as_str()) else {
            continue;
        };
        let Some(groups) = groups.as_array() else {
            continue;
        };
        // Iterate every group/handler CoPet wrote (today: exactly 1 group, 1 handler each).
        for (group_index, group) in groups.iter().enumerate() {
            let matcher = group.get("matcher").and_then(serde_json::Value::as_str);
            let Some(handlers) = group.get("hooks").and_then(serde_json::Value::as_array) else {
                continue;
            };
            for (handler_index, handler) in handlers.iter().enumerate() {
                let Some(command) = handler.get("command").and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                // Only own hooks CoPet authored (defensive: helper name in command).
                if !command.contains(crate::agents::HELPER_NAME) {
                    continue;
                }
                let timeout_sec = handler
                    .get("timeout")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(600);
                let status_message = handler
                    .get("statusMessage")
                    .and_then(serde_json::Value::as_str);
                let descriptor = CoPetCodexHandler {
                    event_label,
                    matcher,
                    command,
                    timeout_sec,
                    status_message,
                };
                let key =
                    hook_state_key(hooks_file_abs_path, event_label, group_index, handler_index);
                let trusted_hash = compute_trusted_hash(&descriptor)?;
                let entry = state_table
                    .entry(&key)
                    .or_insert_with(|| Item::Table(Table::new()))
                    .as_table_mut()
                    .ok_or_else(|| AdapterError::InvalidToml(config_path.to_path_buf()))?;
                entry.insert("trusted_hash", value(trusted_hash));
            }
        }
    }
    Ok(())
}

/// Drop every [hooks.state."<key>"] entry whose key starts with our hooks.json
/// absolute path followed by `:`. Leaves unrelated user-owned state alone.
fn remove_copet_trusted_hashes(document: &mut DocumentMut, hooks_file_abs_path: &Path) {
    let prefix = format!("{}:", hooks_file_abs_path.display());
    let Some(hooks_item) = document.get_mut("hooks") else {
        return;
    };
    let Some(hooks_table) = hooks_item.as_table_mut() else {
        return;
    };
    let Some(state_item) = hooks_table.get_mut("state") else {
        return;
    };
    let Some(state_table) = state_item.as_table_mut() else {
        return;
    };
    let owned_keys: Vec<String> = state_table
        .iter()
        .filter(|(key, _)| key.starts_with(&prefix))
        .map(|(key, _)| key.to_string())
        .collect();
    for key in owned_keys {
        state_table.remove(&key);
    }
    // If [hooks.state] becomes empty, drop it; if [hooks] becomes empty too, drop it.
    if state_table.is_empty() {
        hooks_table.remove("state");
    }
    if hooks_table.is_empty() {
        document.remove("hooks");
    }
}
