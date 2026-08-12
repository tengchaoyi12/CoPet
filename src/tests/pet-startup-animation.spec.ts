import { expect, test } from "@playwright/test";

import { copet, createAppHarness } from "./app-harness";

test("PetWindow does not run startup animation while retained infrastructure remains available", async ({
  browser,
}) => {
  const harness = await createAppHarness(browser, {
    reducedMotion: "no-preference",
    runtimeStatus: {
      port: 8765,
      endpoint: "http://127.0.0.1:8765/v1/events",
      currentState: { state: "running", sinceMs: 100, idleAfterMs: 1600 },
      messages: [
        {
          agent: "codex",
          displayName: "Codex",
          text: "Reading App.tsx",
          updatedAtMs: 100,
        },
      ],
      notifications: [],
      attention: null,
      acceptedEvents: 1,
      rejectedEvents: 0,
    },
    state: {
      currentPetId: copet.id,
      pets: [copet],
      onboardingComplete: false,
      petInteractions: {
        enableClickSounds: true,
        cooldownStyle: "normal",
        enableStartupAnimation: true,
      },
    },
  });
  const page = await harness.openPage("pet");

  await expect(page.getByTestId("pet-agent-message")).toHaveText("Reading App.tsx");
  await expect(page.locator(".pet-sprite")).toHaveAttribute("data-pet-state", "idle");
  await expect(page.locator(".pet-sprite")).toHaveAttribute("data-animated", "false");
  expect(harness.invocations("run_pet_startup_window_animation")).toEqual([]);
  expect(await harness.playedSoundUrls(page)).toEqual([]);
});
