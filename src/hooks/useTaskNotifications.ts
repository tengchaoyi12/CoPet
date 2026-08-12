import { useMemo } from "react";
import { toast } from "sonner";

import {
  dismissTaskNotification,
  openTaskNotification,
} from "../lib/appCommands";
import { useAppSlice } from "./useAppStore";

export function useTaskNotifications() {
  const notifications = useAppSlice((state) => state.taskNotifications);
  const visible = useAppSlice(
    (state) => state.appState?.agentMessageVisible ?? true,
  );

  return useMemo(
    () => ({
      notifications: visible ? notifications : [],
      open: async (id: string) => {
        const result = await openTaskNotification(id);
        if (result.errorMessage) toast.error(result.errorMessage);
      },
      dismiss: async (id: string) => {
        const result = await dismissTaskNotification(id);
        if (result.errorMessage) toast.error(result.errorMessage);
      },
    }),
    [notifications, visible],
  );
}
