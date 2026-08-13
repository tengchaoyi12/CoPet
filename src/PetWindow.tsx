import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { X } from "lucide-react";
import type {
  CSSProperties,
  MouseEvent as ReactMouseEvent,
} from "react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { ErrorView, LoadingView } from "./components/AppShell";
import { PetSprite } from "./components/PetSprite";
import { TaskNotifications } from "./components/TaskNotifications";
import { Toaster } from "./components/ui/sonner";
import { useLayeredPetState } from "./hooks/useLayeredPetState";
import {
  useAgentMessages,
  useLoadState,
  useLocale,
  useAgentMessageVisible,
  usePetWindowSize,
  useSelectedPet,
} from "./hooks/useAppStore";
import { useTaskNotifications } from "./hooks/useTaskNotifications";
import {
  dismissAgentMessage,
  openCodex,
  openSettingsWindow,
  reloadAppStore,
  setAgentMessageVisible as setAgentMessageVisibleCommand,
  setPetVisible as setPetVisibleCommand,
} from "./lib/appCommands";
import { usePetContextMenu } from "./hooks/usePetContextMenu";
import { createTranslator } from "./lib/i18n";
import type { AgentMessage, PetWindowSize } from "./lib/appTypes";
import type { ComposedView } from "./lib/petAnimation";
import {
  defaultPetWindowSize,
  maxPetWindowLogicalDimensions,
  petWindowPadding,
  petWindowScaleFromSize,
  petWindowSizeSliderDragEvent,
  petWindowSizeSliderResizeDelayMs,
  petWindowStackContentSize,
  resizeCurrentPetWindowFromCenter,
  resizeCurrentPetWindowToResetPosition,
} from "./lib/petWindowUi";
import type { PetWindowSizeSliderDragPayload } from "./lib/petWindowUi";
import { agentIconUrl } from "./lib/agentIcons";

const staticPetView: ComposedView = {
  bodySpriteRow: "idle",
  emotionOverlay: null,
  dragging: false,
};

const setAgentMessageVisible = async (visible: boolean) => {
  const { errorMessage } = await setAgentMessageVisibleCommand(visible);
  if (errorMessage) toast.error(errorMessage);
};

const runContextMenuCommand = async (
  command: Promise<{ errorMessage: string | null }>,
) => {
  const { errorMessage } = await command;
  if (errorMessage) toast.error(errorMessage);
};

export function PetWindow() {
  const loadState = useLoadState();
  const agentMessages = useAgentMessages();
  const selectedPet = useSelectedPet();
  const taskNotifications = useTaskNotifications();
  const agentMessageVisible = useAgentMessageVisible();
  const petWindowSize = usePetWindowSize();
  const locale = useLocale();
  const t = createTranslator(locale);

  // macOS NSPanel does not always deliver contextmenu to the webview; long-press
  // is a fallback path that opens the same native menu below the pet.
  // We require __TAURI__ to be present so this path does not activate under
  // bare Playwright (which may report a Mac UA on Apple-silicon CI hosts).
  const isMac =
    typeof navigator !== "undefined" &&
    /Mac/i.test(navigator.userAgent) &&
    typeof (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !==
      "undefined";
  const initialContentResizeAnchorReleaseMs = 250;
  const openPetContextMenuRef = useRef<() => void>(() => undefined);
  const { bindInput, bindMotion, notifyFailed } = useLayeredPetState({
    onLongPress: isMac ? () => openPetContextMenuRef.current() : undefined,
    onPrimaryAction: () => {
      void openCodex().then(({ errorMessage }) => {
        if (errorMessage) {
          toast.error(errorMessage);
        }
      });
    },
  });
  const displayedTaskNotifications = taskNotifications.notifications;
  const taskNotificationAgents = new Set(
    displayedTaskNotifications.map((notification) => notification.agent),
  );
  const displayedAgentMessages = agentMessages.filter(
    (message) => !taskNotificationAgents.has(message.agent),
  );

  const stackRef = useRef<HTMLDivElement | null>(null);
  const sliderDraggingRef = useRef(false);
  const initialContentResizePendingRef = useRef(true);
  const initialContentResizeReleaseTimerRef = useRef<number | null>(null);
  const resizeTimerRef = useRef<number | null>(null);
  const sliderScaleReleaseTimerRef = useRef<number | null>(null);
  const petWindowSizeRef = useRef(defaultPetWindowSize);
  const displayedPetScaleRef = useRef(petWindowScaleFromSize(defaultPetWindowSize));
  const [viewportSize, setViewportSize] = useState(() => ({
    height: window.innerHeight,
    width: window.innerWidth,
  }));
  const [sliderScaleLock, setSliderScaleLock] = useState<{
    startScale: number;
    startSize: PetWindowSize;
  } | null>(null);

  const { openMenu: openPetContextMenu } = usePetContextMenu({
    labels: {
      messages: agentMessageVisible
        ? t("contextMenuHideMessages")
        : t("contextMenuShowMessages"),
      openSettings: t("contextMenuOpenSettings"),
      hidePet: t("contextMenuHidePet"),
    },
    onToggleMessages: () => {
      void setAgentMessageVisible(!agentMessageVisible);
    },
    onOpenSettings: () => {
      void runContextMenuCommand(openSettingsWindow());
    },
    onHidePet: () => {
      void runContextMenuCommand(setPetVisibleCommand(false));
    },
    onPopupFailed: notifyFailed,
  });
  const configuredPetScale = petWindowScaleFromSize(petWindowSize);
  const fitPetScale =
    selectedPet &&
    displayedAgentMessages.length === 0 &&
    displayedTaskNotifications.length === 0
      ? Math.max(
          0.01,
          Math.min(
            configuredPetScale,
            (viewportSize.width - petWindowPadding) / selectedPet.frameWidth,
            (viewportSize.height - petWindowPadding) / selectedPet.frameHeight,
          ),
        )
      : configuredPetScale;
  const petScale =
    sliderScaleLock && petWindowSize === sliderScaleLock.startSize
      ? sliderScaleLock.startScale
      : fitPetScale;

  const resizeToStack = (anchor: "center" | "resetPosition" = "center") => {
    if (sliderDraggingRef.current || !stackRef.current) {
      return Promise.resolve();
    }
    const nextSize = petWindowStackContentSize(stackRef.current);
    return anchor === "resetPosition"
      ? resizeCurrentPetWindowToResetPosition(nextSize)
      : resizeCurrentPetWindowFromCenter(nextSize);
  };

  const petMenuAnchor = () =>
    stackRef.current?.querySelector<HTMLElement>(".pet-sprite-frame") ?? stackRef.current;

  const openPetContextMenuBelowPet = () => {
    void openPetContextMenu(petMenuAnchor());
  };

  useEffect(() => {
    petWindowSizeRef.current = petWindowSize;
    displayedPetScaleRef.current = petScale;
  }, [petScale, petWindowSize]);

  useEffect(() => {
    const animationFrame = window.requestAnimationFrame(() => {
      const anchor =
        initialContentResizePendingRef.current && stackRef.current
          ? "resetPosition"
          : "center";
      if (anchor === "resetPosition" && initialContentResizeReleaseTimerRef.current === null) {
        initialContentResizeReleaseTimerRef.current = window.setTimeout(() => {
          initialContentResizePendingRef.current = false;
          initialContentResizeReleaseTimerRef.current = null;
        }, initialContentResizeAnchorReleaseMs);
      }
      void resizeToStack(anchor);
    });
    return () => window.cancelAnimationFrame(animationFrame);
  }, [
    selectedPet?.id,
    petScale,
    displayedAgentMessages.length,
    displayedTaskNotifications.length,
    viewportSize.height,
    viewportSize.width,
  ]);

  useEffect(() => {
    openPetContextMenuRef.current = () => {
      openPetContextMenuBelowPet();
    };
  }, [openPetContextMenu]);

  useEffect(() => {
    return () => {
      if (initialContentResizeReleaseTimerRef.current !== null) {
        window.clearTimeout(initialContentResizeReleaseTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    const updateViewportSize = () => {
      setViewportSize({ height: window.innerHeight, width: window.innerWidth });
    };
    window.addEventListener("resize", updateViewportSize);
    return () => window.removeEventListener("resize", updateViewportSize);
  }, []);

  useEffect(() => {
    let unlistenDrag: (() => void) | undefined;
    let disposed = false;

    void listen<PetWindowSizeSliderDragPayload>(petWindowSizeSliderDragEvent, (event) => {
      if (event.payload.phase === "begin") {
        sliderDraggingRef.current = true;
        if (resizeTimerRef.current !== null) {
          window.clearTimeout(resizeTimerRef.current);
          resizeTimerRef.current = null;
        }
        if (sliderScaleReleaseTimerRef.current !== null) {
          window.clearTimeout(sliderScaleReleaseTimerRef.current);
          sliderScaleReleaseTimerRef.current = null;
        }
        return;
      }

      if (event.payload.phase === "start") {
        sliderDraggingRef.current = true;
        setSliderScaleLock({
          startScale: displayedPetScaleRef.current,
          startSize: petWindowSizeRef.current,
        });
        void resizeCurrentPetWindowFromCenter(maxPetWindowLogicalDimensions());
        return;
      }

      sliderDraggingRef.current = false;
      setSliderScaleLock({
        startScale: displayedPetScaleRef.current,
        startSize: petWindowSizeRef.current,
      });
      resizeTimerRef.current = window.setTimeout(() => {
        resizeTimerRef.current = null;
        void resizeToStack().finally(() => {
          sliderScaleReleaseTimerRef.current = window.setTimeout(() => {
            sliderScaleReleaseTimerRef.current = null;
            setSliderScaleLock(null);
          }, 50);
        });
      }, petWindowSizeSliderResizeDelayMs);
    }).then((cleanup) => {
      if (disposed) {
        cleanup();
      } else {
        unlistenDrag = cleanup;
      }
    });

    return () => {
      disposed = true;
      unlistenDrag?.();
      if (resizeTimerRef.current !== null) {
        window.clearTimeout(resizeTimerRef.current);
      }
      if (sliderScaleReleaseTimerRef.current !== null) {
        window.clearTimeout(sliderScaleReleaseTimerRef.current);
      }
    };
  }, []);

  const handleContextMenu = (event: ReactMouseEvent<HTMLElement>) => {
    event.preventDefault();
    openPetContextMenuBelowPet();
  };

  if (loadState.status === "loading") {
    return <LoadingView />;
  }

  if (loadState.status === "error") {
    return (
      <ErrorView
        message={loadState.error ?? "Unknown error"}
        onRetry={() => void reloadAppStore()}
        retryLabel={t("retry")}
      />
    );
  }

  const motionHandlers = bindMotion();

  return (
    <>
      <main
        className="pet-window"
        data-tauri-drag-region
        onContextMenu={handleContextMenu}
      >
        <div
          className="pet-window-stack"
          data-fit-pet={
            displayedAgentMessages.length === 0 &&
            displayedTaskNotifications.length === 0
          }
          ref={stackRef}
          style={
            selectedPet
              ? ({
                  "--pet-agent-message-min-width": `${Math.ceil(
                    selectedPet.frameWidth * petScale + 12,
                  )}px`,
                } as CSSProperties)
              : undefined
          }
        >
          {displayedTaskNotifications.length > 0 ? (
            <TaskNotifications
              actionNowMs={taskNotifications.actionNowMs}
              busyActionIds={taskNotifications.busyActionIds}
              notifications={displayedTaskNotifications}
              onAllowOnce={(actionId) =>
                void taskNotifications.allowOnce(actionId)
              }
              onContinueOnce={(actionId) =>
                void taskNotifications.continueOnce(actionId)
              }
              onDismiss={(id) => void taskNotifications.dismiss(id)}
              onFallbackAndOpen={(notificationId, actionId) =>
                void taskNotifications.fallbackAndOpen(notificationId, actionId)
              }
              onOpen={(id) => void taskNotifications.open(id)}
            />
          ) : null}
          {displayedAgentMessages.length > 0 ? (
            <AgentMessages
              dismissLabel={t("dismiss")}
              messages={displayedAgentMessages}
              onDismiss={dismissAgentMessage}
            />
          ) : null}
          {selectedPet ? (
            <PetSprite
              pet={selectedPet}
              composed={staticPetView}
              scale={petScale}
              animated={false}
              inputHandlers={bindInput()}
              motionHandlers={motionHandlers}
            />
          ) : null}
        </div>
      </main>
      <Toaster position="bottom-center" />
    </>
  );
}

function AgentMessages({
  dismissLabel,
  messages,
  onDismiss,
}: {
  dismissLabel: string;
  messages: AgentMessage[];
  onDismiss: (agentId: string) => void;
}) {
  return (
    <div className="pet-agent-messages" data-testid="pet-agent-messages">
      {messages.map((message) => {
        const iconUrl = agentIconUrl(message.agent);
        return (
        <div
          className="pet-agent-message"
          data-testid="pet-agent-message"
          key={`${message.agent}:${message.updatedAtMs}:${message.text}`}
        >
          {iconUrl ? (
            <img
              alt={message.displayName}
              className="pet-agent-icon"
              src={iconUrl}
            />
          ) : null}
          <span className="pet-agent-text">{message.text}</span>
          <button
            aria-label={dismissLabel}
            className="pet-agent-message-dismiss"
            onClick={(event) => {
              event.stopPropagation();
              onDismiss(message.agent);
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
