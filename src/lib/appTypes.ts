export type PetStateId =
  | "idle"
  | "running-right"
  | "running-left"
  | "waving"
  | "jumping"
  | "failed"
  | "waiting"
  | "running"
  | "review"
  | "thinking";

export type PetState = {
  id: PetStateId;
  row: number;
  frames: number;
  durationMs: number;
};

export type PetInteractionSounds = {
  click?: string;
  doubleClick?: string;
  petted?: string;
  pettedSlow?: string;
  dragLand?: string;
};

export type PetAgentSounds = {
  thinking?: string;
  editing?: string;
  inspecting?: string;
  awaitingApproval?: string;
  celebrating?: string;
  failed?: string;
};

export type PetSounds = {
  interactionSounds?: PetInteractionSounds;
  agentSounds?: PetAgentSounds;
};

export type PetSummary = {
  id: string;
  slug: string;
  displayName: string;
  description: string;
  frameWidth: number;
  frameHeight: number;
  gridColumns: number;
  gridRows: number;
  builtIn: boolean;
  spritePath: string;
  sounds?: PetSounds;
};

export type SoundPackSummary = {
  id: string;
  slug: string;
  displayName: string;
  builtIn: boolean;
  sounds: PetSounds;
};

export type Locale = "en-US" | "zh-CN";
export type LocalePreference = Locale;

export type AgentMessageDisplay = "all" | "latest";

export type CooldownStyle = "short" | "normal" | "lazy";

export type PetInteractionPrefs = {
  enableClickSounds: boolean;
  cooldownStyle: CooldownStyle;
  enableStartupAnimation: boolean;
};

export const defaultPetInteractionPrefs: PetInteractionPrefs = {
  enableClickSounds: true,
  cooldownStyle: "normal",
  enableStartupAnimation: true,
};

export type AppState = {
  currentPetId: string;
  currentSoundPackId: string;
  locale: Locale;
  localePreference: LocalePreference;
  pets: PetSummary[];
  soundPacks: SoundPackSummary[];
  onboardingComplete: boolean;
  petWindowSize: PetWindowSize;
  agentMessageDisplay: AgentMessageDisplay;
  agentMessageVisible: boolean;
  petInteractions: PetInteractionPrefs;
};

export type PetWindowSize = number;

export type PetImportResult = {
  imported: number;
  skipped: number;
  pets: PetSummary[];
};

export type PetImportSession = { sessionId: string };

export type PetImportPreview = {
  previewId: string;
  summary: PetSummary;
  sourceLabel: string;
  intendedPetId: string;
  selectedByDefault: boolean;
  warning?: string;
};

export type PetImportPreviewBatch = {
  previews: PetImportPreview[];
  skipped: number;
  errors: string[];
};

export type PetImportFailure = {
  previewId: string;
  errorMessage: string;
};

export type PetImportCommitResult = {
  imported: PetSummary[];
  failed: PetImportFailure[];
  state: AppState;
};

export type DerivedPetState = {
  state: PetStateId;
  sinceMs: number;
  idleAfterMs: number | null;
};

export type AgentMessage = {
  agent: string;
  displayName: string;
  text: string;
  updatedAtMs: number;
};

export type TaskStatus = "running" | "waiting" | "completed" | "failed";

export type TaskActionKind = "continue" | "permission";

export type TaskActionState = "pending" | "resolving" | "expired";

export type TaskActionDecision = "continueOnce" | "allowOnce" | "fallback";

export type TaskAction = {
  id: string;
  kind: TaskActionKind;
  state: TaskActionState;
  label: string;
  requestedAction: string;
  toolName: string | null;
  command: string | null;
  cwd: string | null;
  expiresAtMs: number;
  quickActionAllowed: boolean;
};

export type TaskNotification = {
  id: string;
  agent: string;
  displayName: string;
  sessionId: string | null;
  turnId: string | null;
  status: TaskStatus;
  title: string | null;
  summary: string | null;
  unread: boolean;
  updatedAtMs: number;
  action: TaskAction | null;
};

export type TaskAttention = {
  id: string;
  kind: "waiting" | "completed" | "failed";
  occurredAtMs: number;
};

export type RuntimeUpdate = {
  currentState: DerivedPetState;
  messages: AgentMessage[];
  notifications: TaskNotification[];
  attention: TaskAttention | null;
};

export type RuntimeStatus = {
  port: number;
  endpoint: string;
  currentState: DerivedPetState;
  messages: AgentMessage[];
  notifications: TaskNotification[];
  attention: TaskAttention | null;
  acceptedEvents: number;
  rejectedEvents: number;
};

export type AdapterSummary = {
  id: string;
  displayName: string;
  configPath: string;
  installed: boolean;
  healthy: boolean;
  message: string;
};

export type AdapterOperationResult = {
  adapter: AdapterSummary;
};
