import { useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import "./App.css";
import {
  Button,
  Dialog,
  ErrorState,
  Field,
  StatusPill,
} from "./components";
import { Icon, type IconName } from "./icons";
import { CommandPalette } from "./CommandPalette";
import type {
  AuditEntry,
  EventDraft,
  HostSettings,
  MissionControl,
  PeopleData,
  PlanData,
  RevealPackage,
  Scenario,
  SetupData,
  Theme,
  ViewKey,
  Decision,
} from "./types";
import {
  exportAuditCsv,
  HostError,
  initialDialog,
  initialScenario,
  initialView,
  isDemoMode,
  createEvent,
  decideDeadline,
  decideExpiry,
  decideExtension,
  decideLateSeed,
  dispatchAsyncWave,
  extendSeedDeadline,
  uploadPack,
  openSeedRound,
  editPlanAssignment,
  generatePlan,
  loadPlanEditCandidates,
  lockRoster,
  moveRoundDeadline,
  openRound,
  revealEvent,
  exportRevealZip,
  repairGroups,
  startAsyncEvent,
  startEvent,
  loadAuditLog,
  loadFillInCandidates,
  loadMissionControl,
  loadPeople,
  loadPlan,
  loadRevealPackage,
  loadSettings,
  loadSetup,
  saveSettings,
  setParticipantPaused,
  markParticipantLeft,
  restoreParticipant,
  setTrackTitle,
  addAvailability,
  assignFillIn,
  startFillInList,
  removeAvailability,
  addTrackComment,
  redactTrackComment,
  sendTestNotification,
} from "./data/client";
import {
  MissionControlSkeleton,
  MissionControlView,
} from "./views/MissionControl";
import { SettingsSkeleton, SettingsView } from "./views/Settings";
import { BotSetupDialog } from "./views/BotSetup";
import { AuditLogSkeleton, AuditLogView } from "./views/AuditLog";
import { SetupSkeleton, SetupView } from "./views/Setup";
import { PeopleSkeleton, PeopleView } from "./views/People";
import { PlannerSkeleton, PlannerView } from "./views/Planner";
import { RevealSkeleton, RevealView } from "./views/Reveal";

type LoadStatus = "loading" | "ready" | "error";

const navItems: Array<{ view: ViewKey; label: string; icon: IconName }> = [
  { view: "mission", label: "Mission control", icon: "compass" },
  { view: "audit", label: "Audit log", icon: "audit" },
  { view: "settings", label: "Settings", icon: "settings" },
];

const toolItems: Array<{
  label: string;
  icon: IconName;
  view?: ViewKey;
}> = [
  { label: "Event setup", icon: "plus", view: "setup" },
  { label: "Participants", icon: "users", view: "people" },
  { label: "Schedule", icon: "shuffle", view: "plan" },
  { label: "Reveal show", icon: "sparkles", view: "reveal" },
  { label: "Round board", icon: "disc" },
  { label: "Files", icon: "folder" },
];

export default function App() {
  const scenario = useMemo<Scenario>(() => initialScenario(), []);
  const demo = useMemo(() => isDemoMode(), []);
  const staticMode = useMemo(
    () => new URLSearchParams(window.location.search).has("static"),
    [],
  );
  const [view, setView] = useState<ViewKey>(() => initialView());
  const [status, setStatus] = useState<LoadStatus>("loading");
  const [mission, setMission] = useState<MissionControl | null>(null);
  const [setup, setSetup] = useState<SetupData | null>(null);
  const [people, setPeople] = useState<PeopleData | null>(null);
  const [plan, setPlan] = useState<PlanData | null>(null);
  const [revealPackage, setRevealPackage] = useState<RevealPackage | null>(null);
  const [streamSafe, setStreamSafe] = useState(() => {
    const stored = window.sessionStorage.getItem("msw.reveal.streamSafe");
    if (stored === "0") {
      return false;
    }
    if (stored === "1") {
      return true;
    }
    return (
      new URLSearchParams(window.location.search).get("stream") !== "0"
    );
  });
  const [settings, setSettings] = useState<HostSettings | null>(null);
  const [originalSettings, setOriginalSettings] =
    useState<HostSettings | null>(null);
  const [audit, setAudit] = useState<AuditEntry[] | null>(null);
  const [dirty, setDirty] = useState(
    () =>
      isDemoMode() &&
      new URLSearchParams(window.location.search).get("dirty") === "1",
  );
  const [saving, setSaving] = useState(false);
  const [reloadToken, setReloadToken] = useState(0);
  const reloadFocus = useRef(false);
  const focusMainHeading = () => {
    window.requestAnimationFrame(() => {
      document.querySelector<HTMLElement>("#main h1")?.focus();
    });
  };
  const [errorCode, setErrorCode] = useState("MSW-UI-001");
  const [themeChoice, setThemeChoice] = useState<HostSettings["theme"]>(() => {
    const value = new URLSearchParams(window.location.search).get("theme");
    return value === "light" || value === "dark" ? value : "system";
  });
  const [systemTheme, setSystemTheme] = useState<Theme>("dark");
  const [toast, setToast] = useState<string | null>(null);
  const [dialogOpen, setDialogOpen] = useState(() => initialDialog() === "delete");
  const [botDialogOpen, setBotDialogOpen] = useState(
    () => new URLSearchParams(window.location.search).get("dialog") === "bot",
  );
  const [confirmText, setConfirmText] = useState("");
  const [redactionPreview, setRedactionPreview] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(
    () =>
      isDemoMode() &&
      new URLSearchParams(window.location.search).get("palette") === "1",
  );

  const resolvedTheme: Theme =
    themeChoice === "system" ? systemTheme : themeChoice;
  const firstView = useRef(true);

  useEffect(() => {
    document.documentElement.dataset.theme = resolvedTheme;
  }, [resolvedTheme]);

  useEffect(() => {
    window.sessionStorage.setItem(
      "msw.reveal.streamSafe",
      streamSafe ? "1" : "0",
    );
  }, [streamSafe]);

  useEffect(() => {
    const media = window.matchMedia?.("(prefers-color-scheme: light)");
    if (!media) {
      return;
    }
    setSystemTheme(media.matches ? "light" : "dark");
    const listener = (event: MediaQueryListEvent) => {
      setSystemTheme(event.matches ? "light" : "dark");
    };
    media.addEventListener("change", listener);
    return () => media.removeEventListener("change", listener);
  }, []);

  useEffect(() => {
    let cancelled = false;
    setStatus("loading");
    const queryTheme = new URLSearchParams(window.location.search).get("theme");
    Promise.all([
      loadMissionControl(scenario),
      loadSettings(scenario),
      loadAuditLog(scenario),
      loadSetup(scenario),
      loadPeople(scenario),
      loadPlan(scenario),
      loadRevealPackage(scenario),
    ])
      .then(
        ([
          nextMission,
          nextSettings,
          nextAudit,
          nextSetup,
          nextPeople,
          nextPlan,
          nextReveal,
        ]) => {
        if (cancelled) {
          return;
        }
        if (queryTheme === "light" || queryTheme === "dark") {
          nextSettings = { ...nextSettings, theme: queryTheme };
          setThemeChoice(queryTheme);
        } else {
          setThemeChoice(nextSettings.theme);
        }
        setMission(nextMission);
        setSettings(nextSettings);
        setOriginalSettings(nextSettings);
        setAudit(nextAudit);
        setSetup(nextSetup);
        setPeople(nextPeople);
        setPlan(nextPlan);
        setRevealPackage(nextReveal);
        setDirty(
          isDemoMode() &&
            new URLSearchParams(window.location.search).get("dirty") === "1",
        );
        setStatus("ready");
        },
      )
      .catch((error: unknown) => {
        if (cancelled) {
          return;
        }
        setErrorCode(error instanceof HostError ? error.code : "MSW-UI-002");
        setStatus("error");
        if (reloadFocus.current) {
          reloadFocus.current = false;
          focusMainHeading();
        }
      });
    return () => {
      cancelled = true;
    };
  }, [reloadToken, scenario]);

  useEffect(() => {
    const titles: Record<ViewKey, string> = {
      mission: "Mission control",
      setup: "Event setup",
      people: "Participants",
      plan: "Schedule planner",
      reveal: "Reveal show",
      settings: "Settings",
      audit: "Audit log",
    };
    document.title = `${titles[view]}, Monsquaz Swap`;
    if (firstView.current) {
      firstView.current = false;
      return;
    }
    document.querySelector<HTMLElement>("#main h1")?.focus();
  }, [view]);

  useEffect(() => {
    if (status === "ready" && reloadFocus.current) {
      reloadFocus.current = false;
      focusMainHeading();
    }
  }, [status]);

  useEffect(() => {
    if (view !== "plan") {
      setRedactionPreview(false);
    }
  }, [view]);

  useEffect(() => {
    document.body.classList.toggle(
      "no-scroll",
      dialogOpen || botDialogOpen || paletteOpen,
    );
    return () => document.body.classList.remove("no-scroll");
  }, [dialogOpen, botDialogOpen, paletteOpen]);

  useEffect(() => {
    if (view !== "mission") {
      return;
    }
    const timer = window.setInterval(() => {
      setReloadToken((value) => value + 1);
    }, 30_000);
    return () => window.clearInterval(timer);
  }, [view]);

  useEffect(() => {
    if (view !== "reveal" || isDemoMode()) {
      return;
    }
    const timer = window.setInterval(() => {
      loadRevealPackage(scenario)
        .then(setRevealPackage)
        .catch(() => undefined);
    }, 30_000);
    return () => window.clearInterval(timer);
  }, [view, scenario]);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      const modal = document.querySelector(
        '[role="dialog"][aria-modal="true"]',
      );
      if (modal && !modal.classList.contains("palette")) {
        return;
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen((value) => !value);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);


  function reloadView() {
    reloadFocus.current = true;
    setReloadToken((value) => value + 1);
  }

  async function notify(label: string) {
    if (label === "Set up Round 1") {
      const heading = document.getElementById("seed-round-title");
      heading?.scrollIntoView({ block: "start" });
      setToast("Choose the pack and the seed deadline, then open Round 1.");
      return;
    }
    if (label === "Open event setup" || label === "Start setup") {
      setView("setup");
      return;
    }
    if (label === "Open schedule planner") {
      setView("plan");
      return;
    }
    if (label === "Review participants") {
      setView("people");
      return;
    }
    if (
      label === "Open settings" ||
      label === "Connection settings" ||
      label === "Test connection" ||
      label === "Reconnect now"
    ) {
      setView("settings");
      return;
    }
    if (label === "Review decisions") {
      const heading = document.getElementById("decisions-title");
      heading?.scrollIntoView({ block: "start" });
      heading?.focus();
      setToast("Review the decisions below. Nothing moves until you choose.");
      return;
    }
    if (label === "Start async queue") {
      try {
        await startAsyncEvent();
        setToast("The async queue is open. Dispatch the first wave when ready.");
        reloadView();
      } catch (error) {
        setToast(`Could not start the queue. ${errorDetail(error)}`);
      }
      return;
    }
    if (label === "Dispatch async wave") {
      try {
        const result = await dispatchAsyncWave();
        setToast(`Wave dispatched. ${result.offers} offers queued.`);
        reloadView();
      } catch (error) {
        setToast(`Could not dispatch the wave. ${errorDetail(error)}`);
      }
      return;
    }
    if (label === "Lock roster") {
      try {
        const result = await lockRoster();
        setToast(
          `Roster locked. ${result.active} active, ${result.fillIns} fill-in.`,
        );
        reloadView();
      } catch (error) {
        setToast(`Could not lock the roster. ${errorDetail(error)}`);
      }
      return;
    }
    const round = /^Open Round (\d+)$/.exec(label);
    if (round) {
      const roundNo = Number(round[1]);
      if (roundNo < 2) {
        setToast("Round 1 is the seed round. Open it from event setup.");
        setView("setup");
        return;
      }
      try {
        const opened = await openRound(roundNo - 2);
        setToast(`Round ${roundNo} opened. ${opened.offers} offers queued.`);
        reloadView();
      } catch (error) {
        setToast(`Could not open the round. ${errorDetail(error)}`);
      }
      return;
    }
    if (label === "Reveal event") {
      try {
        const revealed = await revealEvent();
        if (!revealed.published) {
          setToast(
            `Reveal blocked by ${revealed.blocked.length} active assignment(s). Resolve them first.`,
          );
        } else {
          setToast(
            `Reveal recorded. ${revealed.tracks.length} track credits are queued for the reveal channel.`,
          );
          setView("reveal");
        }
        reloadView();
      } catch (error) {
        setToast(`Could not reveal the event. ${errorDetail(error)}`);
      }
      return;
    }
    setToast(
      demo
        ? `Preview: “${label}” does not change Discord yet.`
        : `“${label}” is not wired in this build.`,
    );
  }

  function errorDetail(error: unknown) {
    return typeof error === "string"
      ? error
      : error instanceof Error
        ? error.message
        : "The command failed.";
  }

  async function handleGeneratePlan() {
    if (demo) {
      setToast("Preview: a draft schedule is generated from the seed ledger.");
      return;
    }
    try {
      await generatePlan();
      setToast("Schedule generated. Review the matrix before you lock it.");
      reloadView();
    } catch (error) {
      const detail =
        typeof error === "string"
          ? error
          : error instanceof Error
            ? error.message
            : "Check the roster and seeds.";
      setToast(`Could not generate the schedule. ${detail}`);
    }
  }

  async function handleRepairGroups() {
    if (demo) {
      setToast("Preview: groups would merge or re-split before the plan is locked.");
      return;
    }
    try {
      const next = await repairGroups();
      setPlan(next);
      setToast(`Groups repaired. Plan v${next.version}.`);
    } catch (error) {
      setToast(`Could not repair groups. ${errorDetail(error)}`);
    }
  }

  async function handleEditAssignment(
    round: number,
    trackCode: string,
    participantId: string,
    expectedVersion: number,
  ) {
    const next = await editPlanAssignment(
      round,
      trackCode,
      participantId,
      expectedVersion,
    );
    setPlan(next);
    setToast("Schedule saved as a new version.");
  }

  async function handleSetPaused(userId: string, paused: boolean) {
    if (demo) {
      setToast(
        paused
          ? "Preview: the participant is paused. Offers skip them."
          : "Preview: the participant is ready for offers.",
      );
      return;
    }
    try {
      await setParticipantPaused(userId, paused);
      setToast(paused ? "Participant paused." : "Participant resumed.");
      reloadView();
    } catch (error) {
      const detail =
        typeof error === "string"
          ? error
          : error instanceof Error
            ? error.message
            : "The participant was not changed.";
      setToast(`Could not update the participant. ${detail}`);
      reloadView();
    }
  }

  async function handleLeaveChange(userId: string, leave: boolean) {
    if (demo) {
      setToast(
        leave
          ? "Preview: the participant left the event. Any held track returns unchanged."
          : "Preview: the bot asks the participant to confirm the return.",
      );
      return;
    }
    try {
      if (leave) {
        await markParticipantLeft(userId);
        setToast("The participant left the event. Any held track returned unchanged.");
      } else {
        await restoreParticipant(userId);
        setToast("The bot asks the participant to confirm the return.");
      }
      reloadView();
    } catch (error) {
      setToast(`Could not update the participant. ${errorDetail(error)}`);
      reloadView();
    }
  }

  async function handleAddAvailability(
    participantId: string,
    startsAt: number,
    endsAt: number,
    note: string,
  ) {
    try {
      await addAvailability(participantId, startsAt, endsAt, note);
      setToast(
        "Away range saved. Regenerate the draft schedule to apply it.",
      );
      reloadView();
    } catch (error) {
      setToast(`Could not save the away range. ${errorDetail(error)}`);
    }
  }

  async function handleRemoveAvailability(id: string) {
    try {
      await removeAvailability(id);
      setToast("Away range removed.");
      reloadView();
    } catch (error) {
      setToast(`Could not remove the away range. ${errorDetail(error)}`);
    }
  }

  async function handleLoadFillInCandidates(
    trackCode: string,
    roundNo: number,
  ) {
    return loadFillInCandidates(trackCode, roundNo);
  }

  async function handleAssignFillIn(
    trackCode: string,
    roundNo: number,
    participantId: string,
    dueAt?: number,
  ) {
    try {
      await assignFillIn(trackCode, roundNo, participantId, dueAt);
      setToast("Fill-in assigned. The offer is queued.");
      reloadView();
    } catch (error) {
      setToast(`Could not assign the fill-in. ${errorDetail(error)}`);
    }
  }

  async function handleStartFillInList(
    trackCode: string,
    roundNo: number,
    participantIds: string[],
  ) {
    try {
      const result = await startFillInList(trackCode, roundNo, participantIds);
      setToast(
        result.offeredTo
          ? "Fill-in list saved. The bot offers the track in that order."
          : result.status === "exhausted"
            ? result.decisionOpened
              ? "The fill-in list is exhausted. A host decision is open."
              : "The fill-in list is exhausted. The track passes unchanged."
            : "Fill-in list saved. The bot waits for quiet hours to end, then offers the track.",
      );
      reloadView();
    } catch (error) {
      setToast(`Could not save the fill-in list. ${errorDetail(error)}`);
    }
  }

  async function handleDecision(decision: Decision, action: string) {
    if (demo) {
      setToast(`Preview: the host action “${action}” is applied to this decision.`);
      return;
    }
    try {
      if (decision.kind === "Expiry" && decision.decisionId) {
        await decideExpiry(decision.decisionId, action);
        setToast(
          action === "reoffer"
            ? "The same track is offered to the participant again."
            : action === "pause"
              ? "Participant paused until a resume or a host action."
              : action === "cooldown"
                ? "The participant has a 24-hour cooldown."
                : "The expiry is closed with no change.",
        );
      } else if (decision.kind === "Extension" && decision.decisionId) {
        const status = await decideExtension(
          decision.decisionId,
          action === "approve",
          "Host decision",
        );
        setToast(
          status === "approved"
            ? "Extension approved."
            : status === "pending"
              ? "The extension request is still open."
              : "Extension refused.",
        );
      } else if (decision.assignmentId) {
        const hours = action === "extend" ? 6 : 0;
        await decideDeadline(
          decision.assignmentId,
          action,
          hours,
          "Host decision",
        );
        setToast(
          action === "missed"
            ? "Turn marked missed. The pair counts as seen."
            : action === "extend"
              ? "Deadline extended by six hours."
              : "Deadline kept open.",
        );
      }
      reloadView();
    } catch (error) {
      setToast(`Could not apply the decision. ${errorDetail(error)}`);
      reloadView();
    }
  }

  async function handleStartEvent() {
    if (demo) {
      setToast("Preview: the schedule is locked and the event starts.");
      return;
    }
    try {
      await startEvent();
      setToast("Schedule locked. The event is live.");
      reloadView();
    } catch (error) {
      const detail =
        typeof error === "string"
          ? error
          : error instanceof Error
            ? error.message
            : "Check the roster and schedule.";
      setToast(`Could not start the event. ${detail}`);
    }
  }

  async function handleExportReveal() {
    const path = await saveDialog({
      defaultPath: "monsquaz-reveal.zip",
      filters: [{ name: "ZIP", extensions: ["zip"] }],
    });
    if (!path) {
      return;
    }
    try {
      await exportRevealZip(path);
      setToast(`Reveal ZIP saved to ${path}.`);
    } catch (error) {
      setToast(`Could not export the reveal. ${errorDetail(error)}`);
    }
  }

  async function handleCreateEvent(draft: EventDraft) {
    try {
      const created = await createEvent(draft);
      setToast(
        demo
          ? `Preview: ${created.name} is ready. No Discord message was sent.`
          : `${created.name} is ready.`,
      );
      setView("mission");
      reloadView();
    } catch {
      setToast("Could not create the event. Your draft is still here.");
    }
  }

  async function handleUploadPack(path: string) {
    return uploadPack(path);
  }

  async function handleOpenSeedRound(deadlineAt: number, channelId: string | null) {
    try {
      await openSeedRound(deadlineAt, channelId);
      setToast("Round 1 is open. The pack is queued for delivery.");
      reloadView();
    } catch (error) {
      setToast(`Could not open Round 1. ${errorDetail(error)}`);
    }
  }

  async function handleCloseSeedRound() {
    try {
      const result = await lockRoster();
      setToast(
        `Seed round closed. ${result.active} active, ${result.fillIns} fill-in.`,
      );
      reloadView();
    } catch (error) {
      setToast(`Could not close the seed round. ${errorDetail(error)}`);
    }
  }

  async function handleExtendSeedDeadline(deadlineAt: number) {
    try {
      await extendSeedDeadline(deadlineAt);
      setToast("Seed deadline moved. The new time is in the audit log.");
      reloadView();
    } catch (error) {
      setToast(`Could not move the seed deadline. ${errorDetail(error)}`);
    }
  }

  async function handleDecideLateSeed(id: string, accept: boolean) {
    try {
      await decideLateSeed(id, accept, accept ? "host approved" : "host declined");
      setToast(accept ? "Late seed accepted." : "Late seed declined.");
      reloadView();
    } catch (error) {
      setToast(`Could not decide the late seed. ${errorDetail(error)}`);
    }
  }

  async function handleMoveRoundDeadline(
    roundNo: number,
    deadlineAt: number,
    reason: string,
  ) {
    try {
      await moveRoundDeadline(roundNo, deadlineAt, reason);
      setToast(`Round ${roundNo} deadline moved. Holders were notified.`);
      reloadView();
    } catch (error) {
      setToast(`Could not move the deadline. ${errorDetail(error)}`);
    }
  }

  function changeTheme(next: HostSettings["theme"]) {
    setThemeChoice(next);
    if (settings) {
      setSettings({ ...settings, theme: next });
      setDirty(true);
    }
  }

  function toggleTheme() {
    changeTheme(resolvedTheme === "dark" ? "light" : "dark");
  }

  async function persistSettings() {
    if (!settings || saving) {
      return;
    }
    setSaving(true);
    try {
      const saved = await saveSettings(settings);
      const withHealth = { ...saved, gateway: settings.gateway };
      setSettings(withHealth);
      setOriginalSettings(withHealth);
      setDirty(false);
      setToast("Settings saved on this computer.");
      focusMainHeading();
    } catch {
      setToast("Could not save settings. Your changes are still here. Retry.");
    } finally {
      setSaving(false);
    }
  }

  function resetSettings() {
    if (!originalSettings) {
      return;
    }
    setSettings(originalSettings);
    setThemeChoice(originalSettings.theme);
    setDirty(false);
    setToast("Settings reset to the last saved values.");
    focusMainHeading();
  }

  function retry() {
    if (scenario === "error") {
      setStatus("error");
      return;
    }
    reloadView();
  }

  const topbarLabel =
    status === "loading"
      ? "Loading"
      : status === "error"
        ? "Connection problem"
        : mission?.event
          ? "Current event"
          : "No event yet";
  const topbarTitle =
    status === "error"
      ? "Local service unavailable"
      : mission?.event?.name ??
        (status === "loading" ? "Loading event…" : "Choose an event");

  const content: ReactNode =
    status === "loading" ? (
      view === "mission" ? (
        <MissionControlSkeleton />
      ) : view === "setup" ? (
        <SetupSkeleton />
      ) : view === "people" ? (
        <PeopleSkeleton />
      ) : view === "plan" ? (
        <PlannerSkeleton />
      ) : view === "reveal" ? (
        <RevealSkeleton />
      ) : view === "settings" ? (
        <SettingsSkeleton />
      ) : (
        <AuditLogSkeleton />
      )
    ) : status === "error" ? (
      <div className="view">
        <ErrorState code={errorCode} onRetry={retry} />
      </div>
    ) : view === "mission" && mission ? (
      <MissionControlView
        data={mission}
        timezone={settings?.timezone}
        onAction={notify}
        onDecision={handleDecision}
        onNavigate={(next) => setView(next)}
        onUploadPack={handleUploadPack}
        onOpenSeedRound={handleOpenSeedRound}
        onCloseSeedRound={handleCloseSeedRound}
        onExtendSeedDeadline={handleExtendSeedDeadline}
        onDecideLateSeed={handleDecideLateSeed}
        onMoveRoundDeadline={handleMoveRoundDeadline}
        onLoadFillInCandidates={handleLoadFillInCandidates}
        onAssignFillIn={handleAssignFillIn}
        onStartFillInList={handleStartFillInList}
      />
    ) : view === "setup" && setup ? (
      <SetupView
        data={setup}
        timezone={settings?.timezone}
        onCreate={handleCreateEvent}
      />
    ) : view === "people" && people ? (
      <PeopleView
        data={people}
        onAction={notify}
        onSetPaused={handleSetPaused}
        onLeaveChange={handleLeaveChange}
        onAddAvailability={handleAddAvailability}
        onRemoveAvailability={handleRemoveAvailability}
        timezone={settings?.timezone}
      />
    ) : view === "plan" && plan ? (
      <PlannerView
        data={plan}
        onAction={notify}
        onGeneratePlan={handleGeneratePlan}
        onEditAssignment={handleEditAssignment}
        onLoadCandidates={loadPlanEditCandidates}
        onRepairGroups={handleRepairGroups}
        onStartEvent={handleStartEvent}
        onRedactionChange={setRedactionPreview}
      />
    ) : view === "reveal" && revealPackage ? (
      <RevealView
        data={revealPackage}
        streamSafe={streamSafe}
        onStreamSafeChange={setStreamSafe}
        onAction={setToast}
        onExport={handleExportReveal}
        onSetTitle={async (trackCode, title) => {
          try {
            setRevealPackage(await setTrackTitle(trackCode, title));
          } catch (error) {
            setToast(`Could not save the title. ${errorDetail(error)}`);
            throw error;
          }
        }}
        onAddComment={async (trackCode, participantId, kind, body) => {
          try {
            setRevealPackage(
              await addTrackComment(trackCode, participantId, kind, body),
            );
          } catch (error) {
            setToast(`Could not add the comment. ${errorDetail(error)}`);
            throw error;
          }
        }}
        onRedact={async (commentId) => {
          try {
            setRevealPackage(await redactTrackComment(commentId));
          } catch (error) {
            setToast(`Could not redact the comment. ${errorDetail(error)}`);
            throw error;
          }
        }}
        onPublish={async () => {
          try {
            const revealed = await revealEvent();
            if (!revealed.published) {
              return { published: false, blocked: revealed.blocked };
            }
            setRevealPackage({
              ...(await loadRevealPackage(scenario)),
              published: true,
            });
            setView("reveal");
            return { published: true, blocked: [] };
          } catch (error) {
            setToast(`Could not reveal the event. ${errorDetail(error)}`);
            throw error;
          }
        }}
      />
    ) : view === "settings" && settings ? (
      <SettingsView
        settings={settings}
        eventName={mission?.event?.name ?? "this event"}
        dirty={dirty}
        onSettings={(next) => {
          setSettings(next);
          setDirty(true);
        }}
        onSave={persistSettings}
        onDelete={() => {
          setConfirmText("");
          setDialogOpen(true);
        }}
        onTheme={changeTheme}
        onConfigureBot={() => setBotDialogOpen(true)}
        onDiscard={resetSettings}
        onTestNotification={async () => {
          try {
            setToast(await sendTestNotification());
          } catch {
            setToast("Could not send the test notification.");
          }
        }}
      />
    ) : view === "audit" && audit ? (
      <AuditLogView
        entries={audit}
        hasEvent={Boolean(mission?.event)}
        timezone={settings?.timezone ?? "Europe/Stockholm"}
        onExport={() => {
          exportAuditCsv(audit);
          setToast("Audit log exported as CSV.");
        }}
        onNavigate={() => setView(mission?.event ? "mission" : "setup")}
      />
    ) : (
      <div className="view">
        <ErrorState code={errorCode} onRetry={retry} />
      </div>
    );

  return (
    <>
      <a className="skip-link" href="#main">
        Skip to content
      </a>
      <div
        className={`app${staticMode ? " app--static" : ""}${
          redactionPreview ? " app--redaction" : ""
        }`}
        data-app-status={status}
        data-theme={resolvedTheme}
      >
        <Sidebar
          view={view}
          onNavigate={setView}
          mission={mission}
          status={status}
        />
        <div className="workspace">
          <header className="topbar">
            <div className="event-switcher event-switcher--static">
              <span className="event-switcher__copy">
                <span className="event-switcher__label">{topbarLabel}</span>
                <strong title={topbarTitle}>{topbarTitle}</strong>
              </span>
            </div>
            <div className="topbar__actions">
              <button
                type="button"
                className="command-trigger"
                aria-label="Jump to"
                aria-keyshortcuts="Control+K"
                title="Jump to (Ctrl K)"
                onClick={() => setPaletteOpen(true)}
              >
                <Icon name="search" size={16} />
                <span>Jump to</span>
                <kbd>Ctrl K</kbd>
              </button>
              {demo ? (
                <span
                  className="preview-pill"
                  title="This window uses sample data."
                >
                  Demo data
                </span>
              ) : null}
              <button
                type="button"
                className="icon-button"
                aria-label={
                  resolvedTheme === "dark"
                    ? "Switch to light theme"
                    : "Switch to dark theme"
                }
                title={
                  resolvedTheme === "dark"
                    ? "Switch to light theme"
                    : "Switch to dark theme"
                }
                onClick={toggleTheme}
              >
                <Icon name={resolvedTheme === "dark" ? "sun" : "moon"} size={18} />
              </button>
              <button
                type="button"
                className="icon-button"
                aria-label="Open help"
                title="Open help"
                onClick={() => notify("Help")}
              >
                <Icon name="help" size={18} />
              </button>
            </div>
          </header>
          <main id="main" className="content" tabIndex={-1}>
            {content}
          </main>
          <footer className="statusbar">
            <span className="statusbar__item">SQLite, local</span>
            <span className="statusbar__item statusbar__bot">
              {mission?.bot?.status === "Online"
                ? "Bot online"
                : mission?.bot?.status === "Connecting"
                  ? "Bot connecting"
                  : mission?.bot?.status === "Reconnecting"
                    ? "Bot reconnecting"
                  : mission?.bot?.status === "Offline"
                    ? "Bot offline"
                    : "No bot"}
            </span>
            <span className="statusbar__spacer" />
            <span className="statusbar__item">
              {view === "plan" && plan
                ? `Plan: ${plan.status}`
                : mission?.event?.stage
                  ? `Event: ${mission.event.stage}`
                  : "No event"}
            </span>
            <span className="statusbar__item">v0.1.0</span>
          </footer>
        </div>
      </div>

      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        onNavigate={(next) => setView(next)}
        onAction={notify}
        onToggleTheme={toggleTheme}
      />

      {toast ? (
        <div
          className="toast"
          role={
            toast.startsWith("Could not") ||
            toast.startsWith("Notifications are blocked")
              ? "alert"
              : "status"
          }
        >
          <Icon name="check" size={18} />
          <span>{toast}</span>
          <button
            type="button"
            className="toast__close"
            aria-label="Dismiss message"
            onClick={() => {
              setToast(null);
              focusMainHeading();
            }}
          >
            <Icon name="x" size={16} />
          </button>
        </div>
      ) : null}

      {botDialogOpen && settings ? (
        <BotSetupDialog
          settings={settings}
          onSaved={(next) => {
            setSettings((current) =>
              current ? mergeBotSettings(current, next) : next,
            );
            setOriginalSettings((current) =>
              current ? mergeBotSettings(current, next) : next,
            );
            setToast("Discord connection saved.");
          }}
          onClose={() => {
            setBotDialogOpen(false);
            if (!demo) {
              reloadView();
            }
          }}
        />
      ) : null}

      {dialogOpen ? (
        <Dialog
          title={`Delete ${mission?.event?.name ?? "local event data"} from this computer?`}
          onClose={() => setDialogOpen(false)}
          footer={
            <>
              <Button
                variant="secondary"
                onClick={() => setDialogOpen(false)}
              >
                Cancel
              </Button>
              <Button
                variant="danger"
                icon="trash"
                disabled={confirmText !== "delete"}
                onClick={() => {
                  setDialogOpen(false);
                  setConfirmText("");
                  notify("Delete local data");
                }}
              >
                Delete local data
              </Button>
            </>
          }
        >
          <p>
            This deletes the local database, every stored track version, and
            every local backup for{" "}
            {mission?.event?.name ?? "this event"}. Other events are not touched.
            Discord messages and exported albums stay.
          </p>
          <p className="dialog__warning">
            This cannot be undone. Type <code>delete</code> to continue.
          </p>
          <Field label="Type delete to confirm" htmlFor="delete-confirm">
            <input
              id="delete-confirm"
              className="input"
              value={confirmText}
              onChange={(event) => setConfirmText(event.target.value)}
              autoComplete="off"
            />
          </Field>
        </Dialog>
      ) : null}
    </>
  );
}

function mergeBotSettings(
  current: HostSettings,
  next: HostSettings,
): HostSettings {
  return {
    ...current,
    botConfigured: next.botConfigured,
    botIdentity: next.botIdentity,
    tokenStorage: next.tokenStorage,
    keychainError: false,
    guildId: next.guildId,
    guildName: next.guildName,
    announcementChannelId: next.announcementChannelId,
    announcementChannelName: next.announcementChannelName,
    revealChannelId: next.revealChannelId,
    revealChannelName: next.revealChannelName,
  };
}

function Sidebar({
  view,
  onNavigate,
  mission,
  status,
}: {
  view: ViewKey;
  onNavigate: (view: ViewKey) => void;
  mission: MissionControl | null;
  status: LoadStatus;
}) {
  const bot = mission?.bot;
  const botTone =
    bot?.status === "Online"
      ? "ok"
      : bot?.status === "Offline"
        ? "danger"
        : "muted";

  return (
    <aside className="sidebar">
      <div className="brand">
        <span className="brand__copy">
          <strong>Monsquaz</strong>
          <span>swap host</span>
        </span>
      </div>

      <nav className="nav" aria-label="Main">
        <p className="nav__label">Event</p>
        {navItems.map((item) => (
          <button
            key={item.view}
            type="button"
            className={`nav-item ${
              view === item.view ? "nav-item--active" : ""
            }`}
            aria-current={view === item.view ? "page" : undefined}
            onClick={() => onNavigate(item.view)}
          >
            <Icon name={item.icon} size={18} />
            <span>{item.label}</span>
          </button>
        ))}

        <p className="nav__label nav__label--spaced">Event tools</p>
        {toolItems.map((item) =>
          item.view ? (
            <button
              key={item.label}
              type="button"
              className={`nav-item ${
                view === item.view ? "nav-item--active" : ""
              }`}
              aria-current={view === item.view ? "page" : undefined}
              onClick={() => onNavigate(item.view!)}
            >
              <Icon name={item.icon} size={18} />
              <span>{item.label}</span>
            </button>
          ) : (
            <button
              key={item.label}
              type="button"
              className="nav-item nav-item--disabled"
              disabled
              title="Available in a later milestone"
            >
              <Icon name={item.icon} size={18} />
              <span>{item.label}</span>
              <span className="nav-item__soon">Soon</span>
            </button>
          ),
        )}
      </nav>

      <div className="sidebar__footer">
        <div className="sidebar-bot">
          <StatusPill tone={status === "error" ? "muted" : botTone}>
            {status === "loading"
              ? "Connecting"
              : status === "error"
                ? "Status unavailable"
                : bot?.status === "Online"
                  ? "Bot online"
                  : bot?.status === "Connecting"
                    ? "Bot connecting"
                    : bot?.status === "Reconnecting"
                      ? "Bot reconnecting"
                    : bot?.status === "Offline"
                      ? "Bot offline"
                      : "No bot"}
          </StatusPill>
          <span className="sidebar-bot__name">
            {status === "loading"
              ? "Checking the local service…"
              : status === "error"
                ? "The bot state is unknown."
                : bot?.label ?? "Not configured"}
          </span>
        </div>
      </div>
    </aside>
  );
}
