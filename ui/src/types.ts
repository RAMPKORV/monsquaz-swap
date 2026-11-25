export type ViewKey = "mission" | "audit" | "settings";
export type Theme = "dark" | "light";
export type Scenario =
  | "default"
  | "empty"
  | "loading"
  | "error"
  | "long"
  | "dense"
  | "offline";

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
