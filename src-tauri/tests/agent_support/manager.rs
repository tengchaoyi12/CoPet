use super::helpers::{
    manager_with_fake_agent_names, manager_with_fake_agents, read_json, with_cleared_copilot_home,
    with_opencode_config_dir,
};
use copet_lib::{
    agents::AgentManager, config_store::ConfigStore, ensure_app_adapter_supported,
    run_agent_auto_install_once,
};
use std::fs;

#[test]
fn app_command_boundary_accepts_only_codex() {
    assert!(ensure_app_adapter_supported("codex").is_ok());

    for adapter_id in ["claude-code", "gemini", "cursor", "unknown"] {
        let error = ensure_app_adapter_supported(adapter_id).unwrap_err();
        assert!(error.contains("Codex"), "{adapter_id}: {error}");
    }
}

#[test]
fn list_exposes_each_platform_adapter() {
    with_cleared_copilot_home(|| {
        let temp = tempfile::tempdir().unwrap();
        let manager = AgentManager::new(temp.path().join(".copet"), temp.path().join("home"));

        let adapters = manager
            .list()
            .unwrap()
            .into_iter()
            .map(|adapter| (adapter.id, adapter.display_name))
            .collect::<Vec<_>>();

        assert_eq!(
            adapters,
            [
                ("claude-code".to_string(), "Claude Code".to_string()),
                ("codex".to_string(), "Codex".to_string()),
                ("antigravity".to_string(), "Antigravity".to_string()),
                ("opencode".to_string(), "OpenCode".to_string()),
                ("cursor".to_string(), "Cursor".to_string()),
                ("copilot".to_string(), "Copilot CLI".to_string()),
                ("pi".to_string(), "Pi".to_string()),
                ("gemini".to_string(), "Gemini".to_string()),
            ]
        );
    });
}

#[test]
fn adapters_install_repair_and_uninstall_real_config_files() {
    with_opencode_config_dir(|opencode_config_dir| {
        with_cleared_copilot_home(|| {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join(".copet");
            let home = temp.path().join("home");
            let manager = manager_with_fake_agents(&root, &home);

            for adapter_id in [
                "codex",
                "cursor",
                "claude-code",
                "antigravity",
                "opencode",
                "copilot",
                "gemini",
                "pi",
            ] {
                let installed = manager.install(adapter_id).unwrap();
                assert!(installed.adapter.installed, "{adapter_id} should install");
                assert!(
                    root.join("adapters")
                        .join(format!("{adapter_id}.json"))
                        .exists(),
                    "{adapter_id} should write adapter metadata"
                );
                assert!(
                    root.join("hooks/copet-hook.sh").exists(),
                    "{adapter_id} should ensure the shared helper"
                );
                assert_adapter_config_contains_marker(adapter_id, &home, opencode_config_dir);

                let repaired = manager.repair(adapter_id).unwrap();
                assert!(repaired.adapter.installed, "{adapter_id} should repair");
                assert_adapter_config_contains_marker(adapter_id, &home, opencode_config_dir);

                let uninstalled = manager.uninstall(adapter_id).unwrap();
                assert!(
                    !uninstalled.adapter.installed,
                    "{adapter_id} should report uninstalled"
                );
                assert!(
                    !root
                        .join("adapters")
                        .join(format!("{adapter_id}.json"))
                        .exists(),
                    "{adapter_id} should remove adapter metadata"
                );
                assert_adapter_config_does_not_contain_marker(
                    adapter_id,
                    &home,
                    opencode_config_dir,
                );
            }
        });
    });
}

fn assert_adapter_config_contains_marker(
    adapter_id: &str,
    home: &std::path::Path,
    opencode_config_dir: &std::path::Path,
) {
    match adapter_id {
        "codex" => {
            let value = read_json(home.join(".codex/hooks.json"));
            assert!(value.to_string().contains("copet-hook.sh"));
            assert!(value.to_string().contains("codex"));
            let config = fs::read_to_string(home.join(".codex/config.toml")).unwrap();
            assert!(config.contains("hooks = true"));
        }
        "cursor" => {
            let value = read_json(home.join(".cursor/hooks.json"));
            assert!(value.to_string().contains("copet-hook.sh"));
            assert!(value.to_string().contains("cursor"));
        }
        "claude-code" => {
            let value = read_json(home.join(".claude/settings.json"));
            assert!(value.to_string().contains("copet-hook.sh"));
            assert!(value.to_string().contains("claude-code"));
        }
        "gemini" => {
            let value = read_json(home.join(".gemini/settings.json"));
            assert!(value.to_string().contains("copet-hook.sh"));
            assert!(value.to_string().contains("gemini"));
        }
        "antigravity" => {
            let value = read_json(home.join(".gemini/config/hooks.json"));
            assert!(value.to_string().contains("copet-hook.sh"));
            assert!(value.to_string().contains("antigravity"));
            assert!(value.to_string().contains("copet-antigravity"));
        }
        "opencode" => {
            let content = fs::read_to_string(opencode_config_dir.join("plugins/copet.js")).unwrap();
            assert!(content.contains("copet-managed-hook"));
        }
        "copilot" => {
            let value = read_json(home.join(".copilot/hooks/copet.json"));
            assert!(value.to_string().contains("copet-hook.sh"));
            assert!(value.to_string().contains("copilot"));
        }
        "pi" => {
            let content =
                fs::read_to_string(home.join(".pi/agent/extensions/copet/index.ts")).unwrap();
            assert!(content.contains("copetPiExtension"));
            let marker = read_json(home.join(".pi/agent/extensions/copet/.copet-managed.json"));
            assert_eq!(marker["managed"], true);
        }
        _ => unreachable!("unknown adapter"),
    }
}

fn assert_adapter_config_does_not_contain_marker(
    adapter_id: &str,
    home: &std::path::Path,
    opencode_config_dir: &std::path::Path,
) {
    match adapter_id {
        "codex" => assert_json_file_lacks_marker(home.join(".codex/hooks.json")),
        "cursor" => assert_json_file_lacks_marker(home.join(".cursor/hooks.json")),
        "claude-code" => assert_json_file_lacks_marker(home.join(".claude/settings.json")),
        "gemini" => assert_json_file_lacks_marker(home.join(".gemini/settings.json")),
        "antigravity" => {
            let content =
                fs::read_to_string(home.join(".gemini/config/hooks.json")).unwrap_or_default();
            assert!(!content.contains("copet-antigravity"));
        }
        "opencode" => assert!(!opencode_config_dir.join("plugins/copet.js").exists()),
        "copilot" => assert!(!home.join(".copilot/hooks/copet.json").exists()),
        "pi" => assert!(!home.join(".pi/agent/extensions/copet").exists()),
        _ => unreachable!("unknown adapter"),
    }
}

fn assert_json_file_lacks_marker(path: impl AsRef<std::path::Path>) {
    let content = fs::read_to_string(path).unwrap_or_default();
    assert!(!content.contains("copet-hook.sh"));
}

#[test]
fn auto_install_detected_agents_installs_only_available_cli_adapters() {
    with_cleared_copilot_home(|| {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".copet");
        let home = temp.path().join("home");
        let manager = manager_with_fake_agent_names(
            &root,
            &home,
            &["codex", "cursor", "agy", "copilot", "gemini", "pi"],
        );

        let summary = manager.auto_install_detected_agents();

        assert_eq!(
            summary.installed,
            vec![
                "codex".to_string(),
                "antigravity".to_string(),
                "cursor".to_string(),
                "copilot".to_string(),
                "pi".to_string(),
                "gemini".to_string()
            ]
        );
        assert_eq!(
            summary.skipped,
            vec!["claude-code".to_string(), "opencode".to_string()]
        );
        assert!(summary.failed.is_empty());
        assert!(home.join(".codex/hooks.json").exists());
        assert!(home.join(".cursor/hooks.json").exists());
        assert!(home.join(".gemini/config/hooks.json").exists());
        assert!(home.join(".copilot/hooks/copet.json").exists());
        assert!(home.join(".gemini/settings.json").exists());
        assert!(home.join(".pi/agent/extensions/copet/index.ts").exists());
        assert!(!home.join(".claude/settings.json").exists());
        assert!(!home.join(".config/opencode/plugins/copet.js").exists());
    });
}

#[test]
fn app_auto_install_targets_codex_only() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".copet");
    let home = temp.path().join("home");
    let store = ConfigStore::new(&root);
    store.ensure_ready().unwrap();
    let manager = manager_with_fake_agent_names(&root, &home, &["codex", "claude", "gemini"]);

    let summary = run_agent_auto_install_once(&store, &manager).unwrap();

    assert_eq!(summary.installed, vec!["codex".to_string()]);
    assert!(home.join(".codex/hooks.json").exists());
    assert!(!home.join(".claude/settings.json").exists());
    assert!(!home.join(".gemini/settings.json").exists());
}

#[test]
fn auto_install_detected_agents_skips_already_installed_hooks_without_rewriting() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".copet");
    let home = temp.path().join("home");
    let manager = manager_with_fake_agent_names(&root, &home, &["codex"]);

    manager.install("codex").unwrap();
    let hooks_path = home.join(".codex/hooks.json");
    let config_path = home.join(".codex/config.toml");
    let hooks_before = fs::read_to_string(&hooks_path).unwrap();
    let config_before = fs::read_to_string(&config_path).unwrap();

    let summary = manager.auto_install_detected_agents();

    assert!(summary.installed.is_empty());
    assert!(summary.failed.is_empty());
    assert!(summary.skipped.contains(&"codex".to_string()));
    assert_eq!(fs::read_to_string(&hooks_path).unwrap(), hooks_before);
    assert_eq!(fs::read_to_string(&config_path).unwrap(), config_before);
}

#[test]
fn auto_install_detected_agents_continues_after_adapter_failure() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".copet");
    let home = temp.path().join("home");
    let claude_settings = home.join(".claude/settings.json");
    fs::create_dir_all(claude_settings.parent().unwrap()).unwrap();
    fs::write(&claude_settings, "{not valid json").unwrap();
    let manager = manager_with_fake_agent_names(&root, &home, &["claude", "codex", "agy"]);

    let summary = manager.auto_install_detected_agents();

    assert_eq!(
        summary.installed,
        vec!["codex".to_string(), "antigravity".to_string()]
    );
    assert_eq!(summary.failed.len(), 1);
    assert_eq!(summary.failed[0].adapter_id, "claude-code");
    assert!(summary.failed[0].error.contains("invalid JSON"));
    assert!(home.join(".codex/hooks.json").exists());
    assert!(home.join(".gemini/config/hooks.json").exists());
}

#[test]
fn run_agent_auto_install_once_sets_completion_marker_and_preserves_later_uninstall() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".copet");
    let home = temp.path().join("home");
    let store = ConfigStore::new(&root);
    store.ensure_ready().unwrap();
    let manager = manager_with_fake_agent_names(&root, &home, &["codex"]);

    let first = run_agent_auto_install_once(&store, &manager).unwrap();

    assert_eq!(first.installed, vec!["codex".to_string()]);
    assert!(store.agent_auto_install_complete().unwrap());
    assert!(home.join(".codex/hooks.json").exists());

    manager.uninstall("codex").unwrap();
    let second = run_agent_auto_install_once(&store, &manager).unwrap();

    assert!(second.installed.is_empty());
    assert!(second.skipped.is_empty());
    assert!(second.failed.is_empty());
    let hooks = fs::read_to_string(home.join(".codex/hooks.json")).unwrap_or_default();
    assert!(!hooks.contains("copet-hook.sh"));
}

#[test]
fn run_agent_auto_install_once_restores_helper_when_missing_after_completion() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".copet");
    let home = temp.path().join("home");
    let store = ConfigStore::new(&root);
    store.ensure_ready().unwrap();
    let manager = manager_with_fake_agent_names(&root, &home, &["codex"]);

    run_agent_auto_install_once(&store, &manager).unwrap();
    let helper = root.join("hooks/copet-hook.sh");
    assert!(helper.exists());
    assert!(store.agent_auto_install_complete().unwrap());

    fs::remove_file(&helper).unwrap();
    assert!(!helper.exists());

    let summary = run_agent_auto_install_once(&store, &manager).unwrap();

    assert!(
        helper.exists(),
        "helper script should be restored on launch"
    );
    assert!(summary.installed.is_empty());
    assert!(summary.failed.is_empty());
    assert!(store.agent_auto_install_complete().unwrap());
}

#[test]
fn run_agent_auto_install_once_repairs_stale_codex_action_timeouts_after_completion() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".copet");
    let home = temp.path().join("home");
    let store = ConfigStore::new(&root);
    store.ensure_ready().unwrap();
    let manager = manager_with_fake_agent_names(&root, &home, &["codex"]);

    run_agent_auto_install_once(&store, &manager).unwrap();
    let hooks_path = home.join(".codex/hooks.json");
    let mut hooks = read_json(&hooks_path);
    hooks["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"] = 1.into();
    hooks["hooks"]["Stop"][0]["hooks"][0]["timeout"] = 1.into();
    fs::write(&hooks_path, serde_json::to_vec_pretty(&hooks).unwrap()).unwrap();

    let summary = run_agent_auto_install_once(&store, &manager).unwrap();

    assert_eq!(summary.installed, vec!["codex".to_string()]);
    assert!(summary.failed.is_empty());
    let repaired = read_json(&hooks_path);
    assert_eq!(
        repaired["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"],
        600
    );
    assert_eq!(repaired["hooks"]["Stop"][0]["hooks"][0]["timeout"], 600);
}

#[test]
fn run_agent_auto_install_once_refreshes_existing_legacy_helper_during_codex_migration() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".copet");
    let home = temp.path().join("home");
    let store = ConfigStore::new(&root);
    store.ensure_ready().unwrap();
    let manager = manager_with_fake_agent_names(&root, &home, &["codex"]);

    run_agent_auto_install_once(&store, &manager).unwrap();
    let helper_path = root.join("hooks/copet-hook.sh");
    fs::write(&helper_path, "#!/bin/sh\nprintf '{}\\n'\n").unwrap();
    let hooks_path = home.join(".codex/hooks.json");
    let mut hooks = read_json(&hooks_path);
    hooks["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"] = 1.into();
    fs::write(&hooks_path, serde_json::to_vec_pretty(&hooks).unwrap()).unwrap();

    let summary = run_agent_auto_install_once(&store, &manager).unwrap();

    assert_eq!(summary.installed, vec!["codex".to_string()]);
    assert!(summary.failed.is_empty());
    let helper = fs::read_to_string(helper_path).unwrap();
    assert!(
        helper.contains("/v1/actions"),
        "legacy helper was not refreshed"
    );
    assert!(helper.contains("permission.waiting"));
}

#[test]
fn run_agent_auto_install_once_preserves_user_hook_order_and_trusted_hash_during_codex_migration() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".copet");
    let home = temp.path().join("home");
    let store = ConfigStore::new(&root);
    store.ensure_ready().unwrap();
    let manager = manager_with_fake_agent_names(&root, &home, &["codex"]);

    run_agent_auto_install_once(&store, &manager).unwrap();
    let hooks_path = home.join(".codex/hooks.json");
    let mut hooks = read_json(&hooks_path);
    hooks["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"] = 1.into();
    hooks["hooks"]["PermissionRequest"]
        .as_array_mut()
        .unwrap()
        .insert(
            0,
            serde_json::json!({
                "matcher": "*",
                "hooks": [{
                    "type": "command",
                    "command": "echo user-owned",
                    "timeout": 30
                }]
            }),
        );
    fs::write(&hooks_path, serde_json::to_vec_pretty(&hooks).unwrap()).unwrap();

    let config_path = home.join(".codex/config.toml");
    let config = fs::read_to_string(&config_path).unwrap();
    let hooks_abs = hooks_path.display().to_string();
    let user_key = format!("{hooks_abs}:permission_request:0:0");
    let parsed = config.parse::<toml::Value>().unwrap();
    let old_hash = parsed["hooks"]["state"][&user_key]["trusted_hash"]
        .as_str()
        .unwrap();
    let config = config.replace(
        &format!("trusted_hash = \"{old_hash}\""),
        "trusted_hash = \"sha256:user-owned\"",
    );
    fs::write(&config_path, config).unwrap();

    let summary = run_agent_auto_install_once(&store, &manager).unwrap();

    assert_eq!(summary.installed, vec!["codex".to_string()]);
    assert!(summary.failed.is_empty());
    let repaired = read_json(&hooks_path);
    let permission_groups = repaired["hooks"]["PermissionRequest"].as_array().unwrap();
    assert_eq!(
        permission_groups[0]["hooks"][0]["command"],
        "echo user-owned"
    );
    assert!(permission_groups[1]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .contains("copet-hook.sh"));
    assert_eq!(permission_groups[1]["hooks"][0]["timeout"], 600);

    let config = fs::read_to_string(config_path).unwrap();
    let parsed = config.parse::<toml::Value>().unwrap();
    assert_eq!(
        parsed["hooks"]["state"][&user_key]["trusted_hash"].as_str(),
        Some("sha256:user-owned")
    );
    let copet_key = format!("{hooks_abs}:permission_request:1:0");
    assert!(parsed["hooks"]["state"][&copet_key]["trusted_hash"]
        .as_str()
        .is_some_and(|hash| hash.starts_with("sha256:")));
}

#[test]
fn run_agent_auto_install_once_marks_complete_even_when_adapter_fails() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".copet");
    let home = temp.path().join("home");
    let store = ConfigStore::new(&root);
    store.ensure_ready().unwrap();
    let codex_hooks = home.join(".codex/hooks.json");
    fs::create_dir_all(codex_hooks.parent().unwrap()).unwrap();
    fs::write(&codex_hooks, "{not valid json").unwrap();
    let manager = manager_with_fake_agent_names(&root, &home, &["codex"]);

    let summary = run_agent_auto_install_once(&store, &manager).unwrap();

    assert!(summary.installed.is_empty());
    assert_eq!(summary.failed.len(), 1);
    assert_eq!(summary.failed[0].adapter_id, "codex");
    assert!(store.agent_auto_install_complete().unwrap());
}
