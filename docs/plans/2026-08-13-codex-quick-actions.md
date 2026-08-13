# Codex 快捷操作实施计划

> **For Codex:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**目标：** 让用户从 CoPet 提醒中后台继续无歧义的 Codex 任务，并一次性批准当前工具权限请求。

**架构：** 使用 Codex 原生 `Stop` 与 `PermissionRequest` Hook 建立本地双向动作协议。Rust runtime 负责动作注册、一次性决定、超时和通知状态；React 只通过类型化 Tauri 命令展示和解析动作，不直接调用 IPC。

**技术栈：** Rust、Tauri 2、React、TypeScript、Playwright、Codex Hooks、本机 HTTP loopback。

---

## 实施约束

- 开始编码前使用 `superpowers:executing-plans`。
- 每个任务使用 `superpowers:test-driven-development`，严格记录 FAIL 后再写最小实现。
- 不新建或切换仓库；继续使用当前 worktree 和 `feature/codex-completion-pet` 分支。
- Rust 生产代码不放内联测试，测试全部位于 `src-tauri/tests/`。
- 前端组件不直接调用 `invoke`，统一经 `src/lib/appCommands.ts` 和 `src/hooks/`。
- 每次提交前执行 `git diff --cached --name-status`，并对每个暂存路径运行 `git check-ignore -v --no-index -- <path>`。

### Task 1：建立快捷动作领域模型与安全策略

**文件：**

- 新建：`src-tauri/src/task_actions.rs`
- 修改：`src-tauri/src/lib.rs`
- 修改：`src-tauri/src/task_notifications.rs`
- 测试：`src-tauri/tests/task_actions.rs`
- 测试：`src-tauri/tests/task_notifications.rs`

**步骤 1：编写失败测试**

覆盖以下行为：

```rust
#[test]
fn continue_marker_creates_pending_continue_action() { /* 精确标记可用 */ }

#[test]
fn decision_or_input_request_never_gets_quick_continue() { /* A/B、输入、永久授权 */ }

#[test]
fn stop_hook_active_never_creates_second_continue_action() { /* 防循环 */ }

#[test]
fn permission_action_is_allow_once_only() { /* 不存在永久批准值 */ }

#[test]
fn restored_notification_does_not_restore_executable_action() { /* 重启后过期 */ }
```

动作类型采用可序列化的封闭枚举：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskActionKind { Continue, Permission }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskActionState { Pending, Resolving, Expired }

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
```

**步骤 2：运行测试确认 FAIL**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test task_actions --test task_notifications
```

预期：FAIL，提示 `task_actions` 模块和 `TaskNotification.action` 尚不存在。

**步骤 3：实现最小领域逻辑**

- 在 `TaskNotification` 增加 `#[serde(default)] pub action: Option<TaskAction>`，保证旧持久化文件兼容。
- 实现完全匹配的 `<!-- copet:continue -->` 检测。
- 实现保守排除规则：选项、缺失输入、永久授权和明显不可逆操作命中时不允许快捷动作。
- 提供展示专用的命令遮罩函数；原始值不进入持久化或日志。
- `prune_for_persistence` 保存提醒前把动作转换为 `Expired`，并关闭快捷按钮。

**步骤 4：运行测试确认 PASS**

运行同一步骤 2，预期所有新增测试 PASS。

**步骤 5：提交**

```bash
git add src-tauri/src/task_actions.rs src-tauri/src/lib.rs src-tauri/src/task_notifications.rs src-tauri/tests/task_actions.rs src-tauri/tests/task_notifications.rs
git diff --cached --name-status
git check-ignore -v --no-index -- src-tauri/src/task_actions.rs src-tauri/src/lib.rs src-tauri/src/task_notifications.rs src-tauri/tests/task_actions.rs src-tauri/tests/task_notifications.rs
git commit -m "feat(actions): model one-shot Codex actions"
```

### Task 2：实现 runtime 动作注册、等待与一次性解析

**文件：**

- 修改：`src-tauri/src/runtime_server.rs`
- 修改：`src-tauri/src/task_actions.rs`
- 测试：`src-tauri/tests/runtime_server_core.rs`
- 测试：`src-tauri/tests/runtime_http.rs`
- 测试：`src-tauri/tests/task_actions.rs`

**步骤 1：编写失败测试**

至少覆盖：

```rust
#[test]
fn authenticated_action_request_returns_unique_action_id() { /* 202 */ }

#[test]
fn allow_once_is_consumed_exactly_once() { /* 第二次读取为 stale */ }

#[test]
fn fallback_and_timeout_never_return_allow() { /* 安全默认 */ }

#[test]
fn concurrent_actions_resolve_by_action_id() { /* A 不影响 B */ }

#[test]
fn waiting_for_decision_does_not_block_normal_event_ingestion() { /* 并发 */ }
```

请求只接受原始 Hook JSON，不通过 shell 正则重建嵌套字段：

```json
{
  "agent": "codex",
  "kind": "permission.waiting",
  "hookInput": {
    "session_id": "thread-1",
    "turn_id": "turn-1",
    "run_id_suffix": "approval-1",
    "tool_name": "Bash",
    "tool_input": { "command": "pnpm test", "description": "运行测试" },
    "cwd": "/repo"
  }
}
```

**步骤 2：运行测试确认 FAIL**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test task_actions --test runtime_server_core --test runtime_http
```

预期：FAIL，`/v1/actions` 和决定读取路由返回 404。

**步骤 3：实现最小协议**

- 新增 `ActionRegistry`，内部使用 `Mutex<HashMap<...>> + Condvar` 保存短生命周期动作和一次性决定。
- 新增 `POST /v1/actions` 与 `GET /v1/actions/{id}/decision`。
- 所有路由复用现有 Bearer token、正文大小限制和 loopback 监听。
- listener 为每个连接创建短生命周期工作线程；长轮询不得持有 `RuntimeCore` 锁。
- 决定限定为 `continue_once`、`allow_once`、`fallback`；不得出现 session 或 persistent 值。
- runtime shutdown 时把全部 pending 动作解析为 fallback 并唤醒等待者。

响应形状固定为：

```json
{ "actionId": "随机标识", "expiresAtMs": 123456 }
```

```json
{ "state": "resolved", "decision": "allowOnce" }
```

**步骤 4：运行测试确认 PASS**

运行同一步骤 2，预期 PASS。

**步骤 5：提交**

```bash
git add src-tauri/src/runtime_server.rs src-tauri/src/task_actions.rs src-tauri/tests/runtime_server_core.rs src-tauri/tests/runtime_http.rs src-tauri/tests/task_actions.rs
git diff --cached --name-status
git check-ignore -v --no-index -- src-tauri/src/runtime_server.rs src-tauri/src/task_actions.rs src-tauri/tests/runtime_server_core.rs src-tauri/tests/runtime_http.rs src-tauri/tests/task_actions.rs
git commit -m "feat(actions): add bidirectional runtime protocol"
```

### Task 3：让 Codex Hook 生成动作并回传原生决定

**文件：**

- 修改：`src-tauri/src/agents/mod.rs`
- 修改：`src-tauri/src/agents/adapters/codex.rs`
- 测试：`src-tauri/tests/agent_support/codex.rs`
- 新建：`src-tauri/tests/fixtures/codex-permission-request.json`
- 新建：`src-tauri/tests/fixtures/codex-stop-continue.json`

**步骤 1：编写失败测试**

覆盖：

- `UserPromptSubmit` 输出包含快捷动作标记契约的 `additionalContext`。
- PermissionRequest 注册动作后，`allowOnce` 输出 Codex 原生 allow JSON。
- PermissionRequest fallback、runtime 不可用和超时均输出 `{}`。
- 带标记的 Stop 在 `continueOnce` 后输出 `decision:block`。
- `stop_hook_active=true` 时不注册第二个动作并输出 `{}`。
- 普通 Stop 仍上报完成事件，现有行为不回归。

**步骤 2：运行测试确认 FAIL**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test agent_support codex
```

预期：FAIL，helper 仍总是快速上报并输出 `{}`。

**步骤 3：实现最小 Hook 协议**

- 保留现有短生命周期事件上报路径。
- Codex `user.prompt` 返回受支持的 `hookSpecificOutput.additionalContext`，注入严格标记契约。
- Codex `permission.waiting` 将原始 stdin JSON 包装为 `hookInput` 提交 `/v1/actions`，等待决定后输出：

```json
{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}
```

- 带合法标记的 `session.stop` 走动作协议；`continueOnce` 输出：

```json
{"decision":"block","reason":"用户已在 CoPet 确认继续执行。请继续完成当前任务，不要再次询问是否继续。"}
```

- 其他情况输出 `{}`。
- 为 PermissionRequest 和 Stop 明确设置十分钟 Hook timeout；helper 在外层 timeout 前主动 fallback。
- 更新 trusted hash 测试，证明安装和修复会写入新的等价配置。

**步骤 4：运行测试确认 PASS**

运行同一步骤 2，预期 PASS。

**步骤 5：提交**

```bash
git add src-tauri/src/agents/mod.rs src-tauri/src/agents/adapters/codex.rs src-tauri/tests/agent_support/codex.rs src-tauri/tests/fixtures/codex-permission-request.json src-tauri/tests/fixtures/codex-stop-continue.json
git diff --cached --name-status
git check-ignore -v --no-index -- src-tauri/src/agents/mod.rs src-tauri/src/agents/adapters/codex.rs src-tauri/tests/agent_support/codex.rs src-tauri/tests/fixtures/codex-permission-request.json src-tauri/tests/fixtures/codex-stop-continue.json
git commit -m "feat(actions): bridge Codex hook decisions"
```

### Task 4：增加 Tauri 动作命令和通知状态流转

**文件：**

- 修改：`src-tauri/src/runtime_server.rs`
- 修改：`src-tauri/src/task_notifications.rs`
- 修改：`src-tauri/src/lib.rs`
- 测试：`src-tauri/tests/runtime_server_core.rs`
- 测试：`src-tauri/tests/task_notifications.rs`

**步骤 1：编写失败测试**

覆盖：

- `continue_once` 把目标提醒从 waiting 改为 running。
- `allow_once` 只解析当前 permission 动作。
- `fallback` 取消快捷动作但保留可打开的提醒。
- 旧动作、错误 kind、重复动作返回明确错误且不改其他通知。
- `clear_completed` 仍只清 completed，保留所有 waiting/failed。

**步骤 2：运行测试确认 FAIL**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test runtime_server_core --test task_notifications
```

预期：FAIL，`resolve_task_action` 尚不存在。

**步骤 3：实现最小命令**

新增类型化命令：

```rust
#[tauri::command]
fn resolve_task_action(
    app: tauri::AppHandle,
    id: String,
    decision: TaskActionDecision,
    runtime: tauri::State<'_, RuntimeManager>,
) -> Result<RuntimeUpdate, String>
```

- `continueOnce` 只接受 continue 动作。
- `allowOnce` 只接受 permission 动作且 `quickActionAllowed=true`。
- `fallback` 对两种动作都可用。
- 成功和失败都通过现有本地化错误路径反馈；成功后广播 `pet-state-changed`。
- `dismiss_task_notification` 遇到 pending 动作时先 fallback，再删除提醒，防止 Hook 悬挂。

**步骤 4：运行测试确认 PASS**

运行同一步骤 2，预期 PASS。

**步骤 5：提交**

```bash
git add src-tauri/src/runtime_server.rs src-tauri/src/task_notifications.rs src-tauri/src/lib.rs src-tauri/tests/runtime_server_core.rs src-tauri/tests/task_notifications.rs
git diff --cached --name-status
git check-ignore -v --no-index -- src-tauri/src/runtime_server.rs src-tauri/src/task_notifications.rs src-tauri/src/lib.rs src-tauri/tests/runtime_server_core.rs src-tauri/tests/task_notifications.rs
git commit -m "feat(actions): resolve task actions through Tauri"
```

### Task 5：接通前端类型、命令包装和测试 harness

**文件：**

- 修改：`src/lib/appTypes.ts`
- 修改：`src/lib/appCommands.ts`
- 修改：`src/hooks/useTaskNotifications.ts`
- 修改：`src/tests/app-harness.ts`
- 测试：`src/tests/codex-task-actions.spec.ts`

**步骤 1：编写失败测试**

```typescript
test("继续按钮按 action id 提交 continueOnce", async () => {});
test("权限按钮按 action id 提交 allowOnce", async () => {});
test("命令失败时保留提醒并显示错误", async () => {});
```

**步骤 2：运行测试确认 FAIL**

```bash
pnpm test:frontend src/tests/codex-task-actions.spec.ts
```

预期：FAIL，缺少动作类型和 `resolve_task_action` 调用。

**步骤 3：实现最小前端边界**

- 在 `TaskNotification` 上增加 `action: TaskAction | null`。
- 在 `appCommands.ts` 增加 `resolveTaskAction(id, decision)`，统一 patch runtime update。
- 在 `useTaskNotifications` 暴露 `continueOnce`、`allowOnce` 和 `fallbackAndOpen`。
- harness 精确模拟一次性解析、通知状态更新和命令错误，不在组件中伪造状态。

**步骤 4：运行测试确认 PASS**

运行同一步骤 2，预期 PASS。

**步骤 5：提交**

```bash
git add src/lib/appTypes.ts src/lib/appCommands.ts src/hooks/useTaskNotifications.ts src/tests/app-harness.ts src/tests/codex-task-actions.spec.ts
git diff --cached --name-status
git check-ignore -v --no-index -- src/lib/appTypes.ts src/lib/appCommands.ts src/hooks/useTaskNotifications.ts src/tests/app-harness.ts src/tests/codex-task-actions.spec.ts
git commit -m "feat(actions): expose typed task action commands"
```

### Task 6：实现提醒卡片动作界面

**文件：**

- 修改：`src/components/TaskNotifications.tsx`
- 修改：`src/PetWindow.tsx`
- 修改：`src/lib/i18n.ts`
- 修改：`src/styles.css`
- 测试：`src/tests/codex-task-actions.spec.ts`
- 测试：`src/tests/codex-task-notifications.spec.ts`

**步骤 1：补充失败测试**

覆盖：

- 普通继续展示摘要和“继续执行”。
- 权限请求展示命令、工作目录、“仅允许本次操作”和“允许并继续”。
- 动作按钮阻止卡片点击冒泡，不打开 Codex。
- “在 Codex 中处理”先 fallback 再打开对应任务。
- `quickActionAllowed=false`、expired、需要抉择时不显示批准按钮。
- 多张卡片的按钮绑定各自 `action.id`。

**步骤 2：运行测试确认 FAIL**

```bash
pnpm test:frontend src/tests/codex-task-actions.spec.ts src/tests/codex-task-notifications.spec.ts
```

预期：FAIL，当前卡片只有打开和关闭行为。

**步骤 3：实现最小界面**

- 卡片正文保留打开任务语义；内部按钮是独立的真实 `<button>`。
- 用 `aria-label` 和可见文案表达一次性批准，不依赖颜色传达风险。
- 内容过长时视觉折叠，但 DOM 中保留可访问名称和 title。
- 处理期间禁用当前按钮，其他任务仍可操作。
- 增加中英文文案，不增加基于 CSS 像素的测试。

**步骤 4：运行测试确认 PASS**

运行同一步骤 2，预期 PASS。

**步骤 5：提交**

```bash
git add src/components/TaskNotifications.tsx src/PetWindow.tsx src/lib/i18n.ts src/styles.css src/tests/codex-task-actions.spec.ts src/tests/codex-task-notifications.spec.ts
git diff --cached --name-status
git check-ignore -v --no-index -- src/components/TaskNotifications.tsx src/PetWindow.tsx src/lib/i18n.ts src/styles.css src/tests/codex-task-actions.spec.ts src/tests/codex-task-notifications.spec.ts
git commit -m "feat(actions): add inline Codex action controls"
```

### Task 7：回归、文档与现场验收

**文件：**

- 修改：`README.md`
- 修改：`README.zh.md`
- 修改：`docs/architecture.md`
- 修改：`docs/architecture.zh.md`
- 修改：必要的既有测试夹具

**步骤 1：更新产品和架构文档**

说明：

- 普通继续与一次性权限的差异。
- 不支持需要选择或输入的场景。
- 十分钟超时和安全 fallback。
- Hook 是双向阻塞边界，普通事件仍保持快速失败和静默降级。

**步骤 2：运行聚焦回归**

```bash
pnpm test:frontend src/tests/codex-task-actions.spec.ts src/tests/codex-task-notifications.spec.ts src/tests/static-pet-codex-focus.spec.ts
cargo test --manifest-path src-tauri/Cargo.toml --test task_actions --test task_notifications --test runtime_server_core --test runtime_http --test agent_support codex
```

预期：全部 PASS。

**步骤 3：执行完整验证**

使用 `superpowers:verification-before-completion`，依次运行：

```bash
pnpm test:frontend
pnpm test:rust
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

预期：全部退出码为 0。

**步骤 4：请求代码审查**

使用 `superpowers:requesting-code-review`，重点审查：

- 是否存在跨动作批准、重放或竞态。
- timeout、关闭和 runtime shutdown 是否全部 fail-safe。
- Hook 输出是否与 Codex 当前协议一致。
- 日志和持久化是否泄漏命令或敏感数据。
- existing completed 清理、waiting/failed 保留是否回归。

修复所有高、中优先级问题后重新运行完整验证。

**步骤 5：构建、安装并现场验收**

- 构建 release 应用并替换 `/Applications/CoPet.app`。
- 重启 CoPet 和用于验收的 Codex 会话，使新 Hook 生效。
- 现场验证普通继续：弹窗出现，点击后 Codex 不前台激活但原任务继续。
- 现场验证工具权限：只批准当前命令；下一条权限请求再次弹出。
- 现场验证抉择问题：A/B 或输入请求没有快捷继续按钮。
- 现场验证超时/关闭：均不批准，能够回到 Codex 原生处理。
- 现场验证前台清理：completed 清除，waiting/failed 保留。
- 保存必要截图和脱敏状态证据。

**步骤 6：提交文档和验收调整**

```bash
git add README.md README.zh.md docs/architecture.md docs/architecture.zh.md
git diff --cached --name-status
git check-ignore -v --no-index -- README.md README.zh.md docs/architecture.md docs/architecture.zh.md
git commit -m "docs(actions): document Codex quick actions"
```

**步骤 7：收尾**

使用 `superpowers:finishing-a-development-branch`：

- 确认工作树干净。
- 推送现有 `feature/codex-completion-pet` 分支。
- 更新现有 PR #1，不创建重复 PR。
- 汇报完整测试、构建、安装、现场验收和代码审查证据。

## 阶段 0 检查点

**输入：** 已确认的产品范围、现有 Codex Hook 集成、现有任务提醒和 macOS 前台清理逻辑。

**输出：** 双向 Hook 设计、七个 TDD 实施任务、完整测试与现场验收步骤。

**依赖：** 当前 Codex 版本支持 `PermissionRequest` allow 决定、`Stop` block 决定和 `UserPromptSubmit` additional context。

**核心测试用例：**

1. 无需抉择的阶段结果可在 CoPet 点击后继续原任务。
2. 工具权限只批准当前一次，重复点击和下一条请求不会被连带批准。
3. A/B、参数输入、永久授权和高风险操作不显示快捷批准。
4. 超时、退出、断连和旧动作全部安全 fallback。
5. 多任务并行严格按 `actionId` 隔离。
6. completed 前台清除与 waiting/failed 保留行为无回归。
