import { expect, test } from "@playwright/test";

import {
  copet,
  createAppHarness,
  type RuntimeStatus,
  type TaskNotification,
} from "./app-harness";

const staticPetState = {
  currentPetId: copet.id,
  pets: [copet],
  onboardingComplete: false,
  petInteractions: {
    enableClickSounds: true,
    cooldownStyle: "normal" as const,
    enableStartupAnimation: false,
  },
};

async function shortPress(page: import("@playwright/test").Page, pointerId = 1) {
  const spriteFrame = page.locator(".pet-sprite-frame");
  await spriteFrame.dispatchEvent("pointerdown", {
    button: 0,
    clientX: 50,
    clientY: 50,
    isPrimary: true,
    pointerId,
    pointerType: "mouse",
  });
  await page.evaluate((id) => {
    window.dispatchEvent(
      new PointerEvent("pointerup", { button: 0, pointerId: id }),
    );
  }, pointerId);
}

async function dragPet(
  page: import("@playwright/test").Page,
  distance: number,
  pointerId = 1,
) {
  const spriteFrame = page.locator(".pet-sprite-frame");
  await spriteFrame.dispatchEvent("pointerdown", {
    button: 0,
    clientX: 50,
    clientY: 50,
    isPrimary: true,
    pointerId,
    pointerType: "mouse",
  });
  await page.evaluate(
    ({ id, x }) => {
      window.dispatchEvent(
        new PointerEvent("pointermove", {
          clientX: x,
          clientY: 50,
          pointerId: id,
        }),
      );
      window.dispatchEvent(new PointerEvent("pointerup", { pointerId: id }));
    },
    { id: pointerId, x: 50 + distance },
  );
}

function completedNotification(): TaskNotification {
  return {
    id: "codex:thread-1:turn-1",
    agent: "codex",
    displayName: "Codex",
    sessionId: "thread-1",
    turnId: "turn-1",
    status: "completed",
    title: "完成静态宠物",
    summary: "任务执行完成",
    unread: true,
    updatedAtMs: 100,
  };
}

function runtimeWithCompleted(): RuntimeStatus {
  return {
    port: 8765,
    endpoint: "http://127.0.0.1:8765/v1/events",
    currentState: { state: "waving", sinceMs: 100, idleAfterMs: null },
    messages: [],
    notifications: [completedNotification()],
    attention: null,
    acceptedEvents: 1,
    rejectedEvents: 0,
  };
}

test("宠物始终使用无动画 idle 静态帧且没有情绪覆盖层", async ({ browser }) => {
  const harness = await createAppHarness(browser, { state: staticPetState });
  const page = await harness.openPage("pet");
  const sprite = page.locator(".pet-sprite");

  await expect(sprite).toHaveAttribute("data-animated", "false");
  await expect(sprite).toHaveAttribute("data-pet-state", "idle");
  await expect(page.getByTestId("pet-emotion-overlay")).toHaveCount(0);
});

test("运行、等待和完成事件不会改变静态宠物", async ({ browser }) => {
  const harness = await createAppHarness(browser, { state: staticPetState });
  const page = await harness.openPage("pet");
  const sprite = page.locator(".pet-sprite");

  for (const state of ["running", "waiting", "waving"]) {
    await harness.emitRuntimeUpdate(page, {
      currentState: { state },
      notifications: state === "waving" ? [completedNotification()] : [],
      attention:
        state === "waving"
          ? { id: "codex:thread-1:turn-1", kind: "completed", occurredAtMs: 100 }
          : null,
    });
    await expect(sprite).toHaveAttribute("data-animated", "false");
    await expect(sprite).toHaveAttribute("data-pet-state", "idle");
    await expect(page.getByTestId("pet-emotion-overlay")).toHaveCount(0);
  }
});

test("短按宠物调用 open_codex", async ({ browser }) => {
  const harness = await createAppHarness(browser, { state: staticPetState });
  const page = await harness.openPage("pet");

  await shortPress(page);

  await expect
    .poll(() => harness.calls.filter((call) => call.command === "open_codex"))
    .toHaveLength(1);
  expect(harness.calls).toContainEqual({
    command: "open_codex",
    args: {},
  });
});

test("双击宠物只调用一次 open_codex", async ({ browser }) => {
  const harness = await createAppHarness(browser, { state: staticPetState });
  const page = await harness.openPage("pet");

  await shortPress(page, 1);
  await shortPress(page, 2);

  await expect
    .poll(() => harness.calls.filter((call) => call.command === "open_codex"))
    .toHaveLength(1);
});

test("拖拽超过阈值不会调用 open_codex", async ({ browser }) => {
  const harness = await createAppHarness(browser, { state: staticPetState });
  const page = await harness.openPage("pet");

  await dragPet(page, 40);
  await page.waitForTimeout(100);

  expect(harness.calls.filter((call) => call.command === "open_codex")).toHaveLength(0);
});

test("完成提醒继续显示在宠物之前且提醒操作不会打开 Codex 主界面", async ({
  browser,
}) => {
  const harness = await createAppHarness(browser, {
    runtimeStatus: runtimeWithCompleted(),
    state: staticPetState,
  });
  const page = await harness.openPage("pet");
  const stackChildren = page.locator(".pet-window-stack > *");

  await expect(page.getByTestId("task-notification")).toBeVisible();
  await expect(stackChildren.first()).toHaveAttribute(
    "data-testid",
    "task-notifications",
  );
  await page.getByTestId("task-notification").click();

  expect(harness.calls).toContainEqual({
    command: "open_task_notification",
    args: { id: "codex:thread-1:turn-1" },
  });
  expect(harness.calls.filter((call) => call.command === "open_codex")).toHaveLength(0);
});

test("静态模式不播放点击、拖动落地或 Agent 状态音效", async ({ browser }) => {
  const harness = await createAppHarness(browser, { state: staticPetState });
  const page = await harness.openPage("pet");
  const spriteFrame = page.locator(".pet-sprite-frame");
  await harness.clearPlayedSoundUrls(page);

  await shortPress(page, 1);
  await spriteFrame.dispatchEvent("click", { button: 0, detail: 1 });
  await dragPet(page, 250, 2);
  await harness.emitRuntimeUpdate(page, {
    currentState: { state: "waving" },
    notifications: [completedNotification()],
    attention: {
      id: "codex:thread-1:turn-1",
      kind: "completed",
      occurredAtMs: 100,
    },
  });
  await page.waitForTimeout(100);

  expect(await harness.playedSoundUrls(page)).toEqual([]);
});
