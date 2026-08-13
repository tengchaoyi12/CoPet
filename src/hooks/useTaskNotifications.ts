import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";

import {
  dismissTaskNotification,
  openTaskNotification,
  resolveTaskAction,
} from "../lib/appCommands";
import type { TaskActionDecision } from "../lib/appTypes";
import { useAppSlice } from "./useAppStore";

export function useTaskNotifications() {
  const notifications = useAppSlice((state) => state.taskNotifications);
  const visible = useAppSlice(
    (state) => state.appState?.agentMessageVisible ?? true,
  );
  const [busyActionIds, setBusyActionIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [actionNowMs, setActionNowMs] = useState(() => Date.now());

  useEffect(() => {
    const nextExpiry = notifications.reduce<number | null>((earliest, notification) => {
      const action = notification.action;
      if (!action || action.state !== "pending" || action.expiresAtMs <= actionNowMs) {
        return earliest;
      }
      return earliest === null ? action.expiresAtMs : Math.min(earliest, action.expiresAtMs);
    }, null);
    if (nextExpiry === null) return;
    const timer = window.setTimeout(
      () => setActionNowMs(Date.now()),
      Math.max(0, nextExpiry - Date.now() + 1),
    );
    return () => window.clearTimeout(timer);
  }, [actionNowMs, notifications]);

  const runAction = useCallback(
    async (id: string, decision: TaskActionDecision) => {
      setBusyActionIds((current) => new Set(current).add(id));
      try {
        const result = await resolveTaskAction(id, decision);
        if (result.errorMessage) toast.error(result.errorMessage);
        return result;
      } finally {
        setBusyActionIds((current) => {
          const next = new Set(current);
          next.delete(id);
          return next;
        });
      }
    },
    [],
  );

  return useMemo(
    () => ({
      notifications: visible
        ? notifications.filter((notification) => notification.unread)
        : [],
      actionNowMs,
      busyActionIds,
      open: async (id: string) => {
        const result = await openTaskNotification(id);
        if (result.errorMessage) toast.error(result.errorMessage);
      },
      dismiss: async (id: string) => {
        const result = await dismissTaskNotification(id);
        if (result.errorMessage) toast.error(result.errorMessage);
      },
      continueOnce: (actionId: string) => runAction(actionId, "continueOnce"),
      allowOnce: (actionId: string) => runAction(actionId, "allowOnce"),
      fallbackAndOpen: async (notificationId: string, actionId: string) => {
        const fallback = await runAction(actionId, "fallback");
        if (fallback.errorMessage) return;
        const opened = await openTaskNotification(notificationId);
        if (opened.errorMessage) toast.error(opened.errorMessage);
      },
    }),
    [actionNowMs, busyActionIds, notifications, runAction, visible],
  );
}
