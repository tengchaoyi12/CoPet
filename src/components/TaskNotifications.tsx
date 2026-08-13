import { useEffect, useId, useRef, useState } from "react";
import { ChevronDown, X } from "lucide-react";

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
  onLayoutChange,
  onOpen,
}: {
  actionNowMs: number;
  busyActionIds: ReadonlySet<string>;
  notifications: TaskNotification[];
  onAllowOnce: (actionId: string) => void;
  onContinueOnce: (actionId: string) => void;
  onDismiss: (id: string) => void;
  onFallbackAndOpen: (notificationId: string, actionId: string) => void;
  onLayoutChange: () => void;
  onOpen: (id: string) => void;
}) {
  const t = createTranslator(useLocale());
  const [expandedNotificationId, setExpandedNotificationId] = useState<
    string | null
  >(null);
  const detailsIdPrefix = useId();
  const expandedNotification = notifications.find(
    (notification) => notification.id === expandedNotificationId,
  );
  const expandedAction = expandedNotification?.action;
  const expandedDetailsKey =
    expandedNotification &&
    expandedAction?.state === "pending" &&
    expandedAction.expiresAtMs > actionNowMs &&
    expandedAction.quickActionAllowed
      ? expandedNotification.id
      : null;
  const previousExpandedDetailsKeyRef = useRef<string | null>(null);

  useEffect(() => {
    if (previousExpandedDetailsKeyRef.current === expandedDetailsKey) {
      return;
    }
    previousExpandedDetailsKeyRef.current = expandedDetailsKey;
    onLayoutChange();
  }, [expandedDetailsKey, onLayoutChange]);

  return (
    <div className="pet-task-notifications" data-testid="task-notifications">
      {notifications.map((notification, notificationIndex) => {
        const iconUrl = agentIconUrl(notification.agent);
        const action = notification.action;
        const actionPending = Boolean(
          action?.state === "pending" && action.expiresAtMs > actionNowMs,
        );
        const actionBusy = action ? busyActionIds.has(action.id) : false;
        const quickAction =
          actionPending && action?.quickActionAllowed ? action : null;
        const showQuickAction = Boolean(quickAction);
        const showDetails = Boolean(
          showQuickAction && expandedNotificationId === notification.id,
        );
        const detailsId = `${detailsIdPrefix}-task-action-details-${notificationIndex}`;
        const summary = actionSummary(notification);
        const showOpenAction =
          notification.status === "waiting" && !showQuickAction;
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
                  {notificationStatusLabel(notification, showQuickAction, t)}
                </span>
                {notification.title ? (
                  <span className="pet-task-notification-title">
                    {notification.title}
                  </span>
                ) : null}
                {summary ? (
                  <span
                    className={
                      action?.kind === "permission" && action.command
                        ? "pet-task-notification-summary pet-task-notification-command"
                        : "pet-task-notification-summary"
                    }
                    title={summary}
                  >
                    {summary}
                  </span>
                ) : null}
              </span>
            </button>
            {showQuickAction ? (
              <button
                aria-controls={detailsId}
                aria-expanded={showDetails}
                className="pet-task-action-details-toggle"
                onClick={() =>
                  setExpandedNotificationId((currentId) =>
                    currentId === notification.id ? null : notification.id,
                  )
                }
                type="button"
              >
                {t("taskActionDetails")}
                <ChevronDown aria-hidden="true" />
              </button>
            ) : null}
            {showQuickAction && action ? (
              <div
                className="pet-task-action-details"
                hidden={!showDetails}
                id={detailsId}
              >
                {action.requestedAction ? (
                  <span title={action.requestedAction}>
                    {action.requestedAction}
                  </span>
                ) : null}
                {action.kind === "permission" && action.toolName ? (
                  <span title={action.toolName}>
                    {t("taskActionTool")}: {action.toolName}
                  </span>
                ) : null}
                {action.kind === "permission" && action.command ? (
                  <code title={action.command}>{action.command}</code>
                ) : null}
                {action.kind === "permission" && action.cwd ? (
                  <span title={action.cwd}>
                    {t("taskActionWorkingDirectory")}: {action.cwd}
                  </span>
                ) : null}
                {action.kind === "permission" ? (
                  <strong>{t("taskActionAllowOnceScope")}</strong>
                ) : null}
                <button
                  className="pet-task-action-inline-open"
                  disabled={actionBusy}
                  onClick={openNotification}
                  type="button"
                >
                  {t("taskActionHandleInCodex")}
                </button>
              </div>
            ) : null}
            {showQuickAction || showOpenAction ? (
              <span className="pet-task-action-buttons">
                {quickAction ? (
                  <button
                    className="pet-task-action-primary"
                    disabled={actionBusy}
                    onClick={() => {
                      if (quickAction.kind === "continue") {
                        onContinueOnce(quickAction.id);
                      } else {
                        onAllowOnce(quickAction.id);
                      }
                    }}
                    type="button"
                  >
                    {quickAction.kind === "continue"
                      ? t("taskActionContinue")
                      : t("taskActionAllowOnce")}
                  </button>
                ) : null}
                {showOpenAction ? (
                  <button
                    className="pet-task-action-secondary"
                    disabled={actionBusy}
                    onClick={openNotification}
                    type="button"
                  >
                    {t("taskActionOpenCodex")}
                  </button>
                ) : null}
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

function actionSummary(notification: TaskNotification) {
  const action = notification.action;
  if (action?.kind === "permission") {
    return action.command || action.requestedAction || notification.summary;
  }
  if (action?.kind === "continue") {
    return action.requestedAction || notification.summary;
  }
  return notification.summary;
}

function notificationStatusLabel(
  notification: TaskNotification,
  showQuickAction: boolean,
  t: ReturnType<typeof createTranslator>,
) {
  if (showQuickAction && notification.action?.kind === "permission") {
    return t("taskActionWaitingPermission");
  }
  if (showQuickAction && notification.action?.kind === "continue") {
    return t("taskActionWaitingContinue");
  }
  if (notification.status === "waiting") {
    return t("taskActionNeedsReply");
  }
  return t(`taskNotification${capitalize(notification.status)}`);
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
