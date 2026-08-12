import { PhysicalPosition } from "@tauri-apps/api/dpi";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { PointerEvent as ReactPointerEvent } from "react";
import { useCallback, useEffect, useRef, useState } from "react";

import type { MotionState } from "../lib/petAnimation";
import {
  nativeMoveJitterThreshold,
  pointerMoveJitterThreshold,
} from "../lib/petWindowUi";

const DRAG_LAND_THRESHOLD_PX = 200;
const PRIMARY_ACTION_DEDUP_MS = 500;
const PRIMARY_ACTION_MAX_HOLD_MS = 800;

export type MotionHandlers = {
  onPointerDown: (event: ReactPointerEvent<HTMLElement>) => void;
};

export type UseMotionStateResult = {
  state: MotionState;
  handlers: MotionHandlers;
  notifyActivity: () => void;
  lastActivityAtMs: number;
};

const isWindows = /windows/i.test(navigator.userAgent);

export function useMotionState(opts?: {
  onDragLand?: () => void;
  onPrimaryAction?: () => void;
}): UseMotionStateResult {
  const [state, setState] = useState<MotionState>({ kind: "anchored" });
  const [lastActivityAtMs, setLastActivityAtMs] = useState(() => Date.now());
  const dragPointerRef = useRef<{ lastClientX: number; lastClientY: number } | null>(null);
  const nativeDragRef = useRef<{ lastX: number | null; lastY: number | null }>({
    lastX: null,
    lastY: null,
  });
  // Windows-only: programmatic drag state
  const winDragRef = useRef<{
    baseX: number;
    baseY: number;
    accumX: number;
    accumY: number;
    lastScreenX: number;
    lastScreenY: number;
  } | null>(null);
  const dragDistanceRef = useRef(0);
  const pointerSequenceActiveRef = useRef(false);
  const pointerSequenceStartedAtMsRef = useRef(0);
  const lastPrimaryActionAtMsRef = useRef(Number.NEGATIVE_INFINITY);
  const rafRef = useRef(0);
  const onDragLandRef = useRef(opts?.onDragLand);
  const onPrimaryActionRef = useRef(opts?.onPrimaryAction);

  useEffect(() => {
    onDragLandRef.current = opts?.onDragLand;
  }, [opts?.onDragLand]);

  useEffect(() => {
    onPrimaryActionRef.current = opts?.onPrimaryAction;
  }, [opts?.onPrimaryAction]);

  const notifyActivity = useCallback(() => {
    setLastActivityAtMs(Date.now());
  }, []);

  const finishPointerSequence = useCallback((allowPrimaryAction: boolean) => {
    if (!pointerSequenceActiveRef.current) {
      return;
    }
    pointerSequenceActiveRef.current = false;
    const total = dragDistanceRef.current;
    const heldForMs = Date.now() - pointerSequenceStartedAtMsRef.current;
    dragDistanceRef.current = 0;
    dragPointerRef.current = null;
    nativeDragRef.current = { lastX: null, lastY: null };
    winDragRef.current = null;
    if (rafRef.current) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = 0;
    }
    setState({ kind: "anchored" });
    if (total >= DRAG_LAND_THRESHOLD_PX) {
      onDragLandRef.current?.();
    }
    if (
      allowPrimaryAction &&
      total < pointerMoveJitterThreshold &&
      heldForMs < PRIMARY_ACTION_MAX_HOLD_MS
    ) {
      const now = Date.now();
      if (now - lastPrimaryActionAtMsRef.current >= PRIMARY_ACTION_DEDUP_MS) {
        lastPrimaryActionAtMsRef.current = now;
        onPrimaryActionRef.current?.();
      }
    }
  }, []);

  // ── macOS: native startDragging + tauri://move listener ──
  const onPointerDownMac = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      if (event.button !== 0) return;
      pointerSequenceActiveRef.current = true;
      pointerSequenceStartedAtMsRef.current = Date.now();
      dragPointerRef.current = {
        lastClientX: event.clientX,
        lastClientY: event.clientY,
      };
      nativeDragRef.current = { lastX: null, lastY: null };
      dragDistanceRef.current = 0;
      notifyActivity();
      void getCurrentWebviewWindow().startDragging();
    },
    [notifyActivity],
  );

  // ── Windows: programmatic setPosition via rAF ──
  const onPointerDownWin = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      if (event.button !== 0) return;
      pointerSequenceActiveRef.current = true;
      pointerSequenceStartedAtMsRef.current = Date.now();
      (event.target as HTMLElement).setPointerCapture(event.pointerId);
      void getCurrentWebviewWindow().outerPosition().then((pos) => {
        if (!pointerSequenceActiveRef.current) {
          return;
        }
        winDragRef.current = {
          baseX: pos.x,
          baseY: pos.y,
          accumX: 0,
          accumY: 0,
          lastScreenX: event.screenX,
          lastScreenY: event.screenY,
        };
      });
      dragDistanceRef.current = 0;
      setState({ kind: "dragging", direction: "still" });
      notifyActivity();
    },
    [notifyActivity],
  );

  const onPointerDown = isWindows ? onPointerDownWin : onPointerDownMac;

  // ── macOS: pointermove for animation direction ──
  useEffect(() => {
    if (isWindows) return;

    const handlePointerMove = (event: PointerEvent) => {
      const pointer = dragPointerRef.current;
      if (!pointer) return;
      const deltaX = event.clientX - pointer.lastClientX;
      const deltaY = event.clientY - pointer.lastClientY;
      pointer.lastClientX = event.clientX;
      pointer.lastClientY = event.clientY;
      dragDistanceRef.current += Math.hypot(deltaX, deltaY);
      if (
        dragDistanceRef.current >= pointerMoveJitterThreshold &&
        deltaX !== 0
      ) {
        setState({ kind: "dragging", direction: deltaX > 0 ? "right" : "left" });
      }
    };

    const endWithPrimaryAction = () => finishPointerSequence(true);
    const cancel = () => finishPointerSequence(false);

    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", endWithPrimaryAction);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("blur", cancel);
    return () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", endWithPrimaryAction);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("blur", cancel);
    };
  }, [finishPointerSequence]);

  // ── macOS: tauri://move fallback ──
  useEffect(() => {
    if (isWindows) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void getCurrentWebviewWindow()
      .listen<{ x: number; y: number }>("tauri://move", (event) => {
        if (!dragPointerRef.current) {
          nativeDragRef.current = { lastX: null, lastY: null };
          return;
        }
        const currentX = event.payload.x;
        const currentY = event.payload.y;
        const previousX = nativeDragRef.current.lastX;
        const previousY = nativeDragRef.current.lastY;
        nativeDragRef.current.lastX = currentX;
        nativeDragRef.current.lastY = currentY;
        if (previousX === null || previousY === null) return;
        const deltaX = currentX - previousX;
        const deltaY = currentY - previousY;
        dragDistanceRef.current += Math.hypot(deltaX, deltaY);
        if (
          dragDistanceRef.current >= nativeMoveJitterThreshold &&
          deltaX !== 0
        ) {
          setState({ kind: "dragging", direction: deltaX > 0 ? "right" : "left" });
        }
      })
      .then((cleanup) => {
        if (cancelled) cleanup();
        else unlisten = cleanup;
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // ── Windows: programmatic setPosition via rAF ──
  useEffect(() => {
    if (!isWindows) return;
    const win = getCurrentWebviewWindow();
    const flush = () => {
      rafRef.current = 0;
      const drag = winDragRef.current;
      if (!drag) return;
      const ax = drag.accumX;
      const ay = drag.accumY;
      if (ax === 0 && ay === 0) return;
      drag.accumX = 0;
      drag.accumY = 0;
      drag.baseX += ax;
      drag.baseY += ay;
      void win.setPosition(new PhysicalPosition(drag.baseX, drag.baseY));
    };

    const handlePointerMove = (event: PointerEvent) => {
      const drag = winDragRef.current;
      if (!drag) return;
      const scale = window.devicePixelRatio || 1;
      drag.accumX += event.movementX * scale;
      drag.accumY += event.movementY * scale;
      const deltaX = event.screenX - drag.lastScreenX;
      const deltaY = event.screenY - drag.lastScreenY;
      drag.lastScreenX = event.screenX;
      drag.lastScreenY = event.screenY;
      dragDistanceRef.current += Math.hypot(deltaX, deltaY);
      if (
        dragDistanceRef.current >= pointerMoveJitterThreshold &&
        deltaX !== 0
      ) {
        setState({ kind: "dragging", direction: deltaX > 0 ? "right" : "left" });
      }
      if (!rafRef.current) rafRef.current = requestAnimationFrame(flush);
    };

    const endWithPrimaryAction = () => finishPointerSequence(true);
    const cancel = () => finishPointerSequence(false);

    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", endWithPrimaryAction);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("blur", cancel);
    return () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", endWithPrimaryAction);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("blur", cancel);
    };
  }, [finishPointerSequence]);

  return {
    state,
    handlers: { onPointerDown },
    notifyActivity,
    lastActivityAtMs,
  };
}
