import { expect, test } from "@playwright/test";

import { createAppHarness, copet } from "./app-harness";

const petState = {
  currentPetId: copet.id,
  pets: [copet],
  onboardingComplete: false,
  locale: "en-US" as const,
  petInteractions: {
    enableClickSounds: true,
    cooldownStyle: "normal" as const,
    enableStartupAnimation: false,
  },
};

test("double-click no longer opens settings window", async ({ browser }) => {
  const harness = await createAppHarness(browser, { state: petState });
  const page = await harness.openPage("pet");

  await page.locator(".pet-sprite-frame").dispatchEvent("click", {
    button: 0,
    detail: 2,
  });
  await page.waitForTimeout(50);

  expect(harness.invocations("open_settings_window")).toHaveLength(0);
});

test("long press opens the native menu without opening Codex", async ({ browser }) => {
  const harness = await createAppHarness(browser, { state: petState });
  const page = await harness.openPage("pet");
  const spriteFrame = page.locator(".pet-sprite-frame");

  await spriteFrame.dispatchEvent("pointerdown", {
    button: 0,
    clientX: 50,
    clientY: 50,
    isPrimary: true,
    pointerId: 1,
    pointerType: "mouse",
  });
  await page.waitForTimeout(850);
  await page.evaluate(() => {
    window.dispatchEvent(new PointerEvent("pointerup", { pointerId: 1 }));
  });

  expect(harness.invocations("open_pet_context_menu")).toHaveLength(1);
  expect(harness.invocations("open_codex")).toHaveLength(0);
});

test("right-click opens the native pet context menu command", async ({ browser }) => {
  const harness = await createAppHarness(browser, { state: petState });
  const page = await harness.openPage("pet");
  const spriteFrame = page.locator(".pet-sprite-frame");
  await page.waitForTimeout(350);

  await spriteFrame.dispatchEvent("contextmenu", {
    bubbles: true,
    button: 2,
    clientX: 50,
    clientY: 50,
  });

  expect(harness.invocations("open_pet_context_menu")).toHaveLength(1);
  const box = await spriteFrame.evaluate((node) => {
    const rect = node.getBoundingClientRect();
    return {
      height: rect.height,
      width: rect.width,
      x: rect.left,
      y: rect.top,
    };
  });
  const args = harness.invocations("open_pet_context_menu")[0].args;
  expect(args?.labels).toEqual({
    messages: "Hide Messages",
    openSettings: "Open Settings",
    hidePet: "Hide Pet",
  });
  const position = args?.position as { x: number; y: number };
  expect(position.x).toBeGreaterThanOrEqual(box.x + box.width / 2 - 75);
  expect(position.x).toBeLessThanOrEqual(box.x + box.width / 2 - 73);
  expect(position.y).toBeGreaterThanOrEqual(box.y + box.height + 3);
  expect(position.y).toBeLessThanOrEqual(box.y + box.height + 5);
  await expect(page.getByTestId("pet-context-menu")).toHaveCount(0);
});

test("native menu failure leaves the pet static", async ({ browser }) => {
  const harness = await createAppHarness(browser, {
    nativePetContextMenuError: "popup failed",
    state: petState,
  });
  const page = await harness.openPage("pet");
  const spriteFrame = page.locator(".pet-sprite-frame");
  const sprite = page.locator(".pet-sprite");

  await spriteFrame.dispatchEvent("contextmenu", {
    bubbles: true,
    button: 2,
    clientX: 50,
    clientY: 50,
  });

  await expect(sprite).toHaveAttribute("data-animated", "false");
  await expect(sprite).toHaveAttribute("data-pet-state", "idle");
  await expect(page.getByTestId("pet-emotion-overlay")).toHaveCount(0);
  await expect(page.getByTestId("pet-context-menu")).toHaveCount(0);
});

test("native pet context menu action events run pet commands", async ({ browser }) => {
  const harness = await createAppHarness(browser, {
    state: { ...petState, agentMessageVisible: true },
  });
  await harness.openPage("pet");

  await harness.emitPetContextMenuAction("toggleMessages");
  await expect.poll(() => harness.state().agentMessageVisible).toBe(false);
  expect(harness.invocations("set_agent_message_visible").at(-1)?.args).toEqual({
    visible: false,
  });

  await harness.emitPetContextMenuAction("openSettings");
  await expect
    .poll(() => harness.invocations("open_settings_window").length)
    .toBe(1);

  await harness.emitPetContextMenuAction("hidePet");
  await expect
    .poll(() => harness.invocations("toggle_pet_window_visibility").length)
    .toBe(1);
});
