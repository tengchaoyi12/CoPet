import { expect, test } from "@playwright/test";

import { copet, createAppHarness } from "./app-harness";

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
