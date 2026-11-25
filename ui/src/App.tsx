import { useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import "./App.css";
import {
  Button,
  Dialog,
  ErrorState,
  Field,
  StatusPill,
} from "./components";
import { Icon, type IconName } from "./icons";
import type {
  AuditEntry,
  HostSettings,
  MissionControl,
  Scenario,
  Theme,
  ViewKey,
} from "./types";
import {
  exportAuditCsv,
  HostError,
  initialDialog,
  initialScenario,
  initialView,
  isDemoMode,
  loadAuditLog,
  loadMissionControl,
  loadSettings,
  saveSettings,
  sendTestNotification,
} from "./data/client";
import {
  MissionControlSkeleton,
  MissionControlView,
} from "./views/MissionControl";
import { SettingsSkeleton, SettingsView } from "./views/Settings";
import { AuditLogSkeleton, AuditLogView } from "./views/AuditLog";

type LoadStatus = "loading" | "ready" | "error";

const navItems: Array<{ view: ViewKey; label: string; icon: IconName }> = [
  { view: "mission", label: "Mission control", icon: "compass" },
  { view: "audit", label: "Audit log", icon: "audit" },
  { view: "settings", label: "Settings", icon: "settings" },
];

const toolItems: Array<{ label: string; icon: IconName }> = [
  { label: "Event setup", icon: "plus" },
  { label: "People", icon: "users" },
  { label: "Schedule", icon: "shuffle" },
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
  const [settings, setSettings] = useState<HostSettings | null>(null);
  const [originalSettings, setOriginalSettings] =
    useState<HostSettings | null>(null);
  const [audit, setAudit] = useState<AuditEntry[] | null>(null);
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [reloadToken, setReloadToken] = useState(0);
  const [errorCode, setErrorCode] = useState("MSW-UI-001");
  const [themeChoice, setThemeChoice] = useState<HostSettings["theme"]>(() => {
    const value = new URLSearchParams(window.location.search).get("theme");
    return value === "light" || value === "dark" ? value : "dark";
  });
  const [systemTheme, setSystemTheme] = useState<Theme>("dark");
  const [toast, setToast] = useState<string | null>(null);
  const [toastPaused, setToastPaused] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(() => initialDialog() === "delete");
  const [confirmText, setConfirmText] = useState("");

  const resolvedTheme: Theme =
    themeChoice === "system" ? systemTheme : themeChoice;
  const firstView = useRef(true);

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
    ])
      .then(([nextMission, nextSettings, nextAudit]) => {
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
        setDirty(false);
        setStatus("ready");
      })
      .catch((error: unknown) => {
        if (cancelled) {
          return;
        }
        setErrorCode(error instanceof HostError ? error.code : "MSW-UI-002");
        setStatus("error");
      });
    return () => {
      cancelled = true;
    };
  }, [reloadToken, scenario]);

  useEffect(() => {
    setToastPaused(false);
  }, [toast]);

  useEffect(() => {
    if (!toast || toastPaused) {
      return;
    }
    const timer = window.setTimeout(() => setToast(null), 8000);
    return () => window.clearTimeout(timer);
  }, [toast, toastPaused]);

  useEffect(() => {
    const titles: Record<ViewKey, string> = {
      mission: "Mission control",
      settings: "Settings",
      audit: "Audit log",
    };
    document.title = `${titles[view]} · Monsquaz Swap`;
    if (firstView.current) {
      firstView.current = false;
      return;
    }
    document.querySelector<HTMLElement>("#main h1")?.focus();
  }, [view]);

  useEffect(() => {
    document.body.classList.toggle("no-scroll", dialogOpen);
    return () => document.body.classList.remove("no-scroll");
  }, [dialogOpen]);


  function notify(label: string) {
    setToast(
      demo
        ? `Preview: “${label}” does not change Discord yet.`
        : `“${label}” is not wired in this build.`,
    );
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
    if (!settings) {
      return;
    }
    setSaving(true);
    try {
      const saved = await saveSettings(settings);
      setSettings(saved);
      setOriginalSettings(saved);
      setDirty(false);
      setToast("Settings saved on this computer.");
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
  }

  function retry() {
    if (scenario === "error") {
      setStatus("error");
      return;
    }
    setReloadToken((value) => value + 1);
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
        onAction={notify}
        onNavigate={(next) => setView(next)}
      />
    ) : view === "settings" && settings ? (
      <SettingsView
        settings={settings}
        dirty={dirty}
        saving={saving}
        onSettings={(next) => {
          setSettings(next);
          setDirty(true);
        }}
        onSave={persistSettings}
        onReset={resetSettings}
        onDelete={() => {
          setConfirmText("");
          setDialogOpen(true);
        }}
        onTheme={changeTheme}
        onAction={notify}
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
        onExport={() => {
          exportAuditCsv(audit);
          setToast("Audit log exported as CSV.");
        }}
        onNavigate={() => setView("mission")}
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
        className={staticMode ? "app app--static" : "app"}
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
            <button
              type="button"
              className="event-switcher"
              onClick={() => notify("Event switcher")}
            >
              <span className="event-switcher__copy">
                <span className="event-switcher__label">{topbarLabel}</span>
                <strong>{topbarTitle}</strong>
              </span>
              <Icon name="chevron-down" size={18} />
            </button>
            <div className="topbar__actions">
              {demo ? <span className="preview-pill">Preview data</span> : null}
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
        </div>
      </div>

      {toast ? (
        <div
          className="toast"
          role={
            toast.startsWith("Could not") ||
            toast.startsWith("Notifications are blocked")
              ? "alert"
              : "status"
          }
          onMouseEnter={() => setToastPaused(true)}
          onMouseLeave={() => setToastPaused(false)}
          onFocus={() => setToastPaused(true)}
          onBlur={() => setToastPaused(false)}
        >
          <Icon name="check" size={18} />
          <span>{toast}</span>
          <button
            type="button"
            className="toast__close"
            aria-label="Dismiss message"
            onClick={() => setToast(null)}
          >
            <Icon name="x" size={16} />
          </button>
        </div>
      ) : null}

      {dialogOpen ? (
        <Dialog
          title="Delete local event data?"
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
            This removes the event database, every stored track version, and
            local backups. Discord messages and exported albums stay.
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
        <span className="brand__mark" aria-hidden="true">
          <Icon name="shuffle" size={22} />
        </span>
        <span className="brand__copy">
          <strong>Monsquaz</strong>
          <span>Swap host</span>
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
        {toolItems.map((item) => (
          <button
            key={item.label}
            type="button"
            className="nav-item nav-item--disabled"
            disabled
            title="Available in the next event-tools milestone"
          >
            <Icon name={item.icon} size={18} />
            <span>{item.label}</span>
            <span className="nav-item__soon">Soon</span>
          </button>
        ))}
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
        <span className="version">v0.1.0 · local-first</span>
      </div>
    </aside>
  );
}
