import { expect, test } from "@playwright/test";

import { composeLayers } from "../lib/petAnimation";

test("composeLayers maps surprised to waving", () => {
  const view = composeLayers({
    base: { kind: "blink" },
    agent: { kind: "none" },
    input: { kind: "surprised" },
    motion: { kind: "anchored" },
    emotion: { kind: "none" },
  });
  expect(view.bodySpriteRow).toBe("waving");
});

test("composeLayers maps petted to jumping with heart", () => {
  const view = composeLayers({
    base: { kind: "blink" },
    agent: { kind: "none" },
    input: { kind: "petted" },
    motion: { kind: "anchored" },
    emotion: { kind: "heart" },
  });
  expect(view.bodySpriteRow).toBe("jumping");
  expect(view.emotionOverlay).toBe("heart");
});

test("composeLayers maps slow petting to waiting with heart", () => {
  const view = composeLayers({
    base: { kind: "blink" },
    agent: { kind: "none" },
    input: { kind: "pettedSlow" },
    motion: { kind: "anchored" },
    emotion: { kind: "heart" },
  });
  expect(view.bodySpriteRow).toBe("waiting");
  expect(view.emotionOverlay).toBe("heart");
});

test("composeLayers maps question mark overlay through", () => {
  const view = composeLayers({
    base: { kind: "blink" },
    agent: { kind: "none" },
    input: { kind: "surprised" },
    motion: { kind: "anchored" },
    emotion: { kind: "questionMark" },
  });
  expect(view.emotionOverlay).toBe("question-mark");
});

test("critical hurt state is not preempted by input", () => {
  const view = composeLayers({
    base: { kind: "blink" },
    agent: { kind: "hurt", agent: "claude" },
    input: { kind: "happy" },
    motion: { kind: "anchored" },
    emotion: { kind: "smoke" },
  });
  expect(view.bodySpriteRow).toBe("failed");
  expect(view.emotionOverlay).toBe("smoke");
});

test("critical approval state is not preempted by input", () => {
  const view = composeLayers({
    base: { kind: "blink" },
    agent: { kind: "awaitingApproval", agent: "claude" },
    input: { kind: "happy" },
    motion: { kind: "anchored" },
    emotion: { kind: "none" },
  });
  expect(view.bodySpriteRow).toBe("waiting");
  expect(view.emotionOverlay).toBe(null);
});

test("non-critical thinking can still be preempted inside retained layer infrastructure", () => {
  const view = composeLayers({
    base: { kind: "blink" },
    agent: { kind: "thinking", agent: "claude", phase: "processing" },
    input: { kind: "happy" },
    motion: { kind: "anchored" },
    emotion: { kind: "loadingBubble" },
  });
  expect(view.bodySpriteRow).toBe("jumping");
  expect(view.emotionOverlay).toBe("loading-bubble");
});

test("dragging still wins inside retained layer infrastructure", () => {
  const view = composeLayers({
    base: { kind: "blink" },
    agent: { kind: "hurt", agent: "claude" },
    input: { kind: "happy" },
    motion: { kind: "dragging", direction: "right" },
    emotion: { kind: "smoke" },
  });
  expect(view.bodySpriteRow).toBe("running-right");
  expect(view.dragging).toBe(true);
});
