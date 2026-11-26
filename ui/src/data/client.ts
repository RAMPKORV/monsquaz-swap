import { invoke } from "@tauri-apps/api/core";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import type {
  AuditEntry,
  BotPreview,
  BotSetupInput,
  BotStatus,
  ChannelOption,
  EventDraft,
  EventSummary,
  GuildOption,
  HostSettings,
  MissionControl,
  PeopleData,
  PlanData,
  RevealData,
  RevealTrack,
  RosterLock,
  RoundOpen,
  Scenario,
  SetupData,
  Theme,
  ViewKey,
} from "../types";
import {
  demoAuditLog,
  demoBotPreview,
  demoChannels,
  demoGuilds,
  demoMissionControl,
  demoPeople,
  demoPlan,
  demoSettings,
  demoSetup,
} from "./demo";

export class HostError extends Error {
  readonly code: string;

  constructor(message: string, code: string) {
    super(message);
    this.name = "HostError";
    this.code = code;
  }
}

export function isDemoMode(): boolean {
  if (import.meta.env.VITE_DEMO === "1") {
    return true;
  }
  const params = new URLSearchParams(window.location.search);
  return import.meta.env.DEV && params.get("demo") === "1";
}

export function initialScenario(): Scenario {
  const value = new URLSearchParams(window.location.search).get("state");
  const allowed: Scenario[] = [
    "default",
    "empty",
    "loading",
    "error",
    "long",
    "dense",
    "offline",
    "reconnecting",
    "not-started",
    "lock-ready",
    "extension-ready",
    "round-ready",
    "reveal-ready",
    "infeasible",
  ];
  return allowed.includes(value as Scenario) ? (value as Scenario) : "default";
}

export function initialView(): ViewKey {
  const value = new URLSearchParams(window.location.search).get("view");
  const allowed: ViewKey[] = [
    "mission",
    "setup",
    "people",
    "plan",
    "audit",
    "settings",
  ];
  return allowed.includes(value as ViewKey) ? (value as ViewKey) : "mission";
}

export function initialTheme(): Theme {
  const value = new URLSearchParams(window.location.search).get("theme");
  if (value === "light" || value === "dark") {
    return value;
  }
  const media = window.matchMedia?.("(prefers-color-scheme: light)");
  return media?.matches ? "light" : "dark";
}

export function initialDialog(): "delete" | null {
  return new URLSearchParams(window.location.search).get("dialog") === "delete"
    ? "delete"
    : null;
}

function pause(milliseconds: number): Promise<void> {
  return new Promise((resolve) => {
    window.setTimeout(resolve, milliseconds);
  });
}

function forceLoading<T>(): Promise<T> {
  return new Promise<T>(() => {
    // A deliberate non-resolving promise for the loading review state.
  });
}

function forceError(): never {
  throw new HostError(
    "The local service did not answer. Your event data is safe.",
    "MSW-UI-001",
  );
}

export async function loadMissionControl(
  scenario: Scenario,
): Promise<MissionControl> {
  if (scenario === "loading") {
    return forceLoading<MissionControl>();
  }
  if (scenario === "error") {
    forceError();
  }
  if (isDemoMode()) {
    await pause(220);
    return demoMissionControl(scenario);
  }
  return invoke<MissionControl>("host_mission_control");
}

export async function loadSettings(scenario: Scenario): Promise<HostSettings> {
  if (scenario === "loading") {
    return forceLoading<HostSettings>();
  }
  if (scenario === "error") {
    forceError();
  }
  if (isDemoMode()) {
    await pause(160);
    return demoSettings(scenario);
  }
  const settings = await invoke<HostSettings>("host_settings");
  try {
    const status = await invoke<BotStatus>("host_bot_status");
    if (status.keychainError) {
      return { ...settings, keychainError: true };
    }
    return {
      ...settings,
      keychainError: false,
      botConfigured: status.configured,
      tokenStorage: status.tokenStorage,
      gateway: status.gateway,
    };
  } catch {
    return settings;
  }
}

export async function validateBotToken(token: string): Promise<BotPreview> {
  if (isDemoMode()) {
    await pause(320);
    const value = token.trim();
    if (
      value.length < 20 ||
      value.split(".").length !== 3 ||
      !/^[A-Za-z0-9._-]+$/.test(value)
    ) {
      throw new HostError(
        "The token does not have the expected shape.",
        "MSW-BOT-001",
      );
    }
    return demoBotPreview;
  }
  return invoke<BotPreview>("host_bot_validate", { token });
}

export async function loadBotGuilds(token: string): Promise<GuildOption[]> {
  if (isDemoMode()) {
    await pause(240);
    return demoGuilds;
  }
  return invoke<GuildOption[]>("host_bot_guilds", { token });
}

export async function loadBotChannels(
  token: string,
  guildId: string,
): Promise<ChannelOption[]> {
  if (isDemoMode()) {
    await pause(240);
    return demoChannels;
  }
  return invoke<ChannelOption[]>("host_bot_channels", { token, guildId });
}

export async function saveBotSetup(
  input: BotSetupInput,
): Promise<HostSettings> {
  if (isDemoMode()) {
    await pause(420);
    const keychainDown =
      new URLSearchParams(window.location.search).get("botKeychain") === "down";
    if (keychainDown && !input.allowSessionOnly) {
      throw new HostError(
        "The OS keychain is not available. Install a Secret Service provider, or choose the session-only option.",
        "MSW-BOT-002",
      );
    }
    const guild = demoGuilds.find((entry) => entry.id === input.guildId);
    const announcement = demoChannels.find(
      (entry) => entry.id === input.announcementChannelId,
    );
    const reveal = demoChannels.find((entry) => entry.id === input.revealChannelId);
    return {
      ...demoSettings("default"),
      botConfigured: true,
      botIdentity: demoBotPreview.identity,
      tokenStorage: input.allowSessionOnly ? "session" : "keychain",
      guildId: guild?.id ?? "",
      guildName: guild?.name ?? "",
      announcementChannelId: announcement?.id ?? "",
      announcementChannelName: announcement?.name ?? "",
      revealChannelId: reveal?.id ?? "",
      revealChannelName: reveal?.name ?? "",
    };
  }
  return invoke<HostSettings>("host_bot_save", { input });
}

export async function sendBotTestDm(userId: string): Promise<string> {
  if (isDemoMode()) {
    await pause(260);
    return "Preview: the test message is not sent in this build.";
  }
  return invoke<string>("host_bot_test_dm", { userId });
}

export async function clearBotSetup(): Promise<HostSettings> {
  if (isDemoMode()) {
    await pause(240);
    return {
      ...demoSettings("empty"),
      theme: demoSettings("default").theme,
    };
  }
  return invoke<HostSettings>("host_bot_clear");
}

export async function loadAuditLog(scenario: Scenario): Promise<AuditEntry[]> {
  if (scenario === "loading") {
    return forceLoading<AuditEntry[]>();
  }
  if (scenario === "error") {
    forceError();
  }
  if (isDemoMode()) {
    await pause(180);
    return demoAuditLog(scenario);
  }
  return invoke<AuditEntry[]>("host_audit_log");
}

export async function saveSettings(
  settings: HostSettings,
): Promise<HostSettings> {
  if (isDemoMode()) {
    await pause(420);
    return settings;
  }
  return invoke<HostSettings>("save_host_settings", { settings });
}

export async function sendTestNotification(): Promise<string> {
  if (isDemoMode()) {
    await pause(180);
    return "Preview: native notifications are not sent in this build.";
  }
  let granted = await isPermissionGranted();
  if (!granted) {
    granted = (await requestPermission()) === "granted";
  }
  if (!granted) {
    return "Notifications are blocked. Allow them in the operating system settings.";
  }
  sendNotification({
    title: "Monsquaz Swap",
    body: "Native notices are ready. Deadline decisions will appear here.",
  });
  return "Test notification sent.";
}

export function exportAuditCsv(entries: AuditEntry[]): void {
  const header = ["time", "actor", "action", "subject", "reason", "code"];
  const rows = entries.map((entry) =>
    [
      entry.at,
      entry.actor,
      entry.action,
      entry.subject,
      entry.reason,
      entry.code,
    ]
      .map((value) => `"${value.replace(/"/g, '""')}"`)
      .join(","),
  );
  const blob = new Blob([[header.join(","), ...rows].join("\n")], {
    type: "text/csv;charset=utf-8",
  });
  const link = document.createElement("a");
  link.href = URL.createObjectURL(blob);
  link.download = "monsquaz-audit-log.csv";
  link.click();
  URL.revokeObjectURL(link.href);
}

export async function loadSetup(scenario: Scenario): Promise<SetupData> {
  if (scenario === "loading") {
    return forceLoading<SetupData>();
  }
  if (scenario === "error") {
    forceError();
  }
  if (isDemoMode()) {
    await pause(180);
    return demoSetup(scenario);
  }
  return invoke<SetupData>("host_event_setup");
}

export async function loadPeople(scenario: Scenario): Promise<PeopleData> {
  if (scenario === "loading") {
    return forceLoading<PeopleData>();
  }
  if (scenario === "error") {
    forceError();
  }
  if (isDemoMode()) {
    await pause(180);
    return demoPeople(scenario);
  }
  return invoke<PeopleData>("host_people");
}

export async function setParticipantPaused(
  userId: string,
  paused: boolean,
): Promise<PeopleData> {
  if (isDemoMode()) {
    await pause(220);
    return demoPeople("default");
  }
  return invoke<PeopleData>("host_set_participant_paused", { userId, paused });
}

export async function loadPlan(scenario: Scenario): Promise<PlanData> {
  if (scenario === "loading") {
    return forceLoading<PlanData>();
  }
  if (scenario === "error") {
    forceError();
  }
  if (isDemoMode()) {
    await pause(180);
    return demoPlan(scenario);
  }
  return invoke<PlanData>("host_plan");
}

export async function generatePlan(): Promise<PlanData> {
  if (isDemoMode()) {
    await pause(420);
    return demoPlan("default");
  }
  return invoke<PlanData>("host_generate_plan");
}

const demoRevealTracks: RevealTrack[] = [
  {
    trackCode: "S7F3A",
    seedOwnerName: "Ari",
    seedOwner: "user-ari",
    entries: [
      { round: 0, displayName: "Noor", status: "Submitted", fillIn: false },
      { round: 1, displayName: "Remy", status: "Submitted", fillIn: false },
    ],
  },
  {
    trackCode: "S4K2M",
    seedOwnerName: "Noor",
    seedOwner: "user-noor",
    entries: [
      { round: 0, displayName: "Jules", status: "Submitted", fillIn: true },
      { round: 1, displayName: "Ari", status: "SkippedAfterClaim", fillIn: false },
    ],
  },
];

export async function lockRoster(): Promise<RosterLock> {
  if (isDemoMode()) {
    await pause(300);
    return { active: 12, fillIns: 0 };
  }
  return invoke<RosterLock>("host_lock_roster");
}

export async function openRound(round: number): Promise<RoundOpen> {
  if (isDemoMode()) {
    await pause(360);
    return { round, offers: 12 };
  }
  return invoke<RoundOpen>("host_open_round", { round });
}

export async function decideExtension(
  decisionId: string,
  approve: boolean,
  reason: string,
): Promise<string> {
  if (isDemoMode()) {
    await pause(260);
    return approve ? "approved" : "refused";
  }
  return invoke<string>("host_decide_extension", {
    decisionId,
    approve,
    reason,
  });
}

export async function decideDeadline(
  assignmentId: string,
  action: string,
  hours: number,
  reason: string,
): Promise<void> {
  if (isDemoMode()) {
    await pause(300);
    return;
  }
  return invoke<void>("host_decide_deadline", {
    assignmentId,
    action,
    hours,
    reason,
  });
}

export async function revealEvent(): Promise<RevealData> {
  if (isDemoMode()) {
    await pause(420);
    return {
      published: true,
      blocked: [],
      tracks: demoRevealTracks,
    };
  }
  return invoke<RevealData>("host_reveal");
}

export async function startEvent(): Promise<PlanData> {
  if (isDemoMode()) {
    await pause(360);
    return demoPlan("default");
  }
  return invoke<PlanData>("host_start_event");
}

export async function createEvent(draft: EventDraft): Promise<EventSummary> {
  if (isDemoMode()) {
    await pause(420);
    return {
      id: "preview-event",
      name: draft.name,
      mode: draft.mode,
      stage: "Schedule review",
      participants: 0,
      seeded: 0,
      fillIns: 0,
      tracks: 0,
      round: 0,
      rounds: draft.rounds ?? 0,
    };
  }
  return invoke<EventSummary>("host_create_event", { draft });
}
