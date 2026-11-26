import type {
  ActivityEntry,
  AuditEntry,
  BotPreview,
  ChannelOption,
  Decision,
  GuildOption,
  HostSettings,
  MissionControl,
  RevealAlbumTrack,
  RevealPackage,
  Scenario,
  TimelineStep,
} from "../types";

const longEventName =
  "The Extremely Long and Slightly Haunted Midwinter Remix Exchange, Volume Two";

const defaultTimeline: TimelineStep[] = [
  {
    id: "seed",
    label: "Seeds",
    caption: "12 tracks",
    state: "done",
  },
  {
    id: "lock",
    label: "Roster lock",
    caption: "12 active",
    state: "done",
  },
  {
    id: "round-1",
    label: "Round 1",
    caption: "12 edits",
    state: "done",
  },
  {
    id: "round-2",
    label: "Round 2",
    caption: "11 edits",
    state: "done",
  },
  {
    id: "round-3",
    label: "Round 3",
    caption: "10 claimed",
    state: "current",
  },
  {
    id: "reveal",
    label: "Reveal",
    caption: "After stream",
    state: "next",
  },
];

const defaultDecisions: Decision[] = [
  {
    id: "decision-1",
    kind: "Deadline",
    title: "Track S7F3A is overdue",
    detail:
      "Noor still holds Track S7F3A. Only the host can see this decision. Extend 6 hours, mark the turn missed, or keep waiting.",
    due: "42 minutes ago",
    actionLabel: "Review",
    assignmentId: "assignment-1",
    deadline: "25 Nov, 18:00 CET",
    newDeadline: "26 Nov, 00:42 CET",
    extensionsUsed: 0,
  },
  {
    id: "decision-2",
    kind: "Offer",
    title: "One offer expires in 3 hours",
    detail:
      "S4K2M has not been claimed. If the offer expires, the offer does not count as seen and the track returns to the pool.",
    due: "25 Nov, 17:40 CET",
    actionLabel: "Review",
  },
  {
    id: "decision-3",
    kind: "Schedule",
    title: "Round 4 is ready to open",
    detail:
      "All Round 3 claims are resolved. Opening the round sends 10 offers in the next offer batch.",
    due: "After Round 3 closes",
    actionLabel: "Preview",
  },
];

const defaultActivity: ActivityEntry[] = [
  {
    id: "activity-1",
    at: "17:12",
    actor: "Host",
    action: "extended a deadline",
    subject: "S9Q1B, +6 hours",
  },
  {
    id: "activity-2",
    at: "16:48",
    actor: "Noor",
    action: "claimed",
    subject: "S9Q1B",
  },
  {
    id: "activity-3",
    at: "16:31",
    actor: "Remy",
    action: "submitted a new version of",
    subject: "S2M8C",
  },
  {
    id: "activity-4",
    at: "16:02",
    actor: "Round 3",
    action: "opened with",
    subject: "10 offers sent",
  },
  {
    id: "activity-5",
    at: "15:40",
    actor: "Jules",
    action: "joined as a fill-in",
    subject: "No personal track",
  },
  {
    id: "activity-6",
    at: "14:55",
    actor: "Round 2",
    action: "closed with",
    subject: "11 of 12 edits",
  },
];

function defaultFacts() {
  return [
    { label: "Mode", value: "Classic, 11 rounds" },
    { label: "People", value: "12 active, including 2 fill-ins" },
    { label: "Tracks", value: "12 anonymous tracks" },
    { label: "Round length", value: "24 hours" },
    { label: "Claim window", value: "12 hours" },
    { label: "Next release", value: "Fri 27 Nov, 20:00 CET" },
    { label: "Timezone", value: "Europe/Stockholm" },
  ];
}

function baseMission(): MissionControl {
  return {
    firstRun: false,
    event: {
      id: "night-shift-2",
      name: "Night Shift Vol. 2",
      mode: "Classic",
      stage: "Active",
      participants: 12,
      seeded: 12,
      fillIns: 2,
      tracks: 12,
      round: 3,
      rounds: 11,
    },
    bot: {
      status: "Online",
      label: "Monsquaz Bot",
      detail: "Discord connection healthy, 82 ms",
      lastSeen: "14 seconds ago",
    },
    nextAction: {
      eyebrow: "Next action",
      title: "Three decisions need a host call",
      detail:
        "Round 3 closes Fri 27 Nov, 18:00 (Europe/Stockholm). One track is past its deadline and one offer expires at 17:40.",
      actionLabel: "Review decisions",
      secondaryLabel: "Pause event",
      tone: "attention",
    },
    decisions: defaultDecisions,
    activity: defaultActivity,
    timeline: defaultTimeline,
    facts: defaultFacts(),
  };
}

export function demoMissionControl(scenario: Scenario): MissionControl {
  if (scenario === "empty") {
    return {
      firstRun: true,
      event: null,
      bot: {
        status: "Not configured",
        label: "No bot connected",
        detail: "Connect a bot token to send offers and collect files.",
        lastSeen: null,
      },
      nextAction: null,
      decisions: [],
      activity: [],
      timeline: [],
      facts: [],
    };
  }

  const mission = baseMission();

  if (scenario === "offline") {
    mission.bot = {
      status: "Offline",
      label: "Monsquaz Bot",
      detail: "Discord rejected the bot token",
      lastSeen: "18 minutes ago",
    };
    mission.nextAction = {
      eyebrow: "Next action",
      title: "Reconnect the bot",
      detail:
        "The queue is safe. Reconnect before the next batch so offers can go out.",
      actionLabel: "Reconnect now",
      secondaryLabel: null,
      tone: "attention",
    };
    return mission;
  }

  if (scenario === "not-started") {
    mission.bot = {
      status: "Offline",
      label: "Monsquaz Bot",
      detail: "The gateway is not running.",
      lastSeen: null,
    };
    mission.nextAction = {
      eyebrow: "Next action",
      title: "Start the bot",
      detail: "A token is stored. Open settings and start the Discord gateway.",
      actionLabel: "Open settings",
      secondaryLabel: null,
      tone: "attention",
    };
    return mission;
  }

  if (scenario === "reconnecting") {
    mission.bot = {
      status: "Reconnecting",
      label: "Monsquaz Bot",
      detail: "Discord is not answering. Retry in 8 seconds.",
      lastSeen: "2 minutes ago",
    };
    return mission;
  }

  if (scenario === "lock-ready") {
    if (mission.event) {
      mission.event.stage = "Join phase";
      mission.event.round = 0;
      mission.event.seeded = 10;
      mission.event.fillIns = 0;
      mission.event.tracks = 10;
    }
    mission.nextAction = {
      eyebrow: "Next action",
      title: "Lock the roster",
      detail:
        "10 seeds are in and 2 people still need a seed. Locking closes seed intake; the 2 people become fill-ins.",
      actionLabel: "Lock roster",
      secondaryLabel: null,
      tone: "attention",
    };
    mission.facts = mission.facts.map((fact) =>
      fact.label === "People"
        ? { ...fact, value: "12 signed up, 10 seeds, 2 without a seed" }
        : fact.label === "Tracks"
          ? { ...fact, value: "10 anonymous tracks" }
          : fact.label === "Next release"
            ? { ...fact, label: "First release", value: "After the schedule locks" }
            : fact,
    );
    mission.timeline = [
      {
        id: "seeds",
        label: "Seeds",
        caption: "10 tracks · 2 signups",
        state: "done",
      },
      {
        id: "roster",
        label: "Roster lock",
        caption: "Not locked",
        state: "current",
      },
      {
        id: "rounds",
        label: "Rounds",
        caption: "11 planned",
        state: "next",
      },
      {
        id: "reveal",
        label: "Reveal",
        caption: "After the final round",
        state: "next",
      },
    ];
    mission.decisions = [
      {
        id: "lock-ready",
        kind: "Seeds",
        title: "2 people still need a seed",
        detail:
          "A participant without a seed becomes a fill-in when signups close. The host can wait or invite another seed.",
        due: "Before roster lock",
        actionLabel: "Review",
      },
    ];
    return mission;
  }

  if (scenario === "extension-ready") {
    mission.nextAction = {
      eyebrow: "Next action",
      title: "Review the extension request",
      detail:
        "Noor asked for 6 more hours on Track S7F3A. The deadline does not move until you decide.",
      actionLabel: "Review decisions",
      secondaryLabel: null,
      tone: "attention",
    };
    mission.decisions = [
      {
        id: "extension-ready",
        kind: "Extension",
        title: "Noor asked for 6 more hours",
        detail:
          "Noor has used 1 of 2 extensions. This is the second request. Noor keeps Track S7F3A until you decide.",
        due: "Awaiting host",
        actionLabel: "Review",
        assignmentId: "assignment-1",
        decisionId: "decision-1",
        requestedHours: 6,
        deadline: "26 Nov, 09:00 CET",
        newDeadline: "26 Nov, 15:00 CET",
        extensionsUsed: 1,
      },
    ];
    return mission;
  }

  if (scenario === "round-ready") {
    mission.nextAction = {
      eyebrow: "Next action",
      title: "Open round 4",
      detail:
        "Every track in round 3 is resolved. Opening round 4 sends 10 offers in the next batch.",
      actionLabel: "Open Round 4",
      secondaryLabel: null,
      tone: "attention",
    };
    mission.timeline = mission.timeline.map((step) =>
      step.id === "round-3" ? { ...step, state: "done" } : step,
    );
    mission.decisions = [
      {
        id: "round-ready",
        kind: "Schedule",
        title: "Round 4 is ready to open",
        detail:
          "All round 3 claims are resolved. Opening the round sends ten offers in the next batch.",
        due: "After round 3 closes",
        actionLabel: "Preview",
      },
    ];
    return mission;
  }

  if (scenario === "reveal-ready") {
    if (mission.event) mission.event.round = 11;
    mission.nextAction = {
      eyebrow: "Next action",
      title: "Reveal the event",
      detail:
        "Every planned round is complete. Reveal publishes the credits to the reveal channel.",
      actionLabel: "Reveal event",
      secondaryLabel: null,
      tone: "attention",
    };
    mission.timeline = mission.timeline.map((step) =>
      step.id === "reveal"
        ? { ...step, state: "current" }
        : { ...step, state: "done" },
    );
    mission.facts = mission.facts.map((fact) =>
      fact.label === "Next release"
        ? { ...fact, label: "Reveal", value: "Ready now" }
        : fact,
    );
    mission.decisions = [];
    return mission;
  }

  if (scenario === "long") {
    mission.event = {
      id: "long-event",
      name: longEventName,
      mode: "Classic",
      stage: "Active",
      participants: 48,
      seeded: 40,
      fillIns: 8,
      tracks: 20,
      round: 9,
      rounds: 12,
    };
    mission.bot = {
      status: "Online",
      label: "Monsquaz Bot, The Extremely Long Server Name for Testing",
      detail:
        "Discord connection healthy, 148 ms, last heartbeat received during a very busy offer batch",
      lastSeen: "6 seconds ago",
    };
    mission.nextAction = {
      eyebrow: "Next action",
      title: "Review five deadline decisions before the next offer batch opens",
      detail:
        "Four tracks passed their deadlines and one participant asked for a second extension. The host decision changes the offer order, so resolve the queue before Fri 27 Nov, 18:00 (Europe/Stockholm).",
      actionLabel: "Review decisions",
      secondaryLabel: "Pause the event for 24 hours",
      tone: "attention",
    };
    mission.decisions = [
      ...defaultDecisions,
      {
        id: "decision-4",
        kind: "Deadline",
        title: "S2M8C is waiting for a second extension",
        detail:
          "The participant already used the automatic six-hour extension. A second extension needs a host decision and an audit note.",
        due: "25 Nov, 18:15 CET",
        actionLabel: "Review",
      },
      {
        id: "decision-5",
        kind: "Delivery",
        title: "A private-thread fallback needs approval",
        detail:
          "Direct messages are closed for one participant. The fallback thread keeps the same privacy rules and hides the mapping.",
        due: "Before the next offer",
        actionLabel: "Review",
      },
    ];
    mission.activity = [
      {
        id: "long-activity-1",
        at: "17:36",
        actor: "Alexandria-Cassandra",
        action: "submitted a new version of",
        subject: "S9Q1B",
      },
      ...defaultActivity,
      {
        id: "long-activity-7",
        at: "13:20",
        actor: "Host",
        action: "recorded an override for",
        subject: "S4K2M, fill-in sequence",
      },
      {
        id: "long-activity-8",
        at: "12:05",
        actor: "Round 8",
        action: "closed with",
        subject: "19 of 20 edits",
      },
    ];
    mission.timeline = [
      { id: "seed", label: "Seeds", caption: "40 tracks", state: "done" },
      { id: "lock", label: "Roster lock", caption: "40 active", state: "done" },
      { id: "round-8", label: "Round 8", caption: "19 edits", state: "done" },
      {
        id: "round-9",
        label: "Round 9",
        caption: "34 of 40 claimed",
        state: "current",
      },
      { id: "round-10", label: "Round 10", caption: "Next", state: "next" },
      { id: "reveal", label: "Reveal", caption: "After stream", state: "next" },
    ];
    mission.facts = [
      { label: "Mode", value: "Classic, 12 rounds" },
      {
        label: "People",
        value: "40 active, 8 fill-ins, 3 paused for travel",
      },
      { label: "Tracks", value: "20 anonymous tracks" },
      { label: "Round length", value: "36 hours" },
      { label: "Claim window", value: "18 hours" },
      { label: "Next release", value: "Fri 27 Nov, 20:00 CET" },
      {
        label: "Timezone",
        value: "America/Argentina/Buenos_Aires",
      },
    ];
    return mission;
  }

  return mission;
}

const defaultSettings: HostSettings = {
  botConfigured: true,
  gateway: {
    state: "online",
    attempt: 0,
    lastError: null,
    connectedAt: 1_764_165_600,
    handlerErrors: 0,
  },
  botIdentity: "Monsquaz Bot (1184)",
  tokenStorage: "keychain",
  guildId: "8142276038192",
  guildName: "The Night Shift",
  announcementChannelId: "8142276040001",
  announcementChannelName: "swap-announcements",
  revealChannelId: "8142276040002",
  revealChannelName: "swap-reveal",
  dmFallback: true,
  timezone: "Europe/Stockholm",
  retentionDays: 90,
  theme: "system",
  notifications: {
    deadlines: true,
    botOffline: true,
    releases: true,
  },
};

export function demoSettings(scenario: Scenario): HostSettings {
  if (scenario === "empty") {
    return {
      ...defaultSettings,
      botConfigured: false,
      botIdentity: "",
      tokenStorage: "none",
      guildId: "",
      guildName: "",
      announcementChannelId: "",
      announcementChannelName: "",
      revealChannelId: "",
      revealChannelName: "",
      theme: "system",
    };
  }
  if (scenario === "offline") {
    return {
      ...defaultSettings,
      gateway: {
        state: "failed",
        attempt: 3,
        lastError: "Discord rejected the bot token",
        connectedAt: null,
        handlerErrors: 0,
      },
    };
  }
  if (scenario === "not-started") {
    return { ...defaultSettings, gateway: undefined };
  }
  if (scenario === "reconnecting") {
    return {
      ...defaultSettings,
      gateway: {
        state: "reconnecting",
        attempt: 3,
        lastError: "Discord is not answering",
        connectedAt: null,
        handlerErrors: 0,
      },
    };
  }
  if (scenario === "long") {
    return {
      ...defaultSettings,
      botIdentity:
        "Monsquaz Bot With An Extremely Long Display Name That Keeps Going (1184)",
      guildId: "8142276038192",
      guildName:
        "The Extremely Long Server Name for Testing and Occasionally Haunting",
      announcementChannelId: "8142276040001",
      announcementChannelName: "swap-announcements-and-release-notes",
      revealChannelId: "8142276040002",
      revealChannelName: "swap-reveal-and-album-keepsakes",
      timezone: "America/Argentina/Buenos_Aires",
      retentionDays: 365,
    };
  }
  return { ...defaultSettings, theme: "system" };
}

export const demoBotPreview: BotPreview = {
  botId: "1184",
  identity: "Monsquaz Bot (1184)",
  inviteUrl:
    "https://discord.com/oauth2/authorize?client_id=1184&scope=bot%20applications.commands&permissions=117760",
};

export const demoGuilds: GuildOption[] = [
  { id: "8142276038192", name: "The Night Shift" },
  { id: "8142276038193", name: "Basement Tapes" },
];

export const demoChannels: ChannelOption[] = [
  { id: "8142276040001", name: "swap-announcements" },
  { id: "8142276040002", name: "swap-reveal" },
  { id: "8142276040003", name: "swap-chat" },
  { id: "8142276040004", name: "swap-help" },
];

const auditSeeds: Array<[string, string, string, string, string]> = [
  ["25 Nov, 17:12", "Host", "Deadline extended", "S7F3A", "Holder asked for six hours; first extension is automatic."],
  ["25 Nov, 17:04", "Bot", "Offer sent", "S4K2M", "Round 3 offer delivered in a direct message."],
  ["25 Nov, 16:48", "Noor", "Claim recorded", "S9Q1B", "Claim recorded; this person now counts as having seen this track."],
  ["25 Nov, 16:31", "Remy", "File submitted", "S2M8C", "Version 4 stored, hash checked, previous versions kept."],
  ["25 Nov, 16:02", "Host", "Round opened", "Round 3", "Ten offers sent; two participants paused."],
  ["25 Nov, 15:40", "Host", "Fill-in added", "Jules", "Fill-in added without a personal track."],
  ["25 Nov, 15:22", "Host", "Settings updated", "Retention", "Retention set to 90 days for album files."],
  ["25 Nov, 14:55", "Bot", "Round closed", "Round 2", "Eleven edits recorded. One track had no submission; the gap is anonymous."],
  ["25 Nov, 14:20", "Ari", "Skip recorded", "S2M8C", "Skip after claim; track returned unchanged."],
  ["25 Nov, 13:58", "Host", "Schedule saved", "Plan v1", "Schedule fingerprint posted. No music file was shared."],
  ["25 Nov, 12:40", "Bot", "Offer expired", "S9Q1B", "Offer expired; the participant does not count as having seen the track; the track returned."],
  ["25 Nov, 11:15", "Host", "Event paused", "All clocks", "Event paused for 24 hours; deadlines shifted."],
  ["25 Nov, 10:02", "Sam", "Claim recorded", "S7F3A", "Claim recorded after a reminder."],
  ["25 Nov, 09:31", "Host", "Audit exported", "Audit log", "CSV export for the event archive."],
];

function auditEntry(
  index: number,
  seed: [string, string, string, string, string],
  long = false,
): AuditEntry {
  const [at, actor, action, subject, reason] = seed;
  return {
    id: `audit-${index}`,
    at,
    actor,
    action,
    subject,
    reason: long
      ? `${reason} Reason recorded in the append-only archive. The hash chain prevents later edits.`
      : reason,
    code: `A-${String(1042 + index).padStart(4, "0")}`,
  };
}

export function demoAuditLog(scenario: Scenario): AuditEntry[] {
  if (scenario === "empty") {
    return [];
  }
  if (scenario === "dense") {
    return Array.from({ length: 40 }, (_, index) => {
      const seed = auditSeeds[index % auditSeeds.length]!;
      const minutes = 17 * 60 + 40 - index * 11;
      const hour = Math.floor(minutes / 60);
      const minute = minutes % 60;
      const at = `25 Nov, ${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
      return auditEntry(index, [at, seed[1], seed[2], seed[3], seed[4]], index % 5 === 0);
    });
  }
  return auditSeeds.map((seed, index) => auditEntry(index, seed, scenario === "long"));
}
import type {
  EventDraft,
  EventSummary,
  PeopleData,
  PersonRow,
  PlanData,
  PlannerGroup,
  PlannerWarning,
  SetupData,
  SetupWarning,
} from "../types";

const names = [
  "Noor",
  "Remy",
  "Sam",
  "Ari",
  "Jules",
  "Kai",
  "Mira",
  "Tove",
  "Eli",
  "Rui",
  "Sasha",
  "Wren",
  "Ada",
  "Bo",
  "Cy",
  "Dara",
  "Emre",
  "Fay",
  "Gus",
  "Hana",
  "Ines",
  "Joss",
  "Kira",
  "Lars",
  "Mina",
  "Nils",
  "Oona",
  "Pia",
  "Quinn",
  "Rosa",
  "Sten",
  "Thea",
  "Umar",
  "Vera",
  "Will",
  "Xena",
  "Yann",
  "Zoe",
  "Alva",
  "Bruno",
];

const trackCodes = [
  "S7F3A",
  "S4K2M",
  "S9Q1B",
  "S2M8C",
  "S6R1D",
  "S3T5N",
  "S8W4H",
  "S1P7J",
  "S5Y9E",
  "S0B3V",
  "S6C8L",
  "S2Z4Q",
  "S1N6R",
  "S8T2V",
  "S3H9W",
  "S6P4X",
  "S2C7Y",
  "S9D5Z",
  "S5G1Q",
  "S0J8L",
  "S4M3B",
  "S7R6E",
  "S1V9U",
  "S8K2D",
  "S3F5G",
  "S6H7J",
  "S2L4N",
  "S9P1S",
  "S5R8T",
  "S0W3X",
  "S4Y6Z",
  "S7B9C",
  "S1E2F",
  "S8G4H",
  "S3J6K",
  "S6L8M",
  "S2N1P",
  "S9Q3R",
  "S5S5T",
  "S0U7V",
];

function eventSummary(
  participants: number,
  tracks: number,
  rounds: number,
): EventSummary {
  return {
    id: "night-shift-2",
    name: "Night Shift Vol. 2",
    mode: "Classic",
    stage: "Schedule review",
    participants,
    seeded: participants,
    fillIns: Math.min(2, participants),
    tracks,
    round: 0,
    rounds,
  };
}

function defaultDraft(): EventDraft {
  return {
    name: "Night Shift Vol. 2",
    mode: "Classic",
    rounds: 8,
    claimWindowHours: 12,
    roundWindowHours: 24,
    quietHoursStart: 22,
    quietHoursEnd: 8,
    allowlist: ["it", "mptm", "xm", "mod", "s3m", "mtm", "669", "wav", "flac", "mp3", "ogg", "zip"],
    packType: "Module template + samples",
    packRequired: true,
    groupMode: "Lookup",
    groupCount: 2,
    groupSize: 6,
    rotation: "Participants",
    swapCadence: "K rounds",
    scheduleVisibility: "Hidden",
  };
}

function warning(
  id: string,
  tone: SetupWarning["tone"],
  title: string,
  detail: string,
): SetupWarning {
  return { id, tone, title, detail };
}

export function demoSetup(scenario: Scenario): SetupData {
  if (scenario === "empty") {
    return {
      firstRun: true,
      event: null,
      draft: {
        ...defaultDraft(),
        name: "",
        rounds: null,
        groupCount: 1,
        groupSize: 5,
        allowlist: [],
      },
      warnings: [],
    };
  }
  const draft = defaultDraft();
  if (scenario === "long") {
    return {
      firstRun: false,
      event: null,
      draft: {
        ...draft,
        name: longEventName,
        rounds: 12,
        groupCount: 4,
        groupSize: 10,
        swapCadence: "Calendar",
        scheduleVisibility: "Visible assignments",
      },
      warnings: [
        warning(
          "setup-long-1",
          "warn",
          "Four groups of ten",
          "Each phase gives every track ten possible contributors. The projected event lasts about three months at one release per day.",
        ),
        warning(
          "setup-long-2",
          "info",
          "Pack review is on",
          "A seed that adds an instrument or changes a pack sample is flagged for your review. It is never rejected automatically.",
        ),
      ],
    };
  }
  return {
    firstRun: false,
    event: null,
    draft,
    warnings: [
      warning(
        "setup-1",
        "info",
        "Groups are suggested from the roster size",
        "Two groups of six keep every track inside a smaller social circle. The host can override the count and size.",
      ),
      warning(
        "setup-2",
        "info",
        "Pack review is on",
        "Unknown samples are reported to the host. The seed is not rejected automatically.",
      ),
    ],
  };
}

const longFirstNames = [
  "Alexandria-Cassandra",
  "Maximilian Theodor",
  "Anastasia",
  "Bartholomew James",
  "Cecilia Margareta",
  "Dimitri Alexandrov",
  "Evangeline Rose",
  "Ferdinand",
  "Guinevere Alice",
  "Henrik Wilhelm",
  "Isabella Marie",
  "Joaquim",
  "Katarina",
  "Leopold Alexander",
  "Margareta Elisabeth",
  "Nikolai",
  "Ophelia Jane",
  "Percival",
  "Quintessa",
  "Ragnar",
];

const longLastNames = [
  "Voss",
  "Lindqvist",
  "Petrova",
  "Whitfield",
  "Holm",
  "Bergström",
  "Moreau",
  "Okafor",
];

function personRow(index: number, long = false): PersonRow {
  const name = long
    ? `${longFirstNames[index % longFirstNames.length]} ${
        longLastNames[index % longLastNames.length]
      }`
    : names[index % names.length];
  const role = index % 6 === 4 ? "Fill-in" : "Seed";
  const status = index === 3 ? "Paused" : index === 8 ? "Left" : "Ready";
  return {
    id: `user-${index.toString().padStart(2, "0")}`,
    displayName: name,
    role,
    status,
    seedTrack: role === "Seed" ? trackCodes[index % trackCodes.length] : null,
    timezone:
      long && index % 3 === 0
        ? "America/Argentina/Buenos_Aires"
        : index % 2 === 0
          ? "Europe/Stockholm"
          : "Europe/London",
    maxHandoffs: 1 + (index % 3),
    joined: `25 Nov, ${String(9 + (index % 8)).padStart(2, "0")}:${String((index * 7) % 60).padStart(2, "0")}`,
    deliveryRisk: index === 6,
  };
}

export function demoPeople(scenario: Scenario): PeopleData {
  if (scenario === "empty") {
    return { event: null, rows: [] };
  }
  const count = scenario === "dense" || scenario === "long" ? 40 : 12;
  const rows = Array.from({ length: count }, (_, index) =>
    personRow(index, scenario === "long"),
  );
  const event = eventSummary(count, count, Math.max(1, count - 1));
  return {
    event: scenario === "long" ? { ...event, name: longEventName } : event,
    rows,
  };
}

function plannerGroups(count: number): PlannerGroup[] {
  const groupCount = count >= 40 ? 4 : 2;
  const size = Math.ceil(count / groupCount);
  return Array.from({ length: groupCount }, (_, groupIndex) => ({
    name: String.fromCharCode(65 + groupIndex),
    size: Math.min(size, count - groupIndex * size),
    tracks: Math.min(size, count - groupIndex * size),
    participants: Array.from(
      { length: Math.min(size, count - groupIndex * size) },
      (_, personIndex) => names[(groupIndex * size + personIndex) % names.length]!,
    ),
  }));
}

function seededOrder(count: number, seed: number): number[] {
  let state = seed >>> 0;
  const random = () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let value = state;
    value = Math.imul(value ^ (value >>> 15), value | 1);
    value ^= value + Math.imul(value ^ (value >>> 7), value | 61);
    return ((value ^ (value >>> 14)) >>> 0) / 4294967296;
  };
  const order = Array.from({ length: count }, (_, index) => index);
  for (let index = count - 1; index > 0; index -= 1) {
    const other = Math.floor(random() * (index + 1));
    [order[index], order[other]] = [order[other]!, order[index]!];
  }
  return order;
}

function planTrackOrder(count: number): number[] {
  return seededOrder(count, 0x5eed1234);
}

function planRoundOffsets(count: number): number[] {
  return seededOrder(count - 1, 0x0badf00d).map((index) => index + 1);
}

export function demoOwnerId(trackCode: string): string | null {
  const index = trackCodes.indexOf(trackCode);
  return index === -1 ? null : `user-${index.toString().padStart(2, "0")}`;
}

function buildMatrix(count: number, rounds: number): PlanData["matrix"] {
  const groupCount = count >= 40 ? 4 : 2;
  const groupSize = Math.ceil(count / groupCount);
  const trackOrder = planTrackOrder(count);
  const offsets = planRoundOffsets(count);
  return trackOrder.map((trackIndex, position) => ({
    trackCode: trackCodes[trackIndex % trackCodes.length]!,
    ownerId: `user-${trackIndex.toString().padStart(2, "0")}`,
    ownerGroup: String.fromCharCode(65 + Math.floor(trackIndex / groupSize)),
    cells: Array.from({ length: rounds }, (_, round) => {
      const offset = offsets[round % offsets.length]!;
      const person = trackOrder[(position + offset) % count]!;
      const name = names[person % names.length]!;
      return {
        round,
        trackCode: trackCodes[trackIndex % trackCodes.length]!,
        participantId: `user-${person.toString().padStart(2, "0")}`,
        participantName: `${name} ${person + 1}`,
      };
    }),
  }));
}

function buildGraph(count: number, round: number): PlanData["graph"] {
  const trackOrder = planTrackOrder(count);
  const offsets = planRoundOffsets(count);
  return {
    round,
    edges: trackOrder.map((trackIndex, position) => {
      const offset = offsets[round % offsets.length]!;
      const person = trackOrder[(position + offset) % count]!;
      return {
        trackCode: trackCodes[trackIndex % trackCodes.length]!,
        participantName: `${names[person % names.length]!} ${person + 1}`,
      };
    }),
  };
}

function plannerWarning(
  id: string,
  severity: PlannerWarning["severity"],
  title: string,
  detail: string,
): PlannerWarning {
  return { id, severity, title, detail };
}

export function demoPlan(scenario: Scenario): PlanData {
  if (scenario === "empty") {
    return {
      status: "No event",
      event: null,
      version: 0,
      commitment: "",
      rounds: 0,
      tracks: 0,
      participants: 0,
      committed: false,
      matrix: [],
      graph: { round: 0, edges: [] },
      groups: [],
      warnings: [],
      funScore: null,
      funGuardrails: [],
    };
  }
  const dense = scenario === "dense" || scenario === "long";
  const count = dense ? 40 : 12;
  const rounds = dense ? 8 : 11;
  const event = {
    ...eventSummary(count, count, rounds),
    ...(scenario === "long" ? { name: longEventName } : {}),
  };
  if (scenario === "infeasible") {
    return {
      status: "Infeasible",
      event,
      version: 1,
      commitment: "demo-commit-1",
      rounds,
      tracks: count,
      participants: count,
      committed: false,
      matrix: buildMatrix(count, 3),
      graph: buildGraph(count, 2),
      groups: plannerGroups(count),
      warnings: [
        plannerWarning(
          "plan-infeasible-1",
          "danger",
          "Round 4 cannot be filled",
          "The planner could not fill round 4 within the group rules. These tracks could not be placed: S4K2M, S9Q1B, and S2M8C. No offers were sent and no one was marked as having seen a track.",
        ),
        plannerWarning(
          "plan-infeasible-2",
          "warn",
          "A rebuild can use the full roster",
          "The planner can rebuild the draft from all active participants. Earlier rounds can change. No pair is marked as seen until a claim.",
        ),
      ],
      funScore: null,
      funGuardrails: [
        { label: "Coverage", value: "Blocked at round 4" },
        { label: "No-repeat history", value: "Unchanged" },
      ],
    };
  }
  const warnings = dense
    ? [
        plannerWarning(
          "plan-long-1",
          "warn",
          "Four groups of ten",
          "Each track can reach up to ten contributors per round. The projected event lasts about three months.",
        ),
        plannerWarning(
          "plan-long-2",
          "info",
          "Swap cadence is two rounds",
          "The planner rotates participants four times. Tracks stay in their original group.",
        ),
      ]
    : [
        plannerWarning(
          "plan-1",
          "info",
          "Round 8 is the last full round",
          "After round 8, the planner limits how many people can contribute to each track. No participant receives a track they have seen before.",
        ),
      ];
  return {
    status: "Draft",
    event,
    version: 1,
    commitment: "demo-commit-1",
    rounds,
    tracks: count,
    participants: count,
    committed: false,
    matrix: buildMatrix(count, rounds),
    graph: buildGraph(count, 2),
    groups: plannerGroups(count),
    warnings,
    funScore: dense ? 74 : 82,
    funGuardrails: [
      { label: "Social reach", value: dense ? "Good" : "Strong" },
      { label: "Contributor variety", value: dense ? "Broad" : "Balanced" },
      { label: "Fairness", value: "Within bounds" },
      { label: "Freshness", value: dense ? "Moderate" : "High" },
    ],
  };
}


const revealSeeds = [
  ["S8G4H", "Midnight Draft", "Noor"],
  ["S1P7J", "Glass Elevator", "Fay"],
  ["S0U7V", "Paper Lanterns", "Nils"],
  ["S8T2V", "Low Tide", "Zoe"],
  ["S9D5Z", "Static Bloom", "Hana"],
  ["S6H7J", "Neon Orchard", "Kira"],
  ["S9Q3R", "Copper Wires", "Ari"],
  ["S5S5T", "Slow Machines", "Remy"],
  ["S0W3X", "Afterglow", "Jules"],
  ["S3J6K", "Blue Hour", "Sasha"],
  ["S6L8M", "Night Bus", "Ada"],
  ["S2N1P", "Soft Static", "Wen"],
] as const;

const revealEditors = [
  ["Remy", "Sam", "Ari"],
  ["Xena", "Nils", "Kira"],
  ["Ari", "Zoe", "Xena"],
  ["Joss", "Hana", "Ari"],
  ["Remy", "Kira", "Joss"],
  ["Ada", "Xena", "Remy"],
  ["Bo", "Mira", "Tove"],
  ["Eli", "Rui", "Sasha"],
  ["Wen", "Ada", "Bo"],
  ["Cy", "Dara", "Emre"],
  ["Fay", "Gus", "Hana"],
  ["Ines", "Joss", "Kira"],
] as const;

function revealTrack(index: number, long: boolean): RevealAlbumTrack {
  const [trackCode, baseTitle, seedOwner] = revealSeeds[index];
  const editors = revealEditors[index];
  const title =
    long && index === 0
      ? "Midnight Draft (The Extremely Long and Slightly Haunted Remix)"
      : baseTitle;
  const credits = editors.map((name, round) => ({
    round,
    participantId:
      round === 2 && index % 4 === 1 ? "" : `user-${name.toLowerCase()}`,
    displayName: round === 2 && index % 4 === 1 ? "No edit" : name,
    status: round === 2 && index % 4 === 1 ? "SkippedAfterClaim" : "Submitted",
    fillIn: index === 0 && round === 1,
  }));
  const submitted = credits.filter((credit) => credit.status === "Submitted");
  const versions = [
    {
      version: 1,
      holderId: `user-${seedOwner.toLowerCase()}`,
      holderName: seedOwner,
      role: "seed" as const,
      round: null,
      fileId: `demo-${trackCode.toLowerCase()}-v1`,
      extension: "it",
      createdAt: Date.UTC(2026, 10, 18, 18, 0),
      playable: true,
      finalVersion: false,
    },
    ...submitted.map((credit, position) => ({
      version: position + 2,
      holderId: `user-${credit.displayName.toLowerCase()}`,
      holderName: credit.displayName,
      role: "edit" as const,
      round: credit.round,
      fileId: `demo-${trackCode.toLowerCase()}-v${position + 2}`,
      extension: "it",
      createdAt: Date.UTC(2026, 10, 19 + position, 17, 0),
      playable: true,
      finalVersion: position + 1 === submitted.length,
    })),
  ];
  return {
    position: index + 1,
    trackCode,
    title,
    seedOwnerId: `user-${seedOwner.toLowerCase()}`,
    seedOwnerName: seedOwner,
    credits,
    comments:
      index === 0
        ? [
            {
              id: `comment-${trackCode}-1`,
              participantId: "user-remy",
              authorName: "Remy",
              kind: "comment",
              body: long
                ? "The bassline answer in the second half is the best thing I heard all week. I kept the seed melody in the bridge and let the drums drift out of phase."
                : "The bassline answer in the second half is the best thing I heard all week.",
              createdAt: Date.UTC(2026, 10, 21, 19, 12),
            },
            {
              id: `comment-${trackCode}-2`,
              participantId: "user-sam",
              authorName: "Sam",
              kind: "questionnaire",
              body: "I used the seed melody in the bridge and left the ending open.",
              createdAt: Date.UTC(2026, 10, 21, 20, 4),
            },
          ]
        : index === 2
          ? [
              {
                id: `comment-${trackCode}-1`,
                participantId: "user-ari",
                authorName: "Ari",
                kind: "comment",
                body: "The tempo lift at the last pattern made me smile.",
                createdAt: Date.UTC(2026, 10, 22, 18, 40),
              },
            ]
          : [],
    versions,
    stats: {
      edits: submitted.length,
      fillIns: credits.filter((credit) => credit.fillIn).length,
      skips: credits.filter((credit) => credit.status !== "Submitted").length,
      contributors: new Set([seedOwner, ...submitted.map((credit) => credit.displayName)])
        .size,
    },
  };
}

export function demoRevealPackage(scenario: Scenario): RevealPackage {
  if (scenario === "empty") {
    return {
      eventName: "Night Shift Vol. 2",
      eventState: "Active",
      published: false,
      album: [],
    };
  }
  const count = scenario === "dense" ? 12 : 6;
  const long = scenario === "long";
  return {
    eventName: long ? longEventName : "Night Shift Vol. 2",
    eventState: "Active",
    published: false,
    album: Array.from({ length: count }, (_, index) => revealTrack(index, long)),
  };
}
