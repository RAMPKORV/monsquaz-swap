import { test as base, expect, type Page } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

export { expect };

export type Bridge = {
  url: string;
  exportPath: string;
  process: ChildProcess;
};

type Fixtures = {
  bridge: Bridge;
};

export const test = base.extend<Fixtures>({
  bridge: async ({}, use) => {
    const bin = process.env.MSW_E2E_BRIDGE_BIN;
    const command = bin ?? "cargo";
    const args = bin
      ? []
      : [
          "run",
          "-q",
          "-p",
          "swap-e2e",
          "--features",
          "bridge",
          "--bin",
          "e2e-bridge",
        ];
    const repo = process.env.MSW_E2E_REPO_ROOT ?? resolve(process.cwd(), "..");
    const directory = mkdtempSync(join(tmpdir(), "msw-e2e-browser-"));
    const child = spawn(command, args, {
      cwd: repo,
      env: {
        ...process.env,
        MSW_E2E_PORT: "0",
        MSW_E2E_UI_DIR: "ui/dist",
        MSW_E2E_DATA_DIR: directory,
      },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stderr = "";
    child.stderr?.on("data", (chunk) => {
      stderr += String(chunk);
    });
    const url = await new Promise<string>((resolveUrl, reject) => {
      const timer = setTimeout(() => {
        reject(new Error(`the E2E bridge did not start in 180s:\n${stderr}`));
      }, 180_000);
      let stdout = "";
      child.stdout?.on("data", (chunk) => {
        stdout += String(chunk);
        for (const line of stdout.split("\n")) {
          if (line.startsWith("BRIDGE_URL=")) {
            clearTimeout(timer);
            resolveUrl(line.slice("BRIDGE_URL=".length).trim());
          }
        }
      });
      child.on("exit", (code) => {
        clearTimeout(timer);
        reject(new Error(`the E2E bridge exited with ${code}:\n${stderr}`));
      });
    });
    const bridge: Bridge = {
      url,
      exportPath: join(directory, "reveal.zip"),
      process: child,
    };
    await use(bridge);
    child.kill("SIGTERM");
    await new Promise<void>((done) => {
      const timer = setTimeout(() => {
        child.kill("SIGKILL");
        done();
      }, 5_000);
      child.on("exit", () => {
        clearTimeout(timer);
        done();
      });
    });
  },

  page: async ({ page, bridge }, use) => {
    await page.addInitScript(shim, { exportPath: bridge.exportPath });
    await use(page);
  },
});

type ShimOptions = { exportPath: string };

function shim({ exportPath }: ShimOptions) {
  const internals = {
    invoke: async (command: string, args: unknown = {}) => {
      if (command === "plugin:dialog|open") {
        const response = await fetch("/api/control/pack", { method: "POST" });
        const body = await response.json();
        if (!response.ok) {
          throw new Error(body.error ?? "the pack could not be created");
        }
        return body.value.path;
      }
      if (command === "plugin:dialog|save") {
        return exportPath;
      }
      if (command.startsWith("plugin:notification|")) {
        if (command.endsWith("is_permission_granted")) {
          return true;
        }
        if (command.endsWith("request_permission")) {
          return "granted";
        }
        return null;
      }
      if (command.startsWith("plugin:opener|")) {
        return null;
      }
      const response = await fetch("/api/invoke", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ command, args }),
      });
      const body = await response.json();
      if (!response.ok) {
        throw new Error(body.error ?? "the command failed");
      }
      return body.value;
    },
    convertFileSrc: (path: string) => `/preview/${encodeURIComponent(path)}`,
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { windowLabel: "main", label: "main" },
    },
  };
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: internals,
    configurable: true,
  });
}

export async function bridgeValue(
  bridge: Bridge,
  path: string,
  init?: RequestInit,
): Promise<unknown> {
  const response = await fetch(`${bridge.url}${path}`, init);
  const body = await response.json();
  if (!response.ok) {
    throw new Error(body.error ?? `request failed: ${response.status}`);
  }
  return body.value;
}

export async function control(
  bridge: Bridge,
  action: string,
  user: string,
): Promise<void> {
  await bridgeValue(bridge, "/api/control/participant", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ action, user }),
  });
}

type Assignment = {
  trackCode: string | null;
  round: number | null;
  status: string | null;
};

export async function assignment(
  bridge: Bridge,
  user: string,
): Promise<Assignment> {
  return (await bridgeValue(
    bridge,
    `/api/control/assignment?user=${encodeURIComponent(user)}`,
  )) as Assignment;
}

export async function waitAssignment(
  bridge: Bridge,
  user: string,
  status: string,
  timeoutMs = 20_000,
): Promise<Assignment> {
  const deadline = Date.now() + timeoutMs;
  let last: Assignment | null = null;
  while (Date.now() < deadline) {
    last = await assignment(bridge, user);
    if (last.status === status) {
      return last;
    }
    await new Promise((done) => setTimeout(done, 40));
  }
  throw new Error(
    `timed out waiting for ${user} to reach ${status}; last ${JSON.stringify(last)}`,
  );
}

export async function waitEvent(
  bridge: Bridge,
  state: string,
  timeoutMs = 20_000,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const current = (await bridgeValue(bridge, "/api/control/state")) as {
      event?: { state?: string };
    };
    if (current.event?.state === state) {
      return;
    }
    await new Promise((done) => setTimeout(done, 40));
  }
  throw new Error(`timed out waiting for the event state ${state}`);
}

export async function bridgeToken(bridge: Bridge): Promise<string> {
  const ready = (await bridgeValue(bridge, "/api/control/ready")) as {
    token: string;
  };
  return ready.token;
}

export async function expectNoHorizontalOverflow(page: Page): Promise<void> {
  const overflow = await page.evaluate(() => {
    return document.documentElement.scrollWidth - window.innerWidth;
  });
  expect(overflow).toBeLessThanOrEqual(1);
}
