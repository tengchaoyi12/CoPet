# Codex 任务提醒

CoPet 通过本机 Codex Hook 感知任务状态。首次启动时，如果本机能找到 Codex CLI，CoPet 会自动安装所需 Hook；设置页的 Agent 分区首版只显示 Codex。

## 提醒行为

- 提交任务后，宠物进入工作状态。
- 任务需要权限或用户输入时，显示“任务需要你处理，快去看看吧。”。
- 任务完成时，显示“任务完成啦，快去看看吧。”，并为该任务播放一次庆祝效果。
- 多个并行任务分别显示，使用 Codex 的 `session_id` 和 `turn_id` 区分。
- 点击提醒会优先打开 `codex://threads/{session_id}`；无法打开深链时回退到启动 Codex 应用。
- 点击成功后提醒变为已读；关闭按钮只关闭当前提醒。
- 未处理提醒在 CoPet 重启后恢复，但不会再次播放声音或庆祝动画。

本地状态只保存截断后的标题、摘要、状态和任务标识，不保存完整提示词、完整回答、工作目录或访问令牌。提醒文件位于 `~/.copet/runtime/task-notifications.json`，最多保留 100 条和 30 天。

当前 Codex Hook 没有提供可可靠区分的失败或中断事件，因此首版不承诺失败提醒；在上游提供明确事件前，Stop 事件按任务结束处理。

## 启动与开发

正式安装后，从 macOS“应用程序”目录启动 CoPet。在“设置 → 通用”开启“登录后自动启动”后，关机或退出登录不需要手动处理；下次登录时 macOS 会通过 LaunchAgent 启动 CoPet。

开发时使用：

```bash
pnpm install
pnpm tauri dev
```

React 样式、组件和文案改动会热更新；Rust 代码或 Cargo 依赖改动会触发重新编译。生成正式安装包时运行：

```bash
pnpm tauri build
```

## 验收步骤

1. 启动 CoPet，打开设置的 Agent 页面，确认只显示 Codex 且状态健康。
2. 在 Codex 提交一个简单任务，确认宠物进入工作状态。
3. 等任务结束，确认出现“任务完成啦，快去看看吧。”。
4. 重放同一 Stop Hook，确认不再次发声。
5. 点击气泡，确认打开对应 Codex 任务。
6. 关闭或点击提醒后重启 CoPet，确认已处理提醒不恢复。
7. 保留一条未处理提醒后重启，确认提醒恢复但没有庆祝音效。
8. 开启“登录后自动启动”，重新进入设置确认开关仍反映系统真实状态。

测试用脱敏 Hook 载荷位于：

- `src-tauri/tests/fixtures/codex-user-prompt-submit.json`
- `src-tauri/tests/fixtures/codex-stop.json`

## 排障

- Codex 页面显示未安装：确认 `codex` 可在终端运行，然后在设置页重新开启 Codex。
- 没有任务提醒：检查 `~/.copet/runtime/event-endpoint` 和 `event-token` 是否在 CoPet 运行时存在。
- 点击只能打开应用首页：当前 Codex 版本可能未注册任务深链，CoPet 会自动使用应用启动作为回退。
- 开机后未启动：进入“设置 → 通用”关闭再开启“登录后自动启动”，让系统重新登记 LaunchAgent。
