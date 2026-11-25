import { invoke } from "@tauri-apps/api/core";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import type {
  AuditEntry,
  HostSettings,
  MissionControl,
  Scenario,
  Theme,
  ViewKey,
} from "../types";
import { demoAuditLog, demoMissionControl, demoSettings } from "./demo";

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
  ];
  return allowed.includes(value as Scenario) ? (value as Scenario) : "default";
}

export function initialView(): ViewKey {
  const value = new URLSearchParams(window.location.search).get("view");
  return value === "audit" || value === "settings" ? value : "mission";
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
  return invoke<HostSettings>("host_settings");
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
