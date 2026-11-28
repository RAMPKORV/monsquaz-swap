import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import type {
  AuditEntry,
  BackupStatus,
  RestoreDone,
  RestorePreview,
  RestoreSource,
  BotPreview,
  BotSetupInput,
  BotStatus,
  ChannelOption,
  EventDraft,
  EventSummary,
  FillInCandidate,
  GuildOption,
  HostSettings,
  LateSeed,
  MissionControl,
  PackInfo,
  PeopleData,
  CommitmentCheck,
  PlanData,
  PlanEditCandidate,
  RevealData,
  RevealPackage,
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
  demoBackupStatus,
  demoBotPreview,
  demoChannels,
  demoGuilds,
  demoMissionControl,
  demoPeople,
  demoPlan,
  demoRevealPackage,
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
    "join-phase",
    "join-late",
    "join-open",
    "seed-setup",
    "infeasible",
    "flexible",
    "async",
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
    "reveal",
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

export async function startAsyncEvent(): Promise<EventSummary> {
  if (isDemoMode()) {
    await pause(300);
    return demoEventSummary({ stage: "Active" });
  }
  return invoke<EventSummary>("host_start_async_event");
}

export async function dispatchAsyncWave(): Promise<{ offers: number }> {  if (isDemoMode()) {
    await pause(320);
    return { offers: 4 };
  }
  return invoke<{ offers: number }>("host_async_dispatch");
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

/** D-051: the host marks a participant as left. The UI confirms twice. */
export async function markParticipantLeft(userId: string): Promise<PeopleData> {
  if (isDemoMode()) {
    await pause(220);
    return demoPeople("default");
  }
  return invoke<PeopleData>("host_mark_participant_left", { userId });
}

/** D-051: the bot asks a participant who left to come back. */
export async function restoreParticipant(userId: string): Promise<PeopleData> {
  if (isDemoMode()) {
    await pause(220);
    return demoPeople("default");
  }
  return invoke<PeopleData>("host_restore_participant", { userId });
}

export async function addAvailability(
  participantId: string,
  startsAt: number,
  endsAt: number,
  note: string,
): Promise<PeopleData> {
  if (isDemoMode()) {
    await pause(200);
    return demoPeople("default");
  }
  return invoke<PeopleData>("host_add_availability", {
    participantId,
    startsAt,
    endsAt,
    note: note || null,
  });
}

export async function removeAvailability(id: string): Promise<PeopleData> {
  if (isDemoMode()) {
    await pause(180);
    return demoPeople("default");
  }
  return invoke<PeopleData>("host_remove_availability", { id });
}

let demoPlanState: PlanData | null = null;

export function resetDemoPlan(): void {
  demoPlanState = null;
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
    const data = demoPlan(scenario);
    demoPlanState = data;
    return structuredClone(data);
  }
  return invoke<PlanData>("host_plan");
}

export async function generatePlan(): Promise<PlanData> {
  if (isDemoMode()) {
    await pause(420);
    demoPlanState = demoPlan("default");
    return structuredClone(demoPlanState);
  }
  return invoke<PlanData>("host_generate_plan");
}

export async function repairGroups(): Promise<PlanData> {
  if (isDemoMode()) {
    await pause(320);
    demoPlanState = demoPlanState ?? demoPlan("default");
    return structuredClone(demoPlanState);
  }
  return invoke<PlanData>("host_repair_groups");
}

function demoSwapValid(
  plan: PlanData,
  round: number,
  trackCode: string,
  participantId: string,
): boolean {
  const track = plan.matrix.find((item) => item.trackCode === trackCode);
  const target = track?.cells.find((cell) => cell.round === round);
  if (!track || !target || target.participantId === participantId) {
    return false;
  }
  const current = plan.matrix
    .flatMap((item) => item.cells)
    .find(
      (cell) =>
        cell.round === round && cell.participantId === participantId,
    );
  if (!current) {
    return false;
  }
  if (track.ownerId === participantId) {
    return false;
  }
  const currentTrack = plan.matrix.find(
    (item) => item.trackCode === current.trackCode,
  );
  if (!currentTrack || currentTrack.ownerId === target.participantId) {
    return false;
  }
  const candidateSawTarget = plan.matrix.some(
    (item) =>
      item.trackCode === trackCode &&
      item.cells.some(
        (cell) =>
          cell.round !== round &&
          cell.participantId === participantId,
      ),
  );
  if (candidateSawTarget) {
    return false;
  }
  const displacedSawCurrent = plan.matrix.some(
    (item) =>
      item.trackCode === current.trackCode &&
      item.cells.some(
        (cell) =>
          cell.round !== round &&
          cell.participantId === target.participantId,
      ),
  );
  return !displacedSawCurrent;
}

export async function loadPlanEditCandidates(
  round: number,
  trackCode: string,
): Promise<PlanEditCandidate[]> {
  if (isDemoMode()) {
    await pause(160);
    if (!demoPlanState) {
      demoPlanState = demoPlan("default");
    }
    if (
      new URLSearchParams(window.location.search).get("nocandidates") === "1"
    ) {
      return [];
    }
    const plan = demoPlanState;
    const target = plan.matrix
      .find((item) => item.trackCode === trackCode)
      ?.cells.find((cell) => cell.round === round);
    if (!target) {
      return [];
    }
    const people = new Map<string, string>();
    for (const track of plan.matrix) {
      for (const cell of track.cells) {
        if (cell.round === round && cell.participantId) {
          people.set(cell.participantId, cell.participantName);
        }
      }
    }
    return [...people]
      .filter(
        ([participantId]) =>
          participantId !== target.participantId &&
          demoSwapValid(plan, round, trackCode, participantId),
      )
      .map(([participantId, participantName]) => ({
        participantId,
        participantName,
        currentTrack:
          plan.matrix
            .flatMap((item) => item.cells)
            .find(
              (cell) =>
                cell.round === round &&
                cell.participantId === participantId,
            )?.trackCode ?? "",
      }))
      .sort((left, right) =>
        left.participantName.localeCompare(right.participantName),
      );
  }
  return invoke<PlanEditCandidate[]>("host_plan_edit_candidates", {
    round,
    trackCode,
  });
}

export async function editPlanAssignment(
  round: number,
  trackCode: string,
  participantId: string,
  expectedVersion: number,
): Promise<PlanData> {
  if (isDemoMode()) {
    await pause(260);
    if (!demoPlanState) {
      demoPlanState = demoPlan("default");
    }
    if (demoPlanState.version !== expectedVersion) {
      throw new Error("The plan changed. Close this dialog and try again.");
    }
    if (!demoSwapValid(demoPlanState, round, trackCode, participantId)) {
      throw new Error(
        "That person cannot take this track without breaking a rule.",
      );
    }
    const track = demoPlanState.matrix.find(
      (item) => item.trackCode === trackCode,
    );
    const target = track?.cells.find((cell) => cell.round === round);
    const current = demoPlanState.matrix
      .flatMap((item) => item.cells)
      .find(
        (cell) =>
          cell.round === round && cell.participantId === participantId,
      );
    if (!target || !current) {
      throw new Error("That assignment cannot be changed.");
    }
    const previous = {
      participantId: target.participantId,
      participantName: target.participantName,
    };
    target.participantId = current.participantId;
    target.participantName = current.participantName;
    current.participantId = previous.participantId;
    current.participantName = previous.participantName;
    demoPlanState.version += 1;
    demoPlanState.commitment = `demo-commit-${demoPlanState.version}`;
    return structuredClone(demoPlanState);
  }
  return invoke<PlanData>("host_edit_plan_assignment", {
    round,
    trackCode,
    participantId,
    expectedVersion,
  });
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

let demoRevealState: RevealPackage | null = null;

export function resetDemoReveal(): void {
  demoRevealState = null;
}

export async function loadRevealPackage(
  scenario: Scenario,
): Promise<RevealPackage> {
  if (scenario === "loading") {
    return forceLoading<RevealPackage>();
  }
  if (scenario === "error") {
    forceError();
  }
  if (isDemoMode()) {
    await pause(220);
    if (scenario !== "default" || !demoRevealState) {
      demoRevealState = demoRevealPackage(scenario);
    }
    if (
      new URLSearchParams(window.location.search).get("published") === "1" &&
      demoRevealState
    ) {
      demoRevealState.published = true;
      demoRevealState.eventState = "Reveal";
    }
    return structuredClone(demoRevealState);
  }
  return invoke<RevealPackage>("host_reveal_package");
}

function demoRevealUpdate(
  update: (packageData: RevealPackage) => void,
): RevealPackage {
  if (!demoRevealState) {
    demoRevealState = demoRevealPackage("default");
  }
  update(demoRevealState);
  return structuredClone(demoRevealState);
}

export async function setTrackTitle(
  trackCode: string,
  title: string,
): Promise<RevealPackage> {
  if (isDemoMode()) {
    await pause(200);
    return demoRevealUpdate((packageData) => {
      const track = packageData.album.find((item) => item.trackCode === trackCode);
      if (track) {
        track.title = title.trim();
      }
    });
  }
  return invoke<RevealPackage>("host_set_track_title", { trackCode, title });
}

export async function addTrackComment(
  trackCode: string,
  participantId: string,
  kind: string,
  body: string,
): Promise<RevealPackage> {
  if (isDemoMode()) {
    await pause(200);
    return demoRevealUpdate((packageData) => {
      const track = packageData.album.find((item) => item.trackCode === trackCode);
      if (!track) {
        return;
      }
      const authorName =
        track.credits.find((credit) => credit.displayName !== "No edit")
          ?.displayName ?? track.seedOwnerName;
      track.comments.push({
        id: `comment-${trackCode}-${track.comments.length + 1}`,
        participantId,
        authorName,
        kind,
        body: body.trim(),
        createdAt: Math.floor(Date.now() / 1000),
      });
    });
  }
  return invoke<RevealPackage>("host_add_track_comment", {
    trackCode,
    participantId,
    kind,
    body,
  });
}

export async function redactTrackComment(
  commentId: string,
): Promise<RevealPackage> {
  if (isDemoMode()) {
    await pause(160);
    return demoRevealUpdate((packageData) => {
      for (const track of packageData.album) {
        track.comments = track.comments.filter(
          (comment) => comment.id !== commentId,
        );
      }
    });
  }
  return invoke<RevealPackage>("host_redact_track_comment", { commentId });
}

export function previewUrl(fileId: string): string {
  if (isDemoMode()) {
    return "";
  }
  return convertFileSrc(fileId, "swap-preview");
}

export async function exportRevealZip(path: string): Promise<string> {
  if (isDemoMode()) {
    await pause(260);
    return path;
  }
  return invoke<string>("host_export_reveal_zip", { path });
}

export async function loadFillInCandidates(
  trackCode: string,
  roundNo: number,
): Promise<FillInCandidate[]> {
  if (isDemoMode()) {
    await pause(200);
    return [
      {
        participantId: "user-08",
        displayName: "Tove",
        seen: 2,
        cap: 8,
        tier: "volunteer",
        edits: 1,
      },
      {
        participantId: "user-04",
        displayName: "Jules",
        seen: 3,
        cap: 8,
        tier: "volunteer",
        edits: 3,
      },
    ];
  }
  return invoke<FillInCandidate[]>("host_fill_in_candidates", {
    trackCode,
    roundNo,
  });
}

/** `dueAt` is UTC seconds. Without it, the app uses the default due time. */
export async function assignFillIn(
  trackCode: string,
  roundNo: number,
  participantId: string,
  dueAt?: number,
): Promise<void> {
  if (isDemoMode()) {
    await pause(220);
    return;
  }
  return invoke<void>("host_assign_fill_in", {
    trackCode,
    roundNo,
    participantId,
    dueAt: dueAt ?? null,
  });
}

export type FillInListResult = {
  offeredTo: string | null;
  status: string;
  decisionOpened: boolean;
};

/** The bot offers the track down the ordered list, one person at a time. */
export async function startFillInList(
  trackCode: string,
  roundNo: number,
  participantIds: string[],
): Promise<FillInListResult> {
  if (isDemoMode()) {
    await pause(220);
    return {
      offeredTo: participantIds[0] ?? null,
      status: "active",
      decisionOpened: false,
    };
  }
  return invoke<FillInListResult>("host_start_fill_in_list", {
    trackCode,
    roundNo,
    participantIds,
  });
}

export async function revealEvent(): Promise<RevealData> {
  if (isDemoMode()) {
    await pause(420);
    if (new URLSearchParams(window.location.search).get("blocked") === "1") {
      return {
        published: false,
        blocked: ["Track S7F3A · round 3"],
        tracks: [],
      };
    }
    if (demoRevealState) {
      demoRevealState.published = true;
    }
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
      exchangeRounds: draft.rounds ?? 0,
      seedRoundOpen: false,
      seedDeadlineAt: null,
      lateSeedCount: 0,
      packId: null,
      openSchedule: false,
      handoffPolicy: "Strict",
      fillInFlow: draft.fillInFlow,
      fillInWindowHours: draft.fillInWindowHours,
      fillInClaimWindowHours: draft.fillInClaimWindowHours,
    };
  }
  return invoke<EventSummary>("host_create_event", { draft });
}

export async function uploadPack(path: string): Promise<PackInfo> {
  if (isDemoMode()) {
    await pause(300);
    return {
      id: "pack-demo",
      filename: path.split(/[\\/]/).pop() ?? "pack.zip",
      sha256: "e3b0c44298fc1c149afbf4c8996fb924",
      size: 12_400_000,
      locked: false,
    };
  }
  return invoke<PackInfo>("host_upload_pack", { path });
}

export async function openSeedRound(
  deadlineAt: number,
  channelId: string | null,
): Promise<EventSummary> {
  if (isDemoMode()) {
    await pause(320);
    return demoEventSummary({ seedRoundOpen: true, seedDeadlineAt: deadlineAt, round: 1 });
  }
  return invoke<EventSummary>("host_open_seed_round", {
    deadlineAt,
    channelId,
  });
}

export async function extendSeedDeadline(deadlineAt: number): Promise<EventSummary> {
  if (isDemoMode()) {
    await pause(220);
    return demoEventSummary({ seedRoundOpen: true, seedDeadlineAt: deadlineAt, round: 1 });
  }
  return invoke<EventSummary>("host_extend_seed_deadline", { deadlineAt });
}

export async function reopenSeedRound(deadlineAt: number): Promise<EventSummary> {
  if (isDemoMode()) {
    await pause(220);
    return demoEventSummary({ seedRoundOpen: true, seedDeadlineAt: deadlineAt, round: 1 });
  }
  return invoke<EventSummary>("host_reopen_seed_round", { deadlineAt });
}

export async function pendingLateSeeds(): Promise<LateSeed[]> {
  if (isDemoMode()) {
    await pause(180);
    return [];
  }
  return invoke<LateSeed[]>("host_pending_late_seeds");
}

export async function decideLateSeed(
  id: string,
  accept: boolean,
  reason: string,
): Promise<EventSummary> {
  if (isDemoMode()) {
    await pause(240);
    return demoEventSummary({ lateSeedCount: 0 });
  }
  return invoke<EventSummary>("host_decide_late_seed", { id, accept, reason });
}

export async function moveRoundDeadline(
  roundNo: number,
  deadlineAt: number,
  reason: string,
): Promise<EventSummary> {
  if (isDemoMode()) {
    await pause(240);
    return demoEventSummary({ round: roundNo });
  }
  return invoke<EventSummary>("host_move_round_deadline", {
    roundNo,
    deadlineAt,
    reason,
  });
}

export async function verifyCommitment(): Promise<CommitmentCheck> {
  if (isDemoMode()) {
    await pause(160);
    return {
      recipe: "v2",
      commitment: "a1b2c3d4e5f60718293a4b5c6d7e8f90",
      verified: true,
    };
  }
  return invoke<CommitmentCheck>("host_verify_commitment");
}

function demoEventSummary(
  overrides: Partial<EventSummary> = {},
): EventSummary {
  return {
    id: "preview-event",
    name: "Monsquaz Swap",
    mode: "Classic",
    stage: "Join phase",
    participants: 12,
    seeded: 9,
    fillIns: 0,
    tracks: 9,
    round: 1,
    rounds: 12,
    exchangeRounds: 11,
    seedRoundOpen: true,
    seedDeadlineAt: null,
    lateSeedCount: 0,
    packId: "pack-demo",
    openSchedule: false,
    handoffPolicy: "Strict",
    fillInFlow: "host_appoints",
    fillInWindowHours: 24,
    fillInClaimWindowHours: 6,
    ...overrides,
  };
}

// Backups, the restore wizard, and the single-writer guard. The main process
// keeps every folder path; the UI receives display names and opaque ids.

function demoBackupScenario(): "default" | "empty" | "long" | "foreign" {
  const value = new URLSearchParams(window.location.search).get("backup");
  return value === "empty" || value === "long" || value === "foreign"
    ? value
    : "default";
}

export async function loadBackupStatus(): Promise<BackupStatus> {
  if (isDemoMode()) {
    const scenario = demoBackupScenario();
    const state = new URLSearchParams(window.location.search).get("backup");
    if (state === "error") {
      throw new HostError(
        "The backup folder is not readable. Select the folder again.",
        "MSW-BACKUP-001",
      );
    }
    if (state === "loading") {
      return new Promise<BackupStatus>(() => {
        // A deliberate non-resolving promise for the loading review state.
      });
    }
    await pause(120);
    return demoBackupStatus(scenario);
  }
  return invoke<BackupStatus>("host_backup_status");
}

export async function chooseBackupFolder(): Promise<BackupStatus> {
  if (isDemoMode()) {
    await pause(200);
    const scenario = demoBackupScenario();
    return demoBackupStatus(scenario === "empty" ? "default" : scenario);
  }
  return invoke<BackupStatus>("host_backup_choose_folder");
}

export async function backupNow(): Promise<BackupStatus> {
  if (isDemoMode()) {
    await pause(300);
    return demoBackupStatus(
      demoBackupScenario() === "long" ? "long" : "default",
    );
  }
  return invoke<BackupStatus>("host_backup_now");
}

export async function purgeBackups(confirmation: string): Promise<BackupStatus> {
  if (isDemoMode()) {
    await pause(200);
    if (confirmation.trim() !== "PURGE") {
      throw new HostError("Type PURGE to confirm.", "MSW-BACKUP-004");
    }
    return { ...demoBackupStatus(), backups: [] };
  }
  return invoke<BackupStatus>("host_backup_purge", { confirmation });
}

export async function takeOverWriter(confirmation: string): Promise<BackupStatus> {
  if (isDemoMode()) {
    await pause(200);
    if (confirmation.trim() !== "TAKE OVER") {
      throw new HostError("Type TAKE OVER to confirm.", "MSW-BACKUP-005");
    }
    return {
      ...demoBackupStatus(),
      writer: {
        state: "own",
        machine: "4f2a9c1e",
        heartbeatAt: Math.floor(Date.now() / 1000),
        lostLock: null,
      },
    };
  }
  return invoke<BackupStatus>("host_writer_takeover", { confirmation });
}

export async function loadRestoreSource(): Promise<RestoreSource> {
  if (isDemoMode()) {
    await pause(120);
    const empty =
      new URLSearchParams(window.location.search).get("restore") === "empty";
    const status = empty ? demoBackupStatus("empty") : demoBackupStatus();
    return {
      folderName: empty ? "Swap backups" : status.folderName,
      backups: empty ? [] : status.backups,
    };
  }
  return invoke<RestoreSource>("host_restore_source");
}

export async function chooseRestoreFolder(): Promise<RestoreSource> {
  if (isDemoMode()) {
    return loadRestoreSource();
  }
  return invoke<RestoreSource>("host_restore_choose_folder");
}

export async function previewRestore(id: string): Promise<RestorePreview> {
  if (isDemoMode()) {
    await pause(260);
    if (new URLSearchParams(window.location.search).get("restore") === "error") {
      throw new HostError(
        "The backup is damaged. The file S7F3A.it does not match its hash. The app did not change the current data.",
        "MSW-BACKUP-002",
      );
    }
    const backup = demoBackupStatus().backups.find((item) => item.id === id);
    return {
      id,
      kind: backup?.kind ?? "daily",
      createdAt: backup?.createdAt ?? 0,
      appVersion: "0.1.1",
      files: 31,
      events: backup?.events ?? [],
      staleness: "older",
      newerEntries: 4,
    };
  }
  return invoke<RestorePreview>("host_restore_preview", { id });
}

export async function restoreBackup(
  id: string,
  confirmation: string,
): Promise<RestoreDone> {
  if (isDemoMode()) {
    await pause(400);
    if (confirmation.trim() !== "RESTORE") {
      throw new HostError("Type RESTORE to confirm.", "MSW-BACKUP-003");
    }
    const preview = await previewRestore(id);
    return { events: preview.events, keptPrevious: true };
  }
  return invoke<RestoreDone>("host_restore", { id, confirmation });
}
