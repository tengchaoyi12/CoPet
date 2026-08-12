import { X } from "lucide-react";

import type { TaskNotification } from "../lib/appTypes";
import { agentIconUrl } from "../lib/agentIcons";
import { createTranslator } from "../lib/i18n";
import { useLocale } from "../hooks/useAppStore";

export function TaskNotifications({
  notifications,
  onDismiss,
  onOpen,
}: {
  notifications: TaskNotification[];
  onDismiss: (id: string) => void;
  onOpen: (id: string) => void;
}) {
  const t = createTranslator(useLocale());

  return (
    <div className="pet-task-notifications" data-testid="task-notifications">
      {notifications.map((notification) => {
        const iconUrl = agentIconUrl(notification.agent);
        return (
          <div
            className="pet-task-notification"
            data-status={notification.status}
            data-testid="task-notification"
            key={notification.id}
            onClick={() => onOpen(notification.id)}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                onOpen(notification.id);
              }
            }}
            role="button"
            tabIndex={0}
          >
            {iconUrl ? (
              <img
                alt={notification.displayName}
                className="pet-agent-icon"
                src={iconUrl}
              />
            ) : null}
            <span className="pet-task-notification-copy">
              <span className="pet-task-notification-status">
                {t(`taskNotification${capitalize(notification.status)}`)}
              </span>
              {notification.title ? (
                <span className="pet-task-notification-title">
                  {notification.title}
                </span>
              ) : null}
            </span>
            <button
              aria-label={t("dismiss")}
              className="pet-agent-message-dismiss"
              onClick={(event) => {
                event.stopPropagation();
                onDismiss(notification.id);
              }}
              type="button"
            >
              <X aria-hidden="true" />
            </button>
          </div>
        );
      })}
    </div>
  );
}

function capitalize(status: TaskNotification["status"]):
  | "Running"
  | "Waiting"
  | "Completed"
  | "Failed" {
  return `${status[0].toUpperCase()}${status.slice(1)}` as
    | "Running"
    | "Waiting"
    | "Completed"
    | "Failed";
}
