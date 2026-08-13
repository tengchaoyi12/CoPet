# Codex Quick Action Compact Cards Implementation Plan

> **For Codex:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 Codex 快捷操作提醒改为最多三张、默认紧凑、单卡按需展开的安全操作卡片。

**Architecture:** 保持现有 Rust 动作协议和 `useTaskNotifications` 不变，在 `PetWindow` 限制显示数量，在 `TaskNotifications` 内维护唯一展开项并重组信息层级。行为契约继续通过 Playwright harness 验证，视觉细节只通过构建和现场审查验证。

**Tech Stack:** React、TypeScript、Playwright、CSS、Tauri 前端 harness

---

### Task 1: 锁定紧凑卡片行为

**Files:**

- Modify: `src/tests/codex-task-actions.spec.ts`
- Modify: `src/tests/codex-task-notifications.spec.ts`

**Step 1: Write the failing tests**

增加以下行为测试：

- 权限卡折叠时显示命令和 `仅允许这次`，但不显示工具、目录和范围。
- 点击 `详情`后显示完整信息，再点击另一张卡片的 `详情`时只保留后一张展开。
- waiting 且没有安全快捷动作时只显示 `打开 Codex`。
- 四条提醒只渲染前三条，但不调用 dismiss 命令。
- 既有 `continueOnce`、`allowOnce`、fallback 和 actionId 隔离行为保持不变。

**Step 2: Run tests to verify RED**

Run:

```bash
pnpm test:frontend src/tests/codex-task-actions.spec.ts src/tests/codex-task-notifications.spec.ts
```

Expected: FAIL，当前界面没有 `详情`按钮、权限信息默认全部展开、权限按钮仍为 `允许并继续`，并且会渲染全部提醒。

**Step 3: Commit the test contract only after implementation is green**

本任务不单独提交红色测试；与 Task 2 的最小实现一起提交。

### Task 2: 实现渐进披露与三张上限

**Files:**

- Modify: `src/components/TaskNotifications.tsx`
- Modify: `src/PetWindow.tsx`
- Modify: `src/lib/i18n.ts`
- Modify: `src/styles.css`
- Test: `src/tests/codex-task-actions.spec.ts`
- Test: `src/tests/codex-task-notifications.spec.ts`

**Step 1: Implement the minimal component behavior**

- `PetWindow` 使用 `taskNotifications.notifications.slice(0, 3)` 作为显示列表，不修改原 store。
- `TaskNotifications` 使用本地 `expandedNotificationId`，详情按钮切换当前卡片并收起其他卡片。
- 权限卡摘要优先使用 `action.command`，否则使用 `requestedAction` 或通知摘要。
- 普通继续摘要优先使用 `requestedAction`。
- 详情区域承载完整动作说明、工具、命令、目录、一次性范围和 `在 Codex 中处理`。
- waiting 且没有可用快捷动作时显示 `打开 Codex`。
- 权限主按钮文案改为 `仅允许这次`，调用仍是 `allowOnce`。

**Step 2: Run tests to verify GREEN**

Run:

```bash
pnpm test:frontend src/tests/codex-task-actions.spec.ts src/tests/codex-task-notifications.spec.ts
```

Expected: 全部 PASS。

**Step 3: Refactor while green**

提取最小的摘要和状态文案辅助函数，避免在 JSX 中重复分支；不新增领域模型或 Rust 字段。

**Step 4: Re-run focused tests**

Run the same command. Expected: PASS with no warnings.

**Step 5: Commit**

```bash
git add docs/plans/2026-08-13-codex-quick-action-compact-cards-design.md docs/plans/2026-08-13-codex-quick-action-compact-cards.md src/components/TaskNotifications.tsx src/PetWindow.tsx src/lib/i18n.ts src/styles.css src/tests/codex-task-actions.spec.ts src/tests/codex-task-notifications.spec.ts
git diff --cached --name-status
git check-ignore -v --no-index -- docs/plans/2026-08-13-codex-quick-action-compact-cards-design.md docs/plans/2026-08-13-codex-quick-action-compact-cards.md src/components/TaskNotifications.tsx src/PetWindow.tsx src/lib/i18n.ts src/styles.css src/tests/codex-task-actions.spec.ts src/tests/codex-task-notifications.spec.ts
git commit -m "feat(actions): simplify quick action cards"
```

### Task 3: 回归、视觉审查与交付

**Files:**

- Modify only if a verified regression requires it.

**Step 1: Run the focused regression**

```bash
pnpm test:frontend src/tests/codex-task-actions.spec.ts src/tests/codex-task-notifications.spec.ts src/tests/static-pet-codex-focus.spec.ts
```

Expected: PASS。

**Step 2: Run the required project checks**

```bash
pnpm test:frontend
pnpm test:rust
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

Expected: 全部退出码为 0。

**Step 3: Run design review on the rendered result**

使用真实 Playwright harness 渲染普通继续、权限请求、需要回复和三任务场景，确认：

- 320px 到 420px 宽度无截断或重叠。
- 初始状态紧凑，单卡展开后仍可读。
- 主次按钮层级明确，权限命令在折叠态可见。

**Step 4: Request code review**

使用 `superpowers:requesting-code-review`，重点检查事件冒泡、actionId 隔离、显示上限是否误删提醒，以及无快捷动作时的打开逻辑。

**Step 5: Build and install**

```bash
pnpm tauri build --bundles app
```

替换 `/Applications/CoPet.app`，重启并现场验证三种卡片和展开交互。

**Step 6: Push and update the existing PR**

推送现有 `feature/codex-completion-pet` 分支并更新 PR #1，不创建重复 PR。
