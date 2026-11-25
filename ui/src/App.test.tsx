import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import App from "./App";

vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));

function setUrl(search: string) {
  window.history.pushState({}, "", `/${search}`);
}

beforeEach(() => {
  setUrl("?demo=1");
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });
});

describe("App", () => {
  it("shows the mission control next action", async () => {
    render(<App />);
    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: "Night Shift Vol. 2",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", {
        level: 2,
        name: "Three decisions need a host call",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /review decisions/i }),
    ).toBeInTheDocument();
  });

  it("shows the first-run state", async () => {
    setUrl("?demo=1&state=empty");
    render(<App />);
    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: /set up monsquaz swap/i,
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /start setup/i }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /start setup/i }));
    expect(
      await screen.findByRole("heading", { level: 1, name: "Event setup" }),
    ).toBeInTheDocument();
  });

  it("filters the audit log", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Audit log" }));
    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: "Audit log",
      }),
    ).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Search the audit log"), {
      target: { value: "S7F3A" },
    });
    expect(screen.getByText("2 of 14 entries")).toBeInTheDocument();
  });

  it("confirms destructive deletion with a typed word", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await screen.findByRole("heading", {
      level: 1,
      name: "Settings",
    });
    fireEvent.click(screen.getByRole("button", { name: "Delete event data" }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    const confirm = screen.getByRole("button", { name: "Delete local data" });
    expect(confirm).toBeDisabled();
    const input = screen.getByLabelText("Type delete to confirm");
    input.focus();
    fireEvent.change(input, { target: { value: "delete" } });
    expect(input).toHaveFocus();
    expect(confirm).toBeEnabled();
  });

  it("switches between dark and light themes", async () => {
    const { container } = render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    const app = container.querySelector(".app");
    expect(app).toHaveAttribute("data-theme", "dark");
    fireEvent.click(
      screen.getByRole("button", { name: "Switch to light theme" }),
    );
    expect(app).toHaveAttribute("data-theme", "light");
    fireEvent.click(
      screen.getByRole("button", { name: "Switch to dark theme" }),
    );
    expect(app).toHaveAttribute("data-theme", "dark");
  });

  it("selects the theme from the settings radio group", async () => {
    const { container } = render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await screen.findByRole("heading", {
      level: 1,
      name: "Settings",
    });
    fireEvent.click(screen.getByRole("radio", { name: /^Light/ }));
    expect(container.querySelector(".app")).toHaveAttribute(
      "data-theme",
      "light",
    );
  });

  it("moves focus to the page heading after navigation", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Audit log" }));
    const heading = await screen.findByRole("heading", {
      level: 1,
      name: "Audit log",
    });
    expect(heading).toHaveFocus();
    expect(document.title).toBe("Audit log, Monsquaz Swap");
  });

  it("saves settings and reports the result", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await screen.findByRole("heading", {
      level: 1,
      name: "Settings",
    });
    const retention = screen.getByLabelText("Keep files for");
    fireEvent.change(retention, { target: { value: "30" } });
    fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
    expect(
      await screen.findByText("Settings saved on this computer."),
    ).toBeInTheDocument();
  });

  it("opens the command palette with Ctrl K and navigates", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(
      screen.getByRole("dialog", { name: "Command palette" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("option", { name: /go to audit log/i }));
    expect(
      await screen.findByRole("heading", { level: 1, name: "Audit log" }),
    ).toBeInTheDocument();
  });

  it("walks the event setup and creates a preview event", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Event setup" }));
    expect(
      await screen.findByRole("heading", { level: 1, name: "Event setup" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(screen.getByRole("button", { name: "Create event" }));
    expect(
      await screen.findByText(/Preview: Night Shift Vol. 2 is ready/i),
    ).toBeInTheDocument();
  });

  it("filters the participant roster", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Participants" }));
    expect(
      await screen.findByRole("heading", { level: 1, name: "Participants" }),
    ).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Search participants"), {
      target: { value: "Noor" },
    });
    expect(screen.getByText("1 of 12")).toBeInTheDocument();
  });

  it("pauses a participant from the roster", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Participants" }));
    await screen.findByRole("heading", { level: 1, name: "Participants" });
    fireEvent.click(screen.getByRole("button", { name: "Pause Noor" }));
    expect(
      await screen.findByText(/the participant is paused/i),
    ).toBeInTheDocument();
  });

  it("hides host-only planner data in participant view", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Schedule" }));
    expect(
      await screen.findByRole("heading", { level: 1, name: "Schedule planner" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Fun score")).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: "Preview participant redaction" }),
    );
    expect(
      screen.getByText(/planner fields participants must not see/i),
    ).toBeInTheDocument();
    expect(screen.queryByText("Fun score")).not.toBeInTheDocument();
    expect(screen.queryByText("Groups")).not.toBeInTheDocument();
  });

  it("confirms before locking the schedule", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Schedule" }));
    await screen.findByRole("heading", { level: 1, name: "Schedule planner" });
    fireEvent.click(screen.getByRole("button", { name: "Lock schedule" }));
    expect(
      screen.getByRole("dialog", { name: "Lock this schedule?" }),
    ).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(
      screen.queryByRole("dialog", { name: "Command palette" }),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(
      screen.queryByRole("dialog", { name: "Lock this schedule?" }),
    ).not.toBeInTheDocument();
  });

  it("generates a preview schedule from the planner", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Schedule" }));
    await screen.findByRole("heading", { level: 1, name: "Schedule planner" });
    fireEvent.click(screen.getByRole("button", { name: "Replace draft…" }));
    fireEvent.click(screen.getByRole("button", { name: "Replace draft" }));
    expect(
      await screen.findByText(/a draft schedule is generated/i),
    ).toBeInTheDocument();
  });

  it("offers a notification test in settings", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await screen.findByRole("heading", {
      level: 1,
      name: "Settings",
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Send test notification" }),
    );
    expect(
      await screen.findByText(
        "Preview: native notifications are not sent in this build.",
      ),
    ).toBeInTheDocument();
  });

  it("shows a recoverable error state", async () => {
    setUrl("?demo=1&state=error");
    render(<App />);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("MSW-UI-001");
    expect(
      screen.getByRole("button", { name: "Retry" }),
    ).toBeInTheDocument();
  });
});
