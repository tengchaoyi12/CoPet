# Codex 任务完成提醒宠物 Implementation Plan

> **For Codex:** REQUIRED SUB-SKILL: 使用 `superpowers:executing-plans` 逐项执行本计划；每项功能必须先使用 `superpowers:test-driven-development` 完成失败测试，再写实现。

**Goal（目标）：** 让 CoPet 在每个 Codex 任务等待、失败或完成时展示独立提醒；完成时只庆祝一次，点击提醒可返回对应 Codex 任务，并支持 macOS 登录后自动启动。

**Architecture（架构）：** 沿用现有 Codex Hook → 本地令牌端点 → Rust 运行时 → Tauri 事件 → React 宠物窗口链路。Rust 新增任务通知领域模型、去重与本地持久化，React 只展示任务消息并通过 Hook 调用 Tauri 命令；任务跳转优先使用 `codex://threads/{session_id}`，失败时激活 `com.openai.codex`。

**Tech Stack（技术栈）：** Rust 2021、Tauri 2、`tauri-plugin-autostart`、React 18、TypeScript、Playwright、pnpm、macOS LaunchAgent。

---

## 执行前置条件

当前独立工作树：

```text
/Users/beizhi/Documents/Codex/2026-08-12/code/work/CoPet/.worktrees/codex-completion-pet
```

当前基线检查有两个环境缺口，开始功能实现前必须处理：

1. 全局规范要求加载 `DEV_FLOW.md`，但工作区和仓库内均未找到该文件。执行者必须先获得并完整读取它；找不到时停止代码实现。
2. 当前终端没有 `cargo`，Playwright Chromium 也尚未下载。安装 Rust 1.77.2 或更高版本和 Chromium 后，重新运行基线。

环境准备命令：

```bash
pnpm install --frozen-lockfile
pnpm exec playwright install chromium
cargo --version
pnpm test:frontend
pnpm test:rust
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

预期：165 项前端测试全部通过，Rust 测试、构建和格式检查通过。若仍有基线失败，先报告并取得用户同意，不能把既有失败混入本功能。

参考资料：

- OpenAI Codex 的 `UserPromptSubmit` 输入含 `session_id`、`turn_id` 和 `prompt`：<https://github.com/openai/codex/blob/main/codex-rs/hooks/src/events/user_prompt_submit.rs>
- OpenAI Codex 的 `Stop` 输入含 `session_id`、`turn_id` 和 `last_assistant_message`：<https://github.com/openai/codex/blob/main/codex-rs/hooks/src/events/stop.rs>
- Tauri 官方自启动插件：<https://v2.tauri.app/plugin/autostart/>

## 提交前固定检查

每个任务的提交步骤都必须先执行以下检查；任一暂存路径被忽略时立即停止：

```bash
git diff --cached --name-status
git check-ignore -v --no-index -- <每一个暂存路径>
```

只有 `git check-ignore` 对所有路径均返回未忽略，才可以执行 `git commit`。

### Task 1：让 Codex Hook 传递真实任务标识和摘要

**Files：**

- Modify: `src-tauri/src/runtime_state.rs`
- Modify: `src-tauri/src/agents/mod.rs`
- Test: `src-tauri/tests/agent_support/codex.rs`
- Test: `src-tauri/tests/runtime_server_core.rs`

**Step 1：为 Hook 助手写失败测试**

在 `src-tauri/tests/agent_support/codex.rs` 新增测试，启动已有本地监听器，向助手输入真实 Codex Hook 形状：

```rust
#[test]
fn codex_helper_forwards_task_identity_and_safe_text() {
    let input = r#"{
      "session_id":"thread-123",
      "turn_id":"turn-456",
      "prompt":"修复登录按钮",
      "last_assistant_message":"已完成登录按钮修复"
    }"#;

    // 沿用 codex_helper_bypasses_loopback_proxy_when_posting_runtime_events
    // 的监听器和进程启动方式，分别验证 UserPromptSubmit 与 Stop。
    assert!(request.contains(r#""sessionId":"thread-123""#));
    assert!(request.contains(r#""turnId":"turn-456""#));
    assert!(request.contains(r#""taskTitle":"修复登录按钮""#));
    assert!(request.contains(r#""summary":"已完成登录按钮修复""#));
}
```

再在 `src-tauri/tests/runtime_server_core.rs` 增加 HTTP 反序列化测试，确认 snake_case 与 camelCase 都能进入同一 `RuntimeEvent` 字段。

**Step 2：运行测试并确认失败**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test agent_support codex::codex_helper_forwards_task_identity_and_safe_text -- --exact
cargo test --manifest-path src-tauri/Cargo.toml --test runtime_server_core runtime_event_accepts_codex_task_fields -- --exact
```

Expected：FAIL，原因是助手尚未发送 `sessionId`、`turnId`、`taskTitle` 和 `summary`，Rust 结构也没有对应字段。

**Step 3：扩展运行时事件结构**

在 `RuntimeEvent` 中加入可选字段，并兼容 Codex 的 snake_case：

```rust
#[serde(default, alias = "turn_id")]
pub turn_id: Option<String>,
#[serde(default, alias = "task_title")]
pub task_title: Option<String>,
#[serde(default)]
pub summary: Option<String>,
```

添加统一的安全文本函数：压缩空白、按字符截断标题到 80 个字符、摘要到 240 个字符；不记录完整提示和完整最终回答。

**Step 4：扩展共享 Hook 助手**

在 `helper_script()` 中提取并转发：

```sh
session_id="$(json_string_field session_id)"
turn_id="$(json_string_field turn_id)"
prompt="$(json_string_field prompt)"
last_assistant_message="$(json_string_field last_assistant_message)"
```

只给 `UserPromptSubmit` 写 `taskTitle`，只给 `Stop` 写 `summary`；所有值先经过现有 `json_escape`。维持 `curl --max-time 0.8 ... || true`，CoPet 不可用时仍输出 `{}` 并以 0 退出。

**Step 5：运行测试并确认通过**

Run：重复 Step 2 的两条命令。

Expected：PASS。

**Step 6：提交**

```bash
git add src-tauri/src/runtime_state.rs src-tauri/src/agents/mod.rs src-tauri/tests/agent_support/codex.rs src-tauri/tests/runtime_server_core.rs
git diff --cached --name-status
git check-ignore -v --no-index -- src-tauri/src/runtime_state.rs
git check-ignore -v --no-index -- src-tauri/src/agents/mod.rs
git check-ignore -v --no-index -- src-tauri/tests/agent_support/codex.rs
git check-ignore -v --no-index -- src-tauri/tests/runtime_server_core.rs
git commit -m "feat: capture Codex task metadata"
```

### Task 2：建立任务通知状态机和优先级

**Files：**

- Create: `src-tauri/src/task_notifications.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/task_notifications.rs`
- Test: `src-tauri/tests/repo_layout.rs`

**Step 1：写任务状态机失败测试**

覆盖以下行为：

```rust
#[test]
fn completed_turn_is_notified_once() {
    let mut store = TaskNotificationStore::default();
    let first = store.apply(completed_event("thread-1", "turn-1"), 100);
    let duplicate = store.apply(completed_event("thread-1", "turn-1"), 200);

    assert_eq!(first.attention.unwrap().kind, AttentionKind::Completed);
    assert!(duplicate.attention.is_none());
    assert_eq!(store.visible().len(), 1);
}

#[test]
fn priority_is_waiting_failed_completed_running_idle() {
    let mut store = TaskNotificationStore::default();
    store.apply(running_event("thread-a", "turn-a"), 100);
    store.apply(completed_event("thread-b", "turn-b"), 110);
    store.apply(failed_event("thread-c", "turn-c"), 120);
    store.apply(waiting_event("thread-d", "turn-d"), 130);
    assert_eq!(store.dominant_status(), Some(TaskStatus::Waiting));
}
```

另测两个并行任务产生两个不同通知、点击已读后降级到下一优先级、缺失 `turn_id` 时使用 `agent + session_id` 的稳定回退键。

**Step 2：运行测试并确认失败**

Run：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test task_notifications
```

Expected：FAIL，模块和类型尚不存在。

**Step 3：实现最小领域模型**

核心类型保持可序列化、无 Tauri 依赖：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    Running,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskAttention {
    pub id: String,
    pub kind: AttentionKind,
    pub occurred_at_ms: u64,
}
```

`TaskNotificationStore::apply` 只在状态首次进入 `Waiting`、`Completed` 或 `Failed` 时生成 `TaskAttention`。同一任务的重复事件可以刷新摘要，但不能再次生成注意信号。

**Step 4：实现主状态映射**

```rust
pub fn dominant_pet_state(&self) -> Option<PetStateId> {
    match self.dominant_status()? {
        TaskStatus::Waiting => Some(PetStateId::Waiting),
        TaskStatus::Failed => Some(PetStateId::Failed),
        TaskStatus::Completed => Some(PetStateId::Waving),
        TaskStatus::Running => Some(PetStateId::Running),
    }
}
```

排序键固定为状态优先级降序、`updated_at_ms` 降序、`id` 升序，避免 HashMap 遍历造成界面抖动。

**Step 5：运行测试并确认通过**

Run：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test task_notifications
cargo test --manifest-path src-tauri/Cargo.toml --test repo_layout
```

Expected：PASS，并确认新模块测试放在 `src-tauri/tests`，没有写内联 `#[cfg(test)]`。

**Step 6：提交**

```bash
git add src-tauri/src/task_notifications.rs src-tauri/src/lib.rs src-tauri/tests/task_notifications.rs src-tauri/tests/repo_layout.rs
git diff --cached --name-status
git check-ignore -v --no-index -- src-tauri/src/task_notifications.rs
git check-ignore -v --no-index -- src-tauri/src/lib.rs
git check-ignore -v --no-index -- src-tauri/tests/task_notifications.rs
git check-ignore -v --no-index -- src-tauri/tests/repo_layout.rs
git commit -m "feat: add task notification state machine"
```

### Task 3：持久化未读提醒并在重启时静默恢复

**Files：**

- Modify: `src-tauri/src/task_notifications.rs`
- Modify: `src-tauri/src/config_store.rs`
- Test: `src-tauri/tests/task_notifications.rs`
- Test: `src-tauri/tests/config_store.rs`

**Step 1：写持久化失败测试**

```rust
#[test]
fn reload_restores_unread_without_replaying_attention() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("task-notifications.json");
    let mut store = TaskNotificationStore::load(&path).unwrap();
    assert!(store.apply(completed_event("thread-1", "turn-1"), 100).attention.is_some());
    store.save(&path).unwrap();

    let restored = TaskNotificationStore::load(&path).unwrap();
    assert_eq!(restored.visible().len(), 1);
    assert!(restored.startup_attention().is_none());
}
```

再覆盖损坏 JSON 从空状态启动、已读和已关闭记录不恢复、最多保留 100 条、30 天以上记录清理。

**Step 2：运行测试并确认失败**

Run：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test task_notifications reload_restores_unread_without_replaying_attention -- --exact
```

Expected：FAIL，`load`、`save` 尚不存在。

**Step 3：实现原子保存和容错加载**

状态文件固定为 `ConfigStore::runtime_dir()/task-notifications.json`。写入流程：序列化到同目录 `.tmp` 文件、`sync_all`、`rename`；读取失败记录本地诊断日志并返回空仓库，不让应用启动失败。

持久化内容只含截断后的标题、摘要、状态和标识，不含完整 prompt、完整转录路径或访问令牌。

**Step 4：运行测试并确认通过**

Run：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test task_notifications
cargo test --manifest-path src-tauri/Cargo.toml --test config_store
```

Expected：PASS。

**Step 5：提交**

```bash
git add src-tauri/src/task_notifications.rs src-tauri/src/config_store.rs src-tauri/tests/task_notifications.rs src-tauri/tests/config_store.rs
git diff --cached --name-status
git check-ignore -v --no-index -- src-tauri/src/task_notifications.rs
git check-ignore -v --no-index -- src-tauri/src/config_store.rs
git check-ignore -v --no-index -- src-tauri/tests/task_notifications.rs
git check-ignore -v --no-index -- src-tauri/tests/config_store.rs
git commit -m "feat: persist unread task notifications"
```

### Task 4：把任务通知接入本地运行时和安全端点

**Files：**

- Modify: `src-tauri/src/runtime_server.rs`
- Modify: `src-tauri/src/runtime_state.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/runtime_server_core.rs`
- Test: `src-tauri/tests/runtime_http.rs`

**Step 1：写运行时集成失败测试**

在 `runtime_server_core.rs` 增加：

- 同一 `session_id + turn_id` 的 `Stop` 发两次，只出现一个通知和一个 `TaskAttention`。
- 两个 Codex turn 并行结束后保留两条消息。
- `permission.waiting` 覆盖完成状态，处理后恢复完成状态。
- 未知 `kind` 返回 400，不进入 accepted 计数。
- 超长标题或摘要被服务端再次截断。

预期返回结构：

```rust
assert_eq!(status.notifications.len(), 2);
assert_eq!(status.current_state.state, PetStateId::Waiting);
assert_eq!(status.attention, None); // snapshot 不重放注意信号
```

**Step 2：运行测试并确认失败**

Run：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test runtime_server_core codex_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test runtime_http
```

Expected：FAIL，当前消息仍按 `agent` 覆盖，未知事件也不会得到新的校验结果。

**Step 3：接入 `TaskNotificationStore`**

将 `RuntimeCore` 的任务相关状态改为：

```rust
pub struct RuntimeCore {
    token: String,
    engine: EventStateEngine,
    task_notifications: TaskNotificationStore,
    latest_attention: Option<TaskAttention>,
    // 保留队列、限流、日志与统计字段
}
```

`RuntimeStatus` 和 `RuntimeUpdate` 增加：

```rust
pub notifications: Vec<TaskNotification>,
pub attention: Option<TaskAttention>,
```

兼容期保留现有 `messages` 字段供非 Codex 适配器使用；Codex 任务气泡读取 `notifications`。`RuntimeManager::snapshot()` 的 `attention` 永远为 `None`，只有事件推送携带本次新注意信号，因此重启不会重新播放声音。

**Step 4：加强事件白名单**

在认证和限流通过后，标准化事件；若 `canonical_event_kind` 不认识且原值不在允许集合，返回新的 `RuntimeServerError::UnsupportedEvent`，HTTP 映射为 400。保留现有 16 KiB 请求体限制、回环地址和 Bearer Token。

**Step 5：运行测试并确认通过**

Run：重复 Step 2，并运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test event_state_engine
```

Expected：PASS，旧的宠物状态测试无回归。

**Step 6：提交**

```bash
git add src-tauri/src/runtime_server.rs src-tauri/src/runtime_state.rs src-tauri/src/lib.rs src-tauri/tests/runtime_server_core.rs src-tauri/tests/runtime_http.rs
git diff --cached --name-status
git check-ignore -v --no-index -- src-tauri/src/runtime_server.rs
git check-ignore -v --no-index -- src-tauri/src/runtime_state.rs
git check-ignore -v --no-index -- src-tauri/src/lib.rs
git check-ignore -v --no-index -- src-tauri/tests/runtime_server_core.rs
git check-ignore -v --no-index -- src-tauri/tests/runtime_http.rs
git commit -m "feat: integrate Codex task notifications"
```

### Task 5：实现任务打开、已读和关闭命令

**Files：**

- Create: `src-tauri/src/task_opener.rs`
- Modify: `src-tauri/src/runtime_server.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/task_opener.rs`
- Test: `src-tauri/tests/runtime_server_core.rs`

**Step 1：写打开策略失败测试**

```rust
#[test]
fn valid_codex_session_prefers_thread_deep_link() {
    let plan = codex_open_plan(Some("019ff53e-539b-7053-af7c-01b608aa7059"));
    assert_eq!(plan.primary, vec!["open", "codex://threads/019ff53e-539b-7053-af7c-01b608aa7059"]);
    assert_eq!(plan.fallback, vec!["open", "-b", "com.openai.codex"]);
}
```

再测非法 session 字符不进入 URL、主打开失败后执行回退、两者均失败时不标已读。

**Step 2：运行测试并确认失败**

Run：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test task_opener
```

Expected：FAIL，模块尚不存在。

**Step 3：实现可测试的打开计划**

`task_opener.rs` 不通过 shell 拼字符串，使用 `std::process::Command` 的独立参数：

```rust
pub fn codex_thread_url(session_id: &str) -> Option<String> {
    session_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        .then(|| format!("codex://threads/{session_id}"))
}
```

macOS 首选 `open <url>`，失败后执行 `open -b com.openai.codex`。其他平台返回明确的“不支持精确跳转”错误，不能静默删除提醒。

**Step 4：增加 Tauri 命令**

新增命令：

```rust
#[tauri::command]
fn open_task_notification(id: String, runtime: tauri::State<'_, RuntimeManager>) -> Result<(), String>;

#[tauri::command]
fn dismiss_task_notification(id: String, runtime: tauri::State<'_, RuntimeManager>) -> Result<RuntimeUpdate, String>;
```

打开成功后标记已读并原子保存；打开失败则保留未读。关闭命令显式删除或标记关闭并保存。两者都以通知 `id` 操作，不能再以 `agent` 操作。

**Step 5：运行测试并确认通过**

Run：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test task_opener
cargo test --manifest-path src-tauri/Cargo.toml --test runtime_server_core dismiss_task_notification -- --nocapture
```

Expected：PASS。

**Step 6：提交**

```bash
git add src-tauri/src/task_opener.rs src-tauri/src/runtime_server.rs src-tauri/src/lib.rs src-tauri/tests/task_opener.rs src-tauri/tests/runtime_server_core.rs
git diff --cached --name-status
git check-ignore -v --no-index -- src-tauri/src/task_opener.rs
git check-ignore -v --no-index -- src-tauri/src/runtime_server.rs
git check-ignore -v --no-index -- src-tauri/src/lib.rs
git check-ignore -v --no-index -- src-tauri/tests/task_opener.rs
git check-ignore -v --no-index -- src-tauri/tests/runtime_server_core.rs
git commit -m "feat: open Codex tasks from notifications"
```

### Task 6：展示可点击任务气泡并只庆祝一次

**Files：**

- Create: `src/components/TaskNotifications.tsx`
- Create: `src/hooks/useTaskNotifications.ts`
- Modify: `src/PetWindow.tsx`
- Modify: `src/hooks/useAppStore.ts`
- Modify: `src/hooks/useEmotionState.ts`
- Modify: `src/lib/appCommands.ts`
- Modify: `src/lib/appStore.ts`
- Modify: `src/lib/appTypes.ts`
- Modify: `src/lib/i18n.ts`
- Modify: `src/styles.css`
- Modify: `src/tests/app-harness.ts`
- Create: `src/tests/codex-task-notifications.spec.ts`

**Step 1：写前端失败测试**

Playwright 用例覆盖：

```ts
test("Codex 完成提醒可点击并按通知 id 关闭", async ({ browser }) => {
  const harness = await createAppHarness(browser, {
    runtimeStatus: runtimeWithCompletedTask({
      id: "codex:thread-1:turn-1",
      title: "修复登录按钮",
    }),
  });
  const page = await harness.openPage("pet");

  await expect(page.getByText("任务完成啦，快去看看吧。")).toBeVisible();
  await expect(page.getByText("修复登录按钮")).toBeVisible();
  await page.getByTestId("task-notification").click();
  expect(harness.calls).toContainEqual({
    command: "open_task_notification",
    args: { id: "codex:thread-1:turn-1" },
  });
});
```

另测：等待文案、失败文案、两个并行提醒、点击关闭调用 `dismiss_task_notification`、相同 `attention.id` 不重复播放、不同完成 `attention.id` 即使宠物仍是 `waving` 也会再次庆祝。

**Step 2：运行测试并确认失败**

Run：

```bash
pnpm test:frontend -- src/tests/codex-task-notifications.spec.ts
```

Expected：FAIL，新类型、组件和命令尚不存在。

**Step 3：同步前端类型和仓库状态**

`appTypes.ts` 镜像 Rust 类型：

```ts
export type TaskStatus = "running" | "waiting" | "completed" | "failed";

export type TaskNotification = {
  id: string;
  agent: string;
  displayName: string;
  sessionId: string | null;
  turnId: string | null;
  status: TaskStatus;
  title: string | null;
  summary: string | null;
  unread: boolean;
  updatedAtMs: number;
};
```

`RuntimeStatus` 与 `RuntimeUpdate` 增加 `notifications` 和 `attention`。`useBootstrapAppStore` 载入 snapshot 时忽略 attention；只有 `pet-state-changed` 事件更新 attention。

**Step 4：用 Hook 封装任务操作**

`appCommands.ts` 中集中调用 `invoke`；组件只使用：

```ts
export function useTaskNotifications() {
  const notifications = useAppSlice((state) => state.taskNotifications);
  return {
    notifications,
    open: (id: string) => openTaskNotification(id),
    dismiss: (id: string) => dismissTaskNotification(id),
  };
}
```

命令成功后以服务端返回的最新通知列表更新 store；失败时 toast 提示且保留气泡。

**Step 5：实现任务气泡和一次性注意信号**

`TaskNotifications.tsx` 使用 `notification.id` 作为 key，完成状态显示 `任务完成啦，快去看看吧。`，标题另起一行并限制宽度。消息主体可点击，关闭按钮 `stopPropagation()`。

将 `attention.id` 传给宠物情绪层。`useEmotionState` 的完成效果依赖注意信号标识，而不是只依赖 `agent.kind`：

```ts
if (attention?.kind === "completed" && previousAttentionId.current !== attention.id) {
  previousAttentionId.current = attention.id;
  setState({ kind: "sparkle" });
  playAgentSound("celebrating");
}
```

初次 bootstrap 的 attention 为 `null`，所以恢复未读消息不会重放声音。处理好现有 `PetWindow` 状态音效，避免同一完成事件被旧的 `waving` 逻辑和新 attention 逻辑播放两次。

**Step 6：运行新旧前端测试**

Run：

```bash
pnpm test:frontend -- src/tests/codex-task-notifications.spec.ts
pnpm test:frontend -- src/tests/pet-animation-layers.spec.ts src/tests/pet-sounds.spec.ts src/tests/pet-window-sync.spec.ts
```

Expected：PASS。

**Step 7：提交**

```bash
git add src/components/TaskNotifications.tsx src/hooks/useTaskNotifications.ts src/PetWindow.tsx src/hooks/useAppStore.ts src/hooks/useEmotionState.ts src/lib/appCommands.ts src/lib/appStore.ts src/lib/appTypes.ts src/lib/i18n.ts src/styles.css src/tests/app-harness.ts src/tests/codex-task-notifications.spec.ts
git diff --cached --name-status
git check-ignore -v --no-index -- src/components/TaskNotifications.tsx
git check-ignore -v --no-index -- src/hooks/useTaskNotifications.ts
git check-ignore -v --no-index -- src/PetWindow.tsx
git check-ignore -v --no-index -- src/hooks/useAppStore.ts
git check-ignore -v --no-index -- src/hooks/useEmotionState.ts
git check-ignore -v --no-index -- src/lib/appCommands.ts
git check-ignore -v --no-index -- src/lib/appStore.ts
git check-ignore -v --no-index -- src/lib/appTypes.ts
git check-ignore -v --no-index -- src/lib/i18n.ts
git check-ignore -v --no-index -- src/styles.css
git check-ignore -v --no-index -- src/tests/app-harness.ts
git check-ignore -v --no-index -- src/tests/codex-task-notifications.spec.ts
git commit -m "feat: show actionable Codex task reminders"
```

### Task 7：增加 macOS 登录自启动开关

**Files：**

- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/src/lib.rs`
- Create: `src/hooks/useAutostart.ts`
- Modify: `src/lib/appCommands.ts`
- Modify: `src/components/SettingsPreferencesSection.tsx`
- Modify: `src/SettingsWindow.tsx`
- Modify: `src/lib/i18n.ts`
- Modify: `src/tests/app-harness.ts`
- Create: `src/tests/settings-autostart.spec.ts`

**Step 1：写设置页失败测试**

```ts
test("登录自启动开关读取并更新系统真实状态", async ({ browser }) => {
  const harness = await createAppHarness(browser, {
    commandResults: { get_autostart_enabled: false },
  });
  const page = await harness.openPage("settings");
  await page.getByRole("tab", { name: "通用" }).click();
  const toggle = page.getByRole("switch", { name: "登录后自动启动" });
  await toggle.click();
  expect(harness.calls).toContainEqual({
    command: "set_autostart_enabled",
    args: { enabled: true },
  });
});
```

另测命令失败时开关恢复原状态并显示 toast。

**Step 2：运行测试并确认失败**

Run：

```bash
pnpm test:frontend -- src/tests/settings-autostart.spec.ts
```

Expected：FAIL，自启动 Hook 和控件尚不存在。

**Step 3：添加并初始化官方插件**

在 Rust 依赖中加入与 Tauri 2 兼容的 `tauri-plugin-autostart`，在 `setup` 中以 `MacosLauncher::LaunchAgent` 初始化。不要在首次启动时自动启用。

新增 Rust 命令：

```rust
#[tauri::command]
fn get_autostart_enabled(app: tauri::AppHandle) -> Result<bool, String>;

#[tauri::command]
fn set_autostart_enabled(app: tauri::AppHandle, enabled: bool) -> Result<bool, String>;
```

命令使用 `ManagerExt::autolaunch()` 查询系统实际状态；设置后再次查询并返回，不把配置文件布尔值当成真实状态。

**Step 4：实现前端 Hook 和设置项**

`useAutostart` 在“通用”页激活时懒加载，维护 `loading` 与 `pending`，并通过 `appCommands.ts` 调用 Tauri。设置页只接收 Hook 返回的数据和回调，不直接 `invoke`。

**Step 5：运行测试和构建**

Run：

```bash
pnpm test:frontend -- src/tests/settings-autostart.spec.ts src/tests/settings-shell.spec.ts
cargo test --manifest-path src-tauri/Cargo.toml
pnpm build
```

Expected：PASS。

**Step 6：提交**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs src/hooks/useAutostart.ts src/lib/appCommands.ts src/components/SettingsPreferencesSection.tsx src/SettingsWindow.tsx src/lib/i18n.ts src/tests/app-harness.ts src/tests/settings-autostart.spec.ts
git diff --cached --name-status
git check-ignore -v --no-index -- src-tauri/Cargo.toml
git check-ignore -v --no-index -- src-tauri/Cargo.lock
git check-ignore -v --no-index -- src-tauri/src/lib.rs
git check-ignore -v --no-index -- src/hooks/useAutostart.ts
git check-ignore -v --no-index -- src/lib/appCommands.ts
git check-ignore -v --no-index -- src/components/SettingsPreferencesSection.tsx
git check-ignore -v --no-index -- src/SettingsWindow.tsx
git check-ignore -v --no-index -- src/lib/i18n.ts
git check-ignore -v --no-index -- src/tests/app-harness.ts
git check-ignore -v --no-index -- src/tests/settings-autostart.spec.ts
git commit -m "feat: add login autostart setting"
```

### Task 8：首版只暴露并自动安装 Codex 适配器

**Files：**

- Modify: `src-tauri/src/agents/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/components/SettingsAgentsSection.tsx`
- Test: `src-tauri/tests/agent_support/manager.rs`
- Test: `src/tests/settings-workflows.spec.ts`

**Step 1：写范围限制失败测试**

Rust 测试验证“应用首次自动安装”只调用可用的 Codex，不改动其他 CLI 配置；现有 `AgentManager::auto_install_detected_agents` 的通用能力可以保留。前端测试验证 Agents 页只显示 Codex。

```rust
assert_eq!(summary.installed, vec!["codex"]);
assert!(!home.join(".claude/settings.json").exists());
assert!(!home.join(".gemini/settings.json").exists());
```

**Step 2：运行测试并确认失败**

Run：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test agent_support manager::app_auto_install_targets_codex_only -- --exact
pnpm test:frontend -- src/tests/settings-workflows.spec.ts
```

Expected：FAIL，当前启动逻辑会自动安装所有探测到的适配器，界面也展示全部适配器。

**Step 3：实现 Codex 范围入口**

给 `AgentManager` 增加显式的 `auto_install_selected(&["codex"])`，应用启动调用它；保留通用方法供以后扩展。`list_agent_adapters` 命令首版只返回 `id == "codex"` 的条目，其他适配器源码和测试保留。

前端再做一次防御性过滤，避免旧后端返回其他适配器时误展示。

**Step 4：运行测试并确认通过**

Run：重复 Step 2，Expected：PASS。

**Step 5：提交**

```bash
git add src-tauri/src/agents/mod.rs src-tauri/src/lib.rs src/components/SettingsAgentsSection.tsx src-tauri/tests/agent_support/manager.rs src/tests/settings-workflows.spec.ts
git diff --cached --name-status
git check-ignore -v --no-index -- src-tauri/src/agents/mod.rs
git check-ignore -v --no-index -- src-tauri/src/lib.rs
git check-ignore -v --no-index -- src/components/SettingsAgentsSection.tsx
git check-ignore -v --no-index -- src-tauri/tests/agent_support/manager.rs
git check-ignore -v --no-index -- src/tests/settings-workflows.spec.ts
git commit -m "feat: focus agent integration on Codex"
```

### Task 9：补齐使用说明和真实端到端验收

**Files：**

- Modify: `README.zh.md`
- Modify: `README.md`
- Create: `docs/codex-task-reminders.md`
- Create: `src-tauri/tests/fixtures/codex-user-prompt-submit.json`
- Create: `src-tauri/tests/fixtures/codex-stop.json`

**Step 1：添加真实 Hook 样例并进行本地验收**

样例必须使用 OpenAI 当前字段，但内容为脱敏数据：

```json
{
  "session_id": "019ff53e-539b-7053-af7c-01b608aa7059",
  "turn_id": "turn-1",
  "hook_event_name": "Stop",
  "cwd": "/tmp/demo",
  "model": "gpt-5",
  "permission_mode": "default",
  "last_assistant_message": "任务已完成"
}
```

先运行 `pnpm tauri dev`，再执行一个真实 Codex 任务，检查：

1. 提交后宠物进入工作状态。
2. 任务结束后出现 `任务完成啦，快去看看吧。`。
3. 同一 Stop 重放不会二次发声。
4. 点击气泡打开 `codex://threads/{session_id}`。
5. 关闭或点击后重启 CoPet，不恢复已处理提醒。
6. 未处理提醒在重启后恢复，但不播放庆祝音。

**Step 2：编写启动和修改说明**

文档说明以下命令和区别：

```bash
pnpm tauri dev       # 开发模式，React 改动热更新；Rust 改动会重编译
pnpm tauri build     # 生成正式安装包
```

同时说明：安装后可从应用程序目录启动；开启“登录后自动启动”后，关机再登录会由 macOS 自动启动；前端样式和文案通常无需完整打包即可预览，Rust 或依赖变更需要重新编译。

**Step 3：验证文档和样例**

Run：

```bash
python3 -m json.tool src-tauri/tests/fixtures/codex-user-prompt-submit.json
python3 -m json.tool src-tauri/tests/fixtures/codex-stop.json
rg -n "pnpm tauri dev|登录后自动启动|任务完成啦" README.zh.md docs/codex-task-reminders.md
```

Expected：两个 JSON 合法，三个关键说明均可检索到。

**Step 4：提交**

```bash
git add README.zh.md README.md docs/codex-task-reminders.md src-tauri/tests/fixtures/codex-user-prompt-submit.json src-tauri/tests/fixtures/codex-stop.json
git diff --cached --name-status
git check-ignore -v --no-index -- README.zh.md
git check-ignore -v --no-index -- README.md
git check-ignore -v --no-index -- docs/codex-task-reminders.md
git check-ignore -v --no-index -- src-tauri/tests/fixtures/codex-user-prompt-submit.json
git check-ignore -v --no-index -- src-tauri/tests/fixtures/codex-stop.json
git commit -m "docs: explain Codex task reminders"
```

### Task 10：完整验证、代码审查、PR 与本机安装

**Files：**

- 只有验证发现缺陷时才修改对应文件，不做顺手改动。

**Step 1：使用完成前验证技能**

执行 `@superpowers:verification-before-completion`，从干净工作树运行完整门禁：

```bash
pnpm test:frontend
pnpm test:rust
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
git status --short
```

Expected：全部通过，`git status --short` 无输出。

**Step 2：执行代码审查**

使用 `@superpowers:requesting-code-review` 检查：

- Hook 不会阻塞 Codex。
- 不记录完整提示、完整回答或令牌。
- URL 参数不经 shell 拼接。
- 去重、重启恢复和多任务优先级符合设计。
- React 组件没有直接调用 `invoke`。
- 新测试均在项目规定目录。

如审查发现问题，按 TDD 增加回归测试、修复并单独提交。

**Step 3：构建 macOS 应用**

```bash
pnpm tauri build --bundles app
```

Expected：生成 `src-tauri/target/release/bundle/macos/CoPet.app`。

**Step 4：通过 PR 合并**

不得直接提交到 `main`。推送 `feature/codex-completion-pet`，创建 PR，等待检查通过后合并。使用 `@superpowers:finishing-a-development-branch` 完成分支收尾。

**Step 5：取得用户确认后安装应用**

安装到 `/Applications/CoPet.app` 会覆盖已有同名应用，必须先只读检查目标是否存在并单独向用户确认。确认后用可恢复方式备份旧版本，再复制新构建并启动；不要使用宽泛或递归删除命令。

安装后验收：

```text
CoPet 可从“应用程序”启动
Codex 适配器显示已安装且健康
完成一个真实 Codex 任务后出现一次提醒
点击提醒打开对应 Codex 任务
开启登录自启动后，系统状态查询返回 true
```
