import { X } from "lucide-react";

import type { TaskNotification } from "../lib/appTypes";
import { agentIconUrl } from "../lib/agentIcons";
import { createTranslator } from "../lib/i18n";
import { useLocale } from "../hooks/useAppStore";

export function TaskNotifications({
  busyActionIds,
  notifications,
  onAllowOnce,
  onContinueOnce,
  onDismiss,
  onFallbackAndOpen,
  onOpen,
}: {
  busyActionIds: ReadonlySet<string>;
  notifications: TaskNotification[];
  onAllowOnce: (actionId: string) => void;
  onContinueOnce: (actionId: string) => void;
  onDismiss: (id: string) => void;
  onFallbackAndOpen: (notificationId: string, actionId: string) => void;
  onOpen: (id: string) => void;
}) {
  const t = createTranslator(useLocale());

  return (
    <div className="pet-task-notifications" data-testid="task-notifications">
      {notifications.map((notification) => {
        const iconUrl = agentIconUrl(notification.agent);
        const action = notification.action;
        const actionPending = action?.state === "pending";
        const actionBusy = action ? busyActionIds.has(action.id) : false;
        const showQuickAction = Boolean(
          actionPending && action?.quickActionAllowed,
        );
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
              {notification.summary ? (
                <span
                  className="pet-task-notification-summary"
                  title={notification.summary}
                >
                  {notification.summary}
                </span>
              ) : null}
              {action?.kind === "permission" ? (
                <span className="pet-task-action-details">
                  {action.toolName ? (
                    <span title={action.toolName}>
                      {t("taskActionTool")}: {action.toolName}
                    </span>
                  ) : null}
                  {action.command ? (
                    <code title={action.command}>{action.command}</code>
                  ) : null}
                  {action.cwd ? (
                    <span title={action.cwd}>
                      {t("taskActionWorkingDirectory")}: {action.cwd}
                    </span>
                  ) : null}
                  <strong>{t("taskActionAllowOnceScope")}</strong>
                </span>
              ) : null}
              {action ? (
                <span
                  className="pet-task-action-buttons"
                  onClick={(event) => event.stopPropagation()}
                  onKeyDown={(event) => event.stopPropagation()}
                >
                  {showQuickAction ? (
                    <button
                      className="pet-task-action-primary"
                      disabled={actionBusy}
                      onClick={() => {
                        if (action.kind === "continue") {
                          onContinueOnce(action.id);
                        } else {
                          onAllowOnce(action.id);
                        }
                      }}
                      type="button"
                    >
                      {action.kind === "continue"
                        ? t("taskActionContinue")
                        : t("taskActionAllowAndContinue")}
                    </button>
                  ) : null}
                  <button
                    className="pet-task-action-secondary"
                    disabled={actionBusy}
                    onClick={() => {
                      if (actionPending) {
                        onFallbackAndOpen(notification.id, action.id);
                      } else {
                        onOpen(notification.id);
                      }
                    }}
                    type="button"
                  >
                    {t("taskActionHandleInCodex")}
                  </button>
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
