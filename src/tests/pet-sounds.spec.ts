import { expect, test } from "@playwright/test";

import { copetWithSounds, createAppHarness } from "./app-harness";

function soundState(enableClickSounds: boolean) {
  return {
    currentPetId: copetWithSounds.id,
    pets: [copetWithSounds],
    onboardingComplete: false,
    agentMessageVisible: true,
    petInteractions: {
      enableClickSounds,
      cooldownStyle: "normal" as const,
      enableStartupAnimation: true,
    },
  };
}

test("static pet stays silent even when sound preferences are enabled", async ({
  browser,
}) => {
  const harness = await createAppHarness(browser, { state: soundState(true) });
  const page = await harness.openPage("pet");

  await page.locator(".pet-sprite-frame").dispatchEvent("click", {
    button: 0,
    detail: 1,
  });
  await harness.emitRuntimeUpdate(page, {
    currentState: { state: "running" },
    messages: [
      { agent: "codex", displayName: "Codex", text: "editing", updatedAtMs: 1 },
    ],
  });
  await page.waitForTimeout(100);

  expect(await harness.playedSoundUrls(page)).toEqual([]);
});

test("static pet also stays silent when sound preferences are disabled", async ({
  browser,
}) => {
  const harness = await createAppHarness(browser, { state: soundState(false) });
  const page = await harness.openPage("pet");

  await page.locator(".pet-sprite-frame").dispatchEvent("click", {
    button: 0,
    detail: 1,
  });
  await page.waitForTimeout(100);

  await expect(page.locator(".pet-sprite")).toHaveAttribute("data-animated", "false");
  expect(await harness.playedSoundUrls(page)).toEqual([]);
});
