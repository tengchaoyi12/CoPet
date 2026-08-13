import { expect, test } from "@playwright/test";

import {
  copet,
  createAppHarness,
  type RuntimeStatus,
  type TaskNotification,
} from "./app-harness";

type TaskAction = {
  id: string;
  kind: "continue" | "permission";
  state: "pending" | "resolving" | "expired";
  label: string;
  requestedAction: string;
  toolName: string | null;
  command: string | null;
  cwd: string | null;
  expiresAtMs: number;
  quickActionAllowed: boolean;
};

const zhState = {
  currentPetId: copet.id,
  currentSoundPackId: "system:copet",
  locale: "zh-CN" as const,
  localePreference: "zh-CN" as const,
  pets: [copet],
  onboardingComplete: false,
  petInteractions: {
    enableClickSounds: true,
    cooldownStyle: "normal" as const,
    enableStartupAnimation: false,
  },
};

function action(
  id: string,
  kind: TaskAction["kind"],
  overrides: Partial<TaskAction> = {},
): TaskAction {
  return {
    id,
    kind,
    state: "pending",
    label: kind === "continue" ? "继续执行" : "允许并继续",
    requestedAction:
      kind === "continue" ? "测试已经完成，可以继续收尾。" : "执行测试命令",
    toolName: kind === "permission" ? "shell" : null,
    command: kind === "permission" ? "pnpm test:frontend" : null,
    cwd: kind === "permission" ? "/Users/test/CoPet" : null,
    expiresAtMs: Date.now() + 600_000,
    quickActionAllowed: true,
    ...overrides,
  };
}

function notification(id: string, taskAction: TaskAction): TaskNotification {
  return {
    id,
    agent: "codex",
    displayName: "Codex",
    sessionId: id,
    turnId: null,
    status: "waiting",
    title: id,
    summary: taskAction.requestedAction,
    unread: true,
    updatedAtMs: Date.now(),
    action: taskAction,
  } as TaskNotification;
}

function runtimeWith(notifications: TaskNotification[]): RuntimeStatus {
  return {
    port: 8765,
    endpoint: "http://127.0.0.1:8765/v1/events",
    currentState: { state: "waiting", sinceMs: 100, idleAfterMs: null },
    messages: [],
    notifications,
    attention: null,
    acceptedEvents: notifications.length,
    rejectedEvents: 0,
  };
}

test("继续按钮按 action id 提交 continueOnce 且不打开 Codex", async ({
  browser,
}) => {
  const taskAction = action("action-continue", "continue");
  const harness = await createAppHarness(browser, {
    runtimeStatus: runtimeWith([notification("task-continue", taskAction)]),
    state: zhState,
  });
  const page = await harness.openPage("pet");

  await expect(page.getByText("测试已经完成，可以继续收尾。")).toBeVisible();
  await page.getByRole("button", { name: "继续执行", exact: true }).click();

  expect(harness.calls).toContainEqual({
    command: "resolve_task_action",
    args: { id: "action-continue", decision: "continueOnce" },
  });
  expect(harness.calls.some((call) => call.command === "open_task_notification")).toBe(
    false,
  );
});

test("权限按钮展示范围并按 action id 提交 allowOnce", async ({ browser }) => {
  const taskAction = action("action-permission", "permission");
  const harness = await createAppHarness(browser, {
    runtimeStatus: runtimeWith([notification("task-permission", taskAction)]),
    state: zhState,
  });
  const page = await harness.openPage("pet");

  await expect(page.getByText("pnpm test:frontend")).toBeVisible();
  await expect(page.getByText("/Users/test/CoPet")).toBeVisible();
  await expect(page.getByText("仅允许本次操作")).toBeVisible();
  await page.getByRole("button", { name: "允许并继续", exact: true }).click();

  expect(harness.calls).toContainEqual({
    command: "resolve_task_action",
    args: { id: "action-permission", decision: "allowOnce" },
  });
  expect(harness.calls.some((call) => call.command === "open_task_notification")).toBe(
    false,
  );
});

test("命令失败时保留提醒并显示错误", async ({ browser }) => {
  const taskAction = action("action-error", "continue");
  const harness = await createAppHarness(browser, {
    commandErrors: { resolve_task_action: "操作已失效，请回到 Codex 处理" },
    runtimeStatus: runtimeWith([notification("task-error", taskAction)]),
    state: zhState,
  });
  const page = await harness.openPage("pet");

  await page.getByRole("button", { name: "继续执行", exact: true }).click();

  await expect(page.getByText("操作已失效，请回到 Codex 处理")).toBeVisible();
  await expect(page.getByTestId("task-notification")).toHaveCount(1);
});

test("在 Codex 中处理先 fallback 再打开对应任务", async ({ browser }) => {
  const taskAction = action("action-fallback", "continue");
  const harness = await createAppHarness(browser, {
    runtimeStatus: runtimeWith([notification("task-fallback", taskAction)]),
    state: zhState,
  });
  const page = await harness.openPage("pet");

  await page.getByRole("button", { name: "在 Codex 中处理", exact: true }).click();

  await expect
    .poll(
      () =>
        harness.calls.filter((call) =>
          ["resolve_task_action", "open_task_notification"].includes(call.command),
        ).length,
    )
    .toBe(2);
  const relevantCalls = harness.calls.filter((call) =>
    ["resolve_task_action", "open_task_notification"].includes(call.command),
  );
  expect(relevantCalls).toEqual([
    {
      command: "resolve_task_action",
      args: { id: "action-fallback", decision: "fallback" },
    },
    {
      command: "open_task_notification",
      args: { id: "task-fallback" },
    },
  ]);
});

test("不安全、过期和需要抉择的动作不显示快捷批准", async ({ browser }) => {
  const unsafe = notification(
    "task-unsafe",
    action("action-unsafe", "permission", { quickActionAllowed: false }),
  );
  const expired = notification(
    "task-expired",
    action("action-expired", "continue", {
      state: "expired",
      quickActionAllowed: false,
    }),
  );
  const choice = notification(
    "task-choice",
    action("action-choice", "continue", {
      requestedAction: "请选择 A 或 B",
      quickActionAllowed: false,
    }),
  );
  const harness = await createAppHarness(browser, {
    runtimeStatus: runtimeWith([unsafe, expired, choice]),
    state: zhState,
  });
  const page = await harness.openPage("pet");

  await expect(page.getByRole("button", { name: "继续执行", exact: true })).toHaveCount(
    0,
  );
  await expect(page.getByRole("button", { name: "允许并继续", exact: true })).toHaveCount(
    0,
  );
});

test("并行任务按钮严格绑定各自 action id", async ({ browser }) => {
  const first = notification("task-first", action("action-first", "continue"));
  const second = notification("task-second", action("action-second", "continue"));
  const harness = await createAppHarness(browser, {
    runtimeStatus: runtimeWith([first, second]),
    state: zhState,
  });
  const page = await harness.openPage("pet");

  const secondCard = page.getByTestId("task-notification").filter({
    hasText: "task-second",
  });
  await secondCard.getByRole("button", { name: "继续执行", exact: true }).click();

  expect(harness.calls).toContainEqual({
    command: "resolve_task_action",
    args: { id: "action-second", decision: "continueOnce" },
  });
  expect(harness.calls).not.toContainEqual({
    command: "resolve_task_action",
    args: { id: "action-first", decision: "continueOnce" },
  });
});
