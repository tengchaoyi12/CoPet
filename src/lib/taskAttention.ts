import type { TaskAttention } from "./appTypes";

export function taskAttentionSignalKey(attention: TaskAttention): string {
  return `${attention.id}:${attention.kind}:${attention.occurredAtMs}`;
}
