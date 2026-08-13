import { expect, test } from "@playwright/test";

import {
  copet,
  createAppHarness,
  type RuntimeStatus,
  type TaskNotification,
} from "./app-harness";

function notification(
  id: string,
  status: TaskNotification["status"],
  title: string,
  updatedAtMs: number,
): TaskNotification {
  const [, sessionId, turnId] = id.split(":");
  return {
    id,
    agent: "codex",
    displayName: "Codex",
    sessionId,
    turnId,
    status,
    title,
    summary: status === "completed" ? "任务执行完成" : null,
    unread: status !== "running",
    updatedAtMs,
    action: null,
  };
}

function runtimeWith(
  notifications: TaskNotification[],
  state: RuntimeStatus["currentState"]["state"] = "waving",
): RuntimeStatus {
  return {
    port: 8765,
    endpoint: "http://127.0.0.1:8765/v1/events",
    currentState: { state, sinceMs: 100, idleAfterMs: null },
    messages: [],
    notifications,
    attention: null,
    acceptedEvents: notifications.length,
    rejectedEvents: 0,
  };
}

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

test("Codex 完成提醒可点击并按通知 id 打开", async ({ browser }) => {
  const completed = notification(
    "codex:thread-1:turn-1",
    "completed",
    "修复登录按钮",
    100,
  );
  const harness = await createAppHarness(browser, {
    runtimeStatus: {
      ...runtimeWith([completed]),
      messages: [
        {
          agent: "codex",
          displayName: "Codex",
          text: "Done.",
          updatedAtMs: 100,
        },
      ],
    },
    state: zhState,
  });
  const page = await harness.openPage("pet");

  await expect(page.getByText("任务完成啦，快去看看吧。")).toBeVisible();
  await expect(page.getByText("修复登录按钮")).toBeVisible();
  await expect(page.getByTestId("pet-agent-message")).toHaveCount(0);
  await expect(page.getByTestId("task-notification")).not.toHaveAttribute(
    "role",
    "button",
  );
  await expect(
    page.getByTestId("task-notification").locator(":scope > button").first(),
  ).toContainText("修复登录按钮");
  await page.getByTestId("task-notification").click();

  expect(harness.calls).toContainEqual({
    command: "open_task_notification",
    args: { id: "codex:thread-1:turn-1" },
  });
  await expect(page.getByTestId("task-notification")).toHaveCount(0);
});

test("等待、失败和并行任务分别显示，并按 id 关闭", async ({ browser }) => {
  const waiting = notification(
    "codex:thread-1:turn-1",
    "waiting",
    "等待确认权限",
    200,
  );
  const failed = notification(
    "codex:thread-2:turn-2",
    "failed",
    "运行测试",
    100,
  );
  const harness = await createAppHarness(browser, {
    runtimeStatus: runtimeWith([waiting, failed], "waiting"),
    state: zhState,
  });
  const page = await harness.openPage("pet");

  await expect(page.getByTestId("task-notification")).toHaveCount(2);
  await expect(page.getByText("任务需要你处理，快去看看吧。")).toBeVisible();
  await expect(page.getByText("任务出错了，快去看看吧。")).toBeVisible();

  const failedRow = page.getByTestId("task-notification").filter({
    hasText: "运行测试",
  });
  await failedRow.hover();
  await failedRow.getByRole("button", { name: "关闭" }).click();

  expect(harness.calls).toContainEqual({
    command: "dismiss_task_notification",
    args: { id: "codex:thread-2:turn-2" },
  });
  await expect(page.getByTestId("task-notification")).toHaveCount(1);
});

test("启动恢复和后续完成任务都保持静默并更新提醒", async ({
  browser,
}) => {
  const first = notification(
    "codex:thread-1:turn-1",
    "completed",
    "第一个任务",
    100,
  );
  const second = notification(
    "codex:thread-2:turn-2",
    "completed",
    "第二个任务",
    200,
  );
  const bootstrap = runtimeWith([first]);
  bootstrap.attention = {
    id: first.id,
    kind: "completed",
    occurredAtMs: 100,
  };
  const harness = await createAppHarness(browser, {
    runtimeStatus: bootstrap,
    state: zhState,
  });
  const page = await harness.openPage("pet");

  await expect(page.getByTestId("task-notification")).toHaveCount(1);
  expect(await harness.playedSoundUrls(page)).toEqual([]);

  await harness.emitRuntimeUpdate(page, {
    currentState: { state: "waving" },
    notifications: [first],
    attention: { id: first.id, kind: "completed", occurredAtMs: 100 },
  });
  await expect(page.getByTestId("task-notification")).toHaveCount(1);
  expect(await harness.playedSoundUrls(page)).toEqual([]);

  await harness.emitRuntimeUpdate(page, {
    currentState: { state: "waving" },
    notifications: [first],
    attention: { id: first.id, kind: "completed", occurredAtMs: 100 },
  });
  expect(await harness.playedSoundUrls(page)).toEqual([]);

  await harness.emitRuntimeUpdate(page, {
    currentState: { state: "waving" },
    notifications: [second, first],
    attention: { id: second.id, kind: "completed", occurredAtMs: 200 },
  });
  await expect(page.getByTestId("task-notification")).toHaveCount(2);
  expect(await harness.playedSoundUrls(page)).toEqual([]);
});

test("同一任务等待后完成仍更新提醒且宠物保持静态静音", async ({ browser }) => {
  const waiting = notification(
    "codex:thread-1:turn-1",
    "waiting",
    "等待确认",
    100,
  );
  const completed = notification(
    "codex:thread-1:turn-1",
    "completed",
    "任务完成",
    200,
  );
  const harness = await createAppHarness(browser, { state: zhState });
  const page = await harness.openPage("pet");

  await harness.emitRuntimeUpdate(page, {
    currentState: { state: "waiting" },
    notifications: [waiting],
    attention: { id: waiting.id, kind: "waiting", occurredAtMs: 100 },
  });
  await page.waitForTimeout(50);
  expect(await harness.playedSoundUrls(page)).toHaveLength(0);

  await harness.emitRuntimeUpdate(page, {
    currentState: { state: "waving" },
    notifications: [completed],
    attention: { id: completed.id, kind: "completed", occurredAtMs: 200 },
  });
  await expect(page.getByTestId("task-notification")).toHaveAttribute(
    "data-status",
    "completed",
  );
  await expect(page.locator(".pet-sprite")).toHaveAttribute("data-pet-state", "idle");
  await expect(page.locator(".pet-sprite")).toHaveAttribute("data-animated", "false");
  await expect(page.getByTestId("pet-emotion-overlay")).toHaveCount(0);
  expect(await harness.playedSoundUrls(page)).toEqual([]);

  await harness.emitRuntimeUpdate(page, {
    currentState: { state: "waving" },
    notifications: [completed],
    attention: { id: completed.id, kind: "completed", occurredAtMs: 200 },
  });
  await expect(page.getByTestId("task-notification")).toHaveCount(1);
  expect(await harness.playedSoundUrls(page)).toEqual([]);
});
