import { expect, test } from "@playwright/test";

import { copet, createAppHarness } from "./app-harness";

const zhState = {
  currentPetId: copet.id,
  locale: "zh-CN" as const,
  localePreference: "zh-CN" as const,
  pets: [copet],
  onboardingComplete: false,
};

test("登录自启动开关读取并更新系统真实状态", async ({ browser }) => {
  const harness = await createAppHarness(browser, {
    commandResults: { get_autostart_enabled: false },
    state: zhState,
  });
  const page = await harness.openPage("settings");

  await page.getByRole("tab", { name: "通用" }).click();
  const toggle = page.getByRole("switch", { name: "登录后自动启动" });
  await expect(toggle).toBeEnabled();
  await expect(toggle).not.toBeChecked();
  await toggle.click();

  expect(
    harness.calls.filter((call) => call.command === "set_autostart_enabled"),
  ).toEqual([
    {
      command: "set_autostart_enabled",
      args: { enabled: true },
    },
  ]);
  await expect(toggle).toBeChecked();
});

test("登录自启动设置失败时恢复原状态并显示错误", async ({ browser }) => {
  const harness = await createAppHarness(browser, {
    commandErrors: { set_autostart_enabled: "无法更新登录项" },
    commandResults: { get_autostart_enabled: false },
    state: zhState,
  });
  const page = await harness.openPage("settings");

  await page.getByRole("tab", { name: "通用" }).click();
  const toggle = page.getByRole("switch", { name: "登录后自动启动" });
  await expect(toggle).toBeEnabled();
  await toggle.click();

  await expect(toggle).not.toBeChecked();
  await expect(page.locator("[data-sonner-toast]")).toContainText(
    "无法更新登录项",
  );
});
