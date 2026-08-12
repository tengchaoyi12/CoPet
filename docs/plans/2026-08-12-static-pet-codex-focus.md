# 静态宠物与 Codex 前台清除提醒 Implementation Plan

> **For Codex:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 让宠物始终显示静态帧并保留拖拽，短按宠物打开 Codex，Codex 成为前台后自动清除全部完成提醒。

**Architecture:** React 宠物窗口固定渲染 `idle` 第一帧，并让现有拖拽状态机负责区分短按和真实拖拽；短按通过新的 Tauri 命令复用 `task_opener` 打开 Codex。Rust 运行时增加“清除全部完成提醒”领域操作，macOS 使用 `NSWorkspaceDidActivateApplicationNotification` 监听前台应用并通过现有运行时事件刷新界面。

**Tech Stack:** React、TypeScript、Playwright、Tauri 2、Rust、objc2 AppKit、Cargo 集成测试。

---

## 实施约束

- 使用当前专用 worktree：`/Users/beizhi/Documents/Codex/2026-08-12/code/work/CoPet/.worktrees/codex-completion-pet`。
- 每个行为先写失败测试，再写最小实现。
- React 组件继续平铺在 `src/components/`，Tauri 调用必须经 `src/lib/appCommands.ts` 或 Hook 包装。
- Rust 测试只放在 `src-tauri/tests/*.rs`。
- 不删除现有动画基础设施；本次只让 `PetWindow` 采用静态呈现。
- 不直接提交到 `main`；所有提交保留在 `feature/codex-completion-pet`。

### Task 1：任务通知存储清除全部完成提醒

**Files:**
- Modify: `src-tauri/src/task_notifications.rs`
- Test: `src-tauri/tests/task_notifications.rs`

**Step 1：写失败测试**

在 `src-tauri/tests/task_notifications.rs` 新增测试，构造完成、等待、失败三类提醒，调用 `clear_completed()` 后断言：

```rust
#[test]
fn clear_completed_removes_only_completed_notifications() {
    let mut store = TaskNotificationStore::default();
    store.apply(event("session.stop", Some("done-a"), None, None), 100);
    store.apply(event("session.stop", Some("done-b"), None, None), 110);
    store.apply(event("permission.waiting", Some("waiting"), None, None), 120);
    store.apply(event("session.error", Some("failed"), None, None), 130);

    assert_eq!(store.clear_completed(), 2);

    let visible = store.visible();
    assert_eq!(visible.len(), 2);
    assert!(visible.iter().all(|item| item.status != TaskStatus::Completed));
    assert!(visible.iter().any(|item| item.status == TaskStatus::Waiting));
    assert!(visible.iter().any(|item| item.status == TaskStatus::Failed));
}
```

再加一个幂等测试：没有完成提醒时返回 `0` 且不改变现有提醒。

**Step 2：运行测试确认失败**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test task_notifications clear_completed
```

Expected: FAIL，提示 `TaskNotificationStore` 没有 `clear_completed` 方法。

**Step 3：实现最小领域操作**

在 `TaskNotificationStore` 中实现：

```rust
pub fn clear_completed(&mut self) -> usize {
    let before = self.notifications.len();
    self.notifications
        .retain(|_, task| task.status != TaskStatus::Completed);
    before - self.notifications.len()
}
```

**Step 4：运行测试确认通过**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test task_notifications clear_completed
```

Expected: 新增的两个测试 PASS。

**Step 5：提交**

提交前执行暂存路径与忽略规则检查，然后提交：

```bash
git add src-tauri/src/task_notifications.rs src-tauri/tests/task_notifications.rs
git check-ignore -v --no-index -- src-tauri/src/task_notifications.rs src-tauri/tests/task_notifications.rs
git diff --cached --name-status
git commit -m "feat(codex): clear completed task reminders"
```

### Task 2：运行时清理、持久化与更新事件

**Files:**
- Modify: `src-tauri/src/runtime_server.rs`
- Test: `src-tauri/tests/runtime_server.rs`

**Step 1：写失败测试**

在 `src-tauri/tests/runtime_server.rs` 增加测试：先向 `RuntimeCore` 写入完成、等待、失败事件，再调用 `clear_completed_task_notifications()`，验证返回的 `RuntimeUpdate` 只保留等待和失败提醒，并验证重复调用不改变结果。

测试还应为 `RuntimeCore` 配置临时通知文件，读取保存后的 JSON，确认磁盘状态与返回更新一致。

**Step 2：运行测试确认失败**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test runtime_server clear_completed_task_notifications
```

Expected: FAIL，提示缺少清理方法。

**Step 3：实现运行时方法**

在 `RuntimeCore` 增加：

```rust
pub fn clear_completed_task_notifications(&mut self) -> RuntimeUpdate {
    let changed = self.task_notifications.clear_completed();
    if changed > 0 {
        self.save_task_notifications(now_ms());
    }
    self.latest_attention = None;
    self.runtime_update()
}
```

复用或提取现有状态组装逻辑，避免复制 `current_state`、`messages`、`notifications` 和 `attention` 字段。`RuntimeManager` 增加同名公开方法，通过互斥锁调用核心方法。

**Step 4：运行测试确认通过**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test runtime_server clear_completed_task_notifications
```

Expected: PASS，持久化文件只包含等待和失败提醒。

**Step 5：提交**

```bash
git add src-tauri/src/runtime_server.rs src-tauri/tests/runtime_server.rs
git check-ignore -v --no-index -- src-tauri/src/runtime_server.rs src-tauri/tests/runtime_server.rs
git diff --cached --name-status
git commit -m "feat(codex): clear completed reminders at runtime"
```

### Task 3：监听 Codex 成为 macOS 前台应用

**Files:**
- Create: `src-tauri/src/codex_focus.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/codex_focus.rs`

**Step 1：写应用标识失败测试**

新增 `src-tauri/tests/codex_focus.rs`，测试纯函数：

```rust
#[test]
fn recognizes_codex_bundle_identifier_only() {
    assert!(is_codex_bundle_identifier(Some("com.openai.codex")));
    assert!(!is_codex_bundle_identifier(Some("com.apple.finder")));
    assert!(!is_codex_bundle_identifier(None));
}
```

同时使用本机只读命令确认 `/Applications/ChatGPT.app` 的实际 bundle identifier；如果不是 `com.openai.codex`，将经过验证的标识加入显式白名单并补测试，不根据应用显示名做宽泛匹配。

**Step 2：运行测试确认失败**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test codex_focus
```

Expected: FAIL，模块或函数不存在。

**Step 3：实现纯函数与跨平台入口**

`src-tauri/src/codex_focus.rs` 提供：

```rust
pub fn is_codex_bundle_identifier(value: Option<&str>) -> bool {
    matches!(value, Some("com.openai.codex"))
}

pub fn install_codex_focus_observer(app: &tauri::AppHandle) {
    install_native_codex_focus_observer(app);
}
```

非 macOS 的 `install_native_codex_focus_observer` 为空实现，保证跨平台编译。

**Step 4：实现 macOS 原生观察者**

参考 `src-tauri/src/window_placement.rs` 已有 `NSWorkspace` 通知监听方式：

- 只订阅 `NSWorkspaceDidActivateApplicationNotification`。
- 从通知的 `userInfo` 获取已激活应用并读取 bundle identifier。
- 识别为 Codex 后，在 Tauri 主线程或安全工作线程中获取 `RuntimeManager`。
- 调用 `clear_completed_task_notifications()`。
- 通过现有 `emit_runtime_update` 将最新状态发送给宠物和设置窗口。
- observer block 按现有进程生命周期模式保存，避免悬垂回调。

在 `lib.rs` 导出模块，并在 `app.manage(runtime)` 之后安装观察者。必要时将 `emit_runtime_update` 调整为 `pub(crate)`，不改变事件名和载荷结构。

**Step 5：运行测试与格式检查**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test codex_focus
cargo test --manifest-path src-tauri/Cargo.toml --test runtime_server clear_completed_task_notifications
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
```

Expected: 全部 PASS。

**Step 6：提交**

```bash
git add src-tauri/src/codex_focus.rs src-tauri/src/lib.rs src-tauri/tests/codex_focus.rs
git check-ignore -v --no-index -- src-tauri/src/codex_focus.rs src-tauri/src/lib.rs src-tauri/tests/codex_focus.rs
git diff --cached --name-status
git commit -m "feat(codex): clear reminders when Codex activates"
```

### Task 4：增加打开 Codex 主界面的 Tauri 命令

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/appCommands.ts`
- Modify: `src/tests/app-harness.ts`
- Test: `src-tauri/tests/task_opener.rs`
- Test: `src/tests/static-pet-codex-focus.spec.ts`

**Step 1：写 Rust 失败测试**

扩展 `src-tauri/tests/task_opener.rs`，断言无任务标识时打开计划只包含 Codex 应用回退命令：

```rust
#[test]
fn codex_home_open_plan_uses_application_bundle() {
    let plan = codex_open_plan(None);
    assert!(plan.primary.is_empty());
    assert_eq!(plan.fallback, ["open", "-b", "com.openai.codex"]);
}
```

如果现有测试已覆盖该行为，不重复新增；直接以现有测试作为回归保护。

**Step 2：写前端命令失败测试**

新建 `src/tests/static-pet-codex-focus.spec.ts`，先测试短按宠物后调用：

```ts
expect(harness.calls).toContainEqual({
  command: "open_codex",
  args: {},
});
```

Expected: 当前宠物只产生交互动画，不存在 `open_codex` 调用。

**Step 3：实现命令边界**

在 Rust 中增加：

```rust
#[tauri::command]
fn open_codex() -> Result<(), String> {
    task_opener::open_codex_task(None).map_err(|error| error.to_string())
}
```

将命令加入 `generate_handler!`。在 `src/lib/appCommands.ts` 增加返回 `CommandResult` 的 `openCodex()` 包装，组件不得直接 `invoke`。

测试 harness 对 `open_codex` 返回 `null`，保留命令调用记录。

**Step 4：运行相关测试**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test task_opener
pnpm test:frontend src/tests/static-pet-codex-focus.spec.ts
```

Expected: Rust PASS；前端仍因宠物尚未绑定命令而 FAIL。

**Step 5：提交后端和命令包装**

```bash
git add src-tauri/src/lib.rs src/lib/appCommands.ts src/tests/app-harness.ts src-tauri/tests/task_opener.rs src/tests/static-pet-codex-focus.spec.ts
git check-ignore -v --no-index -- src-tauri/src/lib.rs src/lib/appCommands.ts src/tests/app-harness.ts src-tauri/tests/task_opener.rs src/tests/static-pet-codex-focus.spec.ts
git diff --cached --name-status
git commit -m "feat(codex): add pet shortcut command"
```

### Task 5：静态宠物、短按打开与拖拽保护

**Files:**
- Modify: `src/PetWindow.tsx`
- Modify: `src/components/PetSprite.tsx`
- Modify: `src/hooks/useMotionState.ts`
- Modify: `src/hooks/usePetSounds.ts`（仅当现有静音入口不足时）
- Modify: `src/tests/static-pet-codex-focus.spec.ts`
- Modify: `src/tests/pet-gestures.spec.ts`（仅调整被新产品行为替代的断言）

**Step 1：补齐前端失败测试**

在 `static-pet-codex-focus.spec.ts` 覆盖：

1. 宠物 `data-animated="false"`，`data-pet-state="idle"`，没有情绪覆盖层。
2. 发出运行、等待、完成注意事件后仍保持同一静态状态。
3. 单击宠物只调用一次 `open_codex`。
4. 双击宠物只调用一次 `open_codex`。
5. 指针移动超过现有拖拽阈值后释放，不调用 `open_codex`。
6. 完成提醒继续渲染在宠物上方。
7. 静态模式不播放点击、拖动落地或 Agent 状态音效。

**Step 2：运行测试确认失败**

Run:

```bash
pnpm test:frontend src/tests/static-pet-codex-focus.spec.ts
```

Expected: 静态帧、打开命令和拖拽保护断言 FAIL。

**Step 3：实现固定静态视图**

在 `PetWindow.tsx` 定义稳定常量：

```ts
const staticPetView: ComposedView = {
  bodySpriteRow: "idle",
  emotionOverlay: null,
  dragging: false,
};
```

给 `PetSprite` 传入 `composed={staticPetView}` 和 `animated={false}`。保留任务提醒、窗口尺寸和右键菜单逻辑；不再让 LayeredPetState 或启动动画覆盖宠物显示。删除仅为动画和状态音效存在的调用时，先确认其他功能没有依赖；如果仍需保留 Hook，则让其结果不参与渲染和声音。

**Step 4：实现短按与拖拽区分**

在 `useMotionState` 增加 `onPrimaryAction` 回调，并复用现有 `dragDistanceRef`：

- 单次指针序列累计位移未达到 `pointerMoveJitterThreshold` 时调用一次 `onPrimaryAction`。
- 达到阈值时只完成拖拽。
- `pointerup`、`pointercancel` 和窗口失焦共用一次性结束函数，结束后立即清空当前序列，避免双击或多事件重复调用。
- macOS 原生 `tauri://move` 和 Windows 指针移动都更新同一累计距离。
- 右键、长按菜单和通知气泡点击不调用 `onPrimaryAction`。

`PetWindow` 将 `openCodex()` 传给该回调；失败时使用现有 toast 或 `notifyFailed` 错误路径。不要在展示组件中直接调用 Tauri `invoke`。

如果真实 macOS 的 `startDragging()` 不会为零位移短按产生可靠的释放事件，先写一个最小现场诊断，再将短按检测放回 `PetSprite` 的 `onClick`，同时由 Motion Hook 暴露“本次是否发生拖拽”的消费式标记；不要同时保留两条打开路径。

**Step 5：运行前端相关测试**

Run:

```bash
pnpm test:frontend src/tests/static-pet-codex-focus.spec.ts
pnpm test:frontend src/tests/pet-gestures.spec.ts
pnpm test:frontend src/tests/codex-task-notifications.spec.ts
```

Expected: 全部 PASS；任务气泡仍可单独点击，点击关闭按钮不会打开 Codex。

**Step 6：提交**

```bash
git add src/PetWindow.tsx src/components/PetSprite.tsx src/hooks/useMotionState.ts src/hooks/usePetSounds.ts src/tests/static-pet-codex-focus.spec.ts src/tests/pet-gestures.spec.ts
git check-ignore -v --no-index -- src/PetWindow.tsx src/components/PetSprite.tsx src/hooks/useMotionState.ts src/hooks/usePetSounds.ts src/tests/static-pet-codex-focus.spec.ts src/tests/pet-gestures.spec.ts
git diff --cached --name-status
git commit -m "feat(pet): make static pet open Codex"
```

只暂存实际发生修改的文件；未修改的可选文件不得出现在提交中。

### Task 6：完整验收、安装与 PR 更新

**Files:**
- Modify: `docs/codex-task-reminders.md`
- Modify: `README.zh.md`
- Modify: `README.md`

**Step 1：更新用户文档**

说明：

- 宠物现在固定显示静态帧。
- 可以拖拽调整位置。
- 短按宠物打开 Codex。
- Codex 成为前台后自动清除完成提醒，但保留等待和失败提醒。

不要声称非 macOS 平台支持前台自动清理。

**Step 2：运行完整质量检查**

Run:

```bash
pnpm test:frontend
pnpm test:rust
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
```

Expected:

- 前端全部测试 PASS。
- Rust 全部测试 PASS。
- Vite/TypeScript 构建成功。
- Rust 格式检查无输出并返回 `0`。

**Step 3：构建 macOS 应用**

Run:

```bash
pnpm tauri build
```

Expected: 生成 `src-tauri/target/release/bundle/macos/CoPet.app`。

**Step 4：现场验收**

安装新构建前先正常退出旧 CoPet，再替换 `/Applications/CoPet.app` 并启动。依次验证：

1. 宠物保持静态帧。
2. 拖动宠物不会打开 Codex。
3. 短按宠物打开 Codex。
4. 发送一条本机模拟完成事件，完成气泡出现。
5. 切换到其他应用再回到 Codex，完成气泡自动消失。
6. 等待和失败提醒不会被清除。

安装和启动 GUI 应用需要使用明确的系统权限批准；不得覆盖用户其他应用或配置。

**Step 5：提交文档**

```bash
git add README.md README.zh.md docs/codex-task-reminders.md
git check-ignore -v --no-index -- README.md README.zh.md docs/codex-task-reminders.md
git diff --cached --name-status
git commit -m "docs(codex): explain static pet reminder behavior"
```

**Step 6：最终审查并推送**

- 使用 `superpowers:requesting-code-review` 审查当前分支相对 `main` 的差异。
- 修复所有 Critical 和 Important 问题并重新运行受影响测试。
- 使用 `superpowers:verification-before-completion` 重新核验完整命令输出。
- 推送 `feature/codex-completion-pet`，更新现有 PR #1，不创建重复 PR。

最终应报告实际测试数量、构建产物、现场验收结果和所有修改文件。
