import type {
  ActivityEntry,
  AuditEntry,
  Decision,
  HostSettings,
  MissionControl,
  Scenario,
  TimelineStep,
} from "../types";

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
    title: "S7F3A passed its deadline",
    detail:
      "The current holder still has the track. Other participants cannot see it until the reveal. Decide whether to extend, skip, or wait.",
    due: "42 minutes ago",
    actionLabel: "Review",
  },
  {
    id: "decision-2",
    kind: "Offer",
    title: "One offer expires in 3 hours",
    detail:
      "S4K2M has not been claimed. If the offer expires, the track returns to the pool and the pair stays unseen.",
    due: "25 Nov · 17:40 CET",
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
    subject: "S7F3A · +6 hours",
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
    { label: "Mode", value: "Classic · 11 rounds" },
    { label: "People", value: "12 active · 2 fill-ins" },
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
      detail: "Discord connection healthy · 82 ms",
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
      detail: "Discord connection is down · offers are paused",
      lastSeen: "18 minutes ago",
    };
    mission.nextAction = {
      eyebrow: "Next action",
      title: "Reconnect the bot",
      detail:
        "The queue is safe. Reconnect before the next batch so offers can go out.",
      actionLabel: "Reconnect now",
      secondaryLabel: "Run connection test",
      tone: "attention",
    };
    return mission;
  }

  if (scenario === "long") {
    mission.event = {
      id: "long-event",
      name: "The Extremely Long and Slightly Haunted Midwinter Remix Exchange, Volume Two",
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
      label: "Monsquaz Bot · The Extremely Long Server Name for Testing",
      detail:
        "Discord connection healthy · 148 ms · last heartbeat received during a very busy offer batch",
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
        due: "25 Nov · 18:15 CET",
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
        subject: "S4K2M · fill-in sequence",
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
      { label: "Mode", value: "Classic · 12 rounds" },
      {
        label: "People",
        value: "40 active · 8 fill-ins · 3 paused for travel",
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
  botIdentity: "Monsquaz Bot · application 1184",
  guildName: "The Night Shift",
  announcementChannel: "#swap-announcements",
  revealChannel: "#swap-reveal",
  dmFallback: true,
  timezone: "Europe/Stockholm",
  retentionDays: 90,
  theme: "dark",
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
      guildName: "",
      announcementChannel: "",
      revealChannel: "",
      theme: "dark",
    };
  }
  if (scenario === "long") {
    return {
      ...defaultSettings,
      botIdentity:
        "Monsquaz Bot · application 1184 · The Extremely Long Server Name for Testing",
      guildName:
        "The Extremely Long Server Name for Testing and Occasionally Haunting",
      announcementChannel: "#swap-announcements-and-release-notes",
      revealChannel: "#swap-reveal-and-album-keepsakes",
      timezone: "America/Argentina/Buenos_Aires",
      retentionDays: 365,
    };
  }
  return { ...defaultSettings, theme: "dark" };
}

const auditSeeds: Array<[string, string, string, string, string]> = [
  ["25 Nov · 17:12", "Host", "Deadline extended", "S7F3A", "Holder asked for six hours; first extension is automatic."],
  ["25 Nov · 17:04", "Bot", "Offer sent", "S4K2M", "Round 3 offer delivered in a direct message."],
  ["25 Nov · 16:48", "Noor", "Claim recorded", "S9Q1B", "Claim recorded; seen pair written to the ledger."],
  ["25 Nov · 16:31", "Remy", "File submitted", "S2M8C", "Version 4 stored, hash checked, previous versions kept."],
  ["25 Nov · 16:02", "Host", "Round opened", "Round 3", "Ten offers sent; two participants paused."],
  ["25 Nov · 15:40", "Host", "Fill-in added", "Jules", "Fill-in added without a personal track."],
  ["25 Nov · 15:22", "Host", "Settings updated", "Retention", "Retention set to 90 days for album files."],
  ["25 Nov · 14:55", "Bot", "Round closed", "Round 2", "Eleven edits recorded; one anonymous gap."],
  ["25 Nov · 14:20", "Ari", "Skip recorded", "S2M8C", "Skip after claim; track returned unchanged."],
  ["25 Nov · 13:58", "Host", "Schedule saved", "Plan v1", "Commitment posted without any file hash."],
  ["25 Nov · 12:40", "Bot", "Offer expired", "S9Q1B", "Offer expired; pair stays unseen and the track returned."],
  ["25 Nov · 11:15", "Host", "Event paused", "All clocks", "Event paused for 24 hours; deadlines shifted."],
  ["25 Nov · 10:02", "Sam", "Claim recorded", "S7F3A", "Claim recorded after a reminder."],
  ["25 Nov · 09:31", "Host", "Audit exported", "Audit log", "CSV export for the event archive."],
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
      ? `${reason} The host recorded the full reason so the event archive explains every override and every change without guessing.`
      : reason,
    code: `A-${String(1042 + index).padStart(4, "0")}`,
  };
}

export function demoAuditLog(scenario: Scenario): AuditEntry[] {
  if (scenario === "empty") {
    return [];
  }
  if (scenario === "dense") {
    return Array.from({ length: 40 }, (_, index) =>
      auditEntry(
        index,
        auditSeeds[index % auditSeeds.length]!,
        index % 5 === 0,
      ),
    );
  }
  return auditSeeds.map((seed, index) => auditEntry(index, seed, scenario === "long"));
}
