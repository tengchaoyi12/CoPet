import { X } from "lucide-react";

import type { TaskNotification } from "../lib/appTypes";
import { agentIconUrl } from "../lib/agentIcons";
import { createTranslator } from "../lib/i18n";
import { useLocale } from "../hooks/useAppStore";

export function TaskNotifications({
  actionNowMs,
  busyActionIds,
  notifications,
  onAllowOnce,
  onContinueOnce,
  onDismiss,
  onFallbackAndOpen,
  onOpen,
}: {
  actionNowMs: number;
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
        const actionPending = Boolean(
          action?.state === "pending" && action.expiresAtMs > actionNowMs,
        );
        const actionBusy = action ? busyActionIds.has(action.id) : false;
        const showQuickAction = Boolean(
          actionPending && action?.quickActionAllowed,
        );
        const openNotification = () => {
          if (action && actionPending) {
            onFallbackAndOpen(notification.id, action.id);
          } else {
            onOpen(notification.id);
          }
        };
        return (
          <div
            className="pet-task-notification"
            data-status={notification.status}
            data-testid="task-notification"
            key={notification.id}
          >
            <button
              className="pet-task-notification-body"
              onClick={openNotification}
              type="button"
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
                {action?.requestedAction &&
                action.requestedAction !== notification.summary ? (
                  <span
                    className="pet-task-notification-summary"
                    title={action.requestedAction}
                  >
                    {action.requestedAction}
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
              </span>
            </button>
            {action ? (
              <span className="pet-task-action-buttons">
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
                  onClick={openNotification}
                  type="button"
                >
                  {t("taskActionHandleInCodex")}
                </button>
              </span>
            ) : null}
            <button
              aria-label={t("dismiss")}
              className="pet-agent-message-dismiss"
              onClick={(event) => {
                event.stopPropagation();
                onDismiss(notification.id);
              }}
              onKeyDown={(event) => event.stopPropagation()}
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
