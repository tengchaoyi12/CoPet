import { useCallback, useMemo, useState } from "react";
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
      notifications: visible ? notifications : [],
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
    [busyActionIds, notifications, runAction, visible],
  );
}
