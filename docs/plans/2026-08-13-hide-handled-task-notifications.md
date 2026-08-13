# Hide Handled Task Notifications Implementation Plan

> **For Codex:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 成功处理或打开任务后立即隐藏对应提醒，同时保留失败和未处理提醒。

**Architecture:** 保持 Rust 任务状态机和命令协议不变，在 `useTaskNotifications` 的展示边界过滤已读提醒。后端已经在成功快捷操作和成功打开任务后设置 `unread=false`，前端只需让这一状态真正控制可见性。

**Tech Stack:** React、TypeScript、Tauri、Playwright

---

### Task 1: 用行为测试定义隐藏规则

**Files:**
- Modify: `src/tests/codex-task-actions.spec.ts`
- Modify: `src/tests/app-harness.ts`

**Step 1: Write the failing tests**

- 在继续操作成功后断言任务卡片数量为零。
- 在一次性允许成功后断言任务卡片数量为零。
- 新增无快捷动作的 waiting 提醒，点击“打开 Codex”成功后断言卡片数量为零。
- 保留既有失败测试，确认命令失败时卡片仍存在。
- 让测试 harness 与真实后端一致：打开只将提醒设为已读，关闭才删除。

**Step 2: Run test to verify it fails**

Run: `pnpm test:frontend src/tests/codex-task-actions.spec.ts`

Expected: 新增的成功隐藏断言失败，卡片数量仍为一。

### Task 2: 实现最小可见性过滤

**Files:**
- Modify: `src/hooks/useTaskNotifications.ts`

**Step 1: Write minimal implementation**

在 hook 返回展示数据前过滤 `unread=false` 的提醒；不可见总开关仍优先返回空数组。

**Step 2: Run focused test to verify it passes**

Run: `pnpm test:frontend src/tests/codex-task-actions.spec.ts`

Expected: PASS。

**Step 3: Run related notification tests**

Run: `pnpm test:frontend src/tests/codex-task-notifications.spec.ts`

Expected: PASS，最多三条和既有提醒行为无回归。

### Task 3: 验证与交付

**Files:**
- Verify: `src/hooks/useTaskNotifications.ts`
- Verify: `src/tests/codex-task-actions.spec.ts`

**Step 1: Run full frontend tests**

Run: `pnpm test:frontend`

Expected: PASS。

**Step 2: Build**

Run: `pnpm build`

Expected: PASS。

**Step 3: Check diff**

Run: `git diff --check`

Expected: no output。

**Step 4: Review and commit**

检查变更仅包含设计文档、实施计划、hook 和对应测试；按项目规则检查 staged 路径的 ignore 状态后提交。
