export type ViewKey = "mission" | "audit" | "settings" | "setup" | "people" | "plan";
export type Theme = "dark" | "light";
export type Scenario =
  | "default"
  | "empty"
  | "loading"
  | "error"
  | "long"
  | "dense"
  | "offline"
  | "infeasible";

export type MissionControl = {
  firstRun: boolean;
  event: EventSummary | null;
  bot: BotHealth;
  nextAction: NextAction | null;
  decisions: Decision[];
  activity: ActivityEntry[];
  timeline: TimelineStep[];
  facts: EventFact[];
};

export type EventSummary = {
  id: string;
  name: string;
  mode: "Classic" | "Async";
  stage: "Join phase" | "Schedule review" | "Active" | "Paused" | "Reveal";
  participants: number;
  seeded: number;
  fillIns: number;
  tracks: number;
  round: number;
  rounds: number;
};

export type BotHealth = {
  status: "Online" | "Offline" | "Not configured";
  label: string;
  detail: string;
  lastSeen: string | null;
};

export type NextAction = {
  eyebrow: string;
  title: string;
  detail: string;
  actionLabel: string;
  secondaryLabel: string | null;
  tone: "normal" | "attention";
};

export type Decision = {
  id: string;
  kind: "Deadline" | "Offer" | "Schedule" | "Delivery";
  title: string;
  detail: string;
  due: string;
  actionLabel: string;
};

export type ActivityEntry = {
  id: string;
  at: string;
  actor: string;
  action: string;
  subject: string;
};

export type TimelineStep = {
  id: string;
  label: string;
  caption: string;
  state: "done" | "current" | "next";
};

export type EventFact = {
  label: string;
  value: string;
};

export type AuditEntry = {
  id: string;
  at: string;
  actor: string;
  action: string;
  subject: string;
  reason: string;
  code: string;
};

export type HostSettings = {
  botConfigured: boolean;
  botIdentity: string;
  guildName: string;
  announcementChannel: string;
  revealChannel: string;
  dmFallback: boolean;
  timezone: string;
  retentionDays: number;
  theme: "dark" | "light" | "system";
  notifications: {
    deadlines: boolean;
    botOffline: boolean;
    releases: boolean;
  };
};

export type EventMode = "Classic" | "Async";

export type SetupWarning = {
  id: string;
  tone: "info" | "warn" | "danger";
  title: string;
  detail: string;
};

export type EventDraft = {
  name: string;
  mode: EventMode;
  rounds: number | null;
  claimWindowHours: number;
  roundWindowHours: number;
  allowlist: string[];
  packType: "Module template + samples" | "Samples only" | "Reference track" | "None";
  packRequired: boolean;
  groupMode: "Lookup" | "Single group" | "Manual";
  groupCount: number;
  groupSize: number;
  rotation: "Participants" | "Track pools" | "Both";
  swapCadence: "K rounds" | "Calendar" | "Host-triggered";
  scheduleVisibility: "Hidden" | "Visible assignments";
};

export type SetupData = {
  firstRun: boolean;
  event: EventSummary | null;
  draft: EventDraft;
  warnings: SetupWarning[];
};

export type PersonRow = {
  id: string;
  displayName: string;
  role: "Seed" | "Fill-in";
  status: "Ready" | "Paused" | "Left";
  seedTrack: string | null;
  timezone: string;
  maxHandoffs: number;
  joined: string;
  deliveryRisk: boolean;
};

export type PeopleData = {
  event: EventSummary | null;
  rows: PersonRow[];
};

export type PlanCell = {
  round: number;
  trackCode: string;
  participantId: string;
  participantName: string;
  isYou?: boolean;
};

export type MatrixTrack = {
  trackCode: string;
  ownerGroup: string;
  cells: PlanCell[];
};

export type PlannerGroup = {
  name: string;
  size: number;
  tracks: number;
  participants: string[];
};

export type PlannerWarning = {
  id: string;
  severity: "info" | "warn" | "danger";
  title: string;
  detail: string;
};

export type GraphEdge = {
  trackCode: string;
  participantName: string;
  isYou?: boolean;
};

export type PlanData = {
  status: "No event" | "No plan" | "Draft" | "Committed" | "Infeasible";
  event: EventSummary | null;
  rounds: number;
  tracks: number;
  participants: number;
  committed: boolean;
  matrix: MatrixTrack[];
  graph: { round: number; edges: GraphEdge[] };
  groups: PlannerGroup[];
  warnings: PlannerWarning[];
  funScore: number | null;
  funGuardrails: Array<{ label: string; value: string }>;
};
