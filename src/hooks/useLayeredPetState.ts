import { useMemo, useRef } from "react";

import {
  useAgentMessages,
  useAppSlice,
  usePetState,
} from "./useAppStore";
import { useAgentState } from "./useAgentState";
import { useBaseState } from "./useBaseState";
import { useEmotionState } from "./useEmotionState";
import { useInteractionState } from "./useInteractionState";
import { useMotionState } from "./useMotionState";
import type { InteractionSoundKey } from "./usePetSounds";
import { composeLayers } from "../lib/petAnimation";
import type {
  ComposedView,
  InputState,
  MotionState,
  PetLayers,
} from "../lib/petAnimation";
import type { InteractionHandlers } from "./useInteractionState";
import type { MotionHandlers } from "./useMotionState";
import type { CooldownStyle } from "../lib/appTypes";
import type { TaskAttention } from "../lib/appTypes";

export type UseLayeredPetStateResult = {
  layers: PetLayers;
  composed: ComposedView;
  bindInput: () => InteractionHandlers;
  bindMotion: () => MotionHandlers;
  notifyFailed: () => void;
};

export function useLayeredPetState(opts?: {
  onLongPress?: (origin: { x: number; y: number }) => void;
  onInteractionSound?: (kind: InteractionSoundKey) => void;
  attention?: TaskAttention | null;
}): UseLayeredPetStateResult {
  const petState = usePetState();
  const agentMessages = useAgentMessages();
  const cooldownStyle: CooldownStyle = useAppSlice(
    (s) => s.appState?.petInteractions?.cooldownStyle ?? "normal",
  );

  const agent = useAgentState({ petState, agentMessages });
  const interaction = useInteractionState({
    onLongPress: opts?.onLongPress,
    onInteractionSound: opts?.onInteractionSound,
    cooldownStyle,
  });
  const motion = useMotionState({
    onDragLand: () => interaction.notifyDragLand(),
  });
  const emotion = useEmotionState(
    agent,
    interaction.state as InputState,
    opts?.attention ?? null,
  );

  const agentActivityRef = useRef(Date.now());
  if (agent.kind !== "none") {
    agentActivityRef.current = Date.now();
  }

  const lastActivityAtMs = Math.max(
    agentActivityRef.current,
    interaction.lastActivityAtMs,
    motion.lastActivityAtMs,
  );

  const base = useBaseState({ lastActivityAtMs });

  const layers: PetLayers = useMemo(
    () => ({
      base,
      agent,
      input: interaction.state as InputState,
      motion: motion.state as MotionState,
      emotion,
    }),
    [base, agent, interaction.state, motion.state, emotion],
  );

  const composed = useMemo(() => composeLayers(layers), [layers]);

  return {
    layers,
    composed,
    bindInput: () => interaction.handlers,
    bindMotion: () => motion.handlers,
    notifyFailed: interaction.notifyFailed,
  };
}
