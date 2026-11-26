import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import App from "./App";
import { loadRevealPackage, resetDemoReveal } from "./data/client";

vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));

function setUrl(search: string) {
  window.history.pushState({}, "", `/${search}`);
}

beforeEach(() => {
  window.sessionStorage.clear();
  resetDemoReveal();
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
  it("loads the demo reveal package", async () => {
    setUrl("?demo=1");
    const packageData = await loadRevealPackage("default");
    expect(packageData.album).toHaveLength(6);
  });

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

  it("opens the next round from mission control", async () => {
    setUrl("?demo=1&state=round-ready");
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Open Round 4" }));
    expect(await screen.findByText(/Round 4 opened/i)).toBeInTheDocument();
  });

  it("locks the roster from mission control", async () => {
    setUrl("?demo=1&state=lock-ready");
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Lock roster" }));
    const dialog = await screen.findByRole("dialog", { name: "Lock the roster?" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Lock roster" }));
    expect(await screen.findByText(/Roster locked\. 12 active/i)).toBeInTheDocument();
  });

  it("reveals the event from mission control", async () => {
    setUrl("?demo=1&state=reveal-ready");
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Reveal event" }));
    const dialog = await screen.findByRole("dialog", {
      name: "Reveal the event?",
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Reveal event" }));
    expect(
      await screen.findByText(/2 track credits are queued/i),
    ).toBeInTheDocument();
  });

  it("shows the gateway failure in settings", async () => {
    setUrl("?demo=1&state=offline&view=settings");
    render(<App />);
    expect(
      await screen.findByText(/Discord rejected the bot token/i),
    ).toBeInTheDocument();
    expect(screen.getAllByText("Offline").length).toBeGreaterThan(0);
  });

  it("reviews an extension decision", async () => {
    setUrl("?demo=1&state=extension-ready&dialog=decision");
    render(<App />);
    const dialog = await screen.findByRole("dialog", {
      name: "Noor asked for 6 more hours",
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Approve" }));
    expect(
      await screen.findByText(/host action “approve”/i),
    ).toBeInTheDocument();
  });

  it("confirms a missed turn before it changes seen state", async () => {
    setUrl("?demo=1&state=default&dialog=decision");
    render(<App />);
    const review = await screen.findByRole("dialog", {
      name: "Track S7F3A is overdue",
    });
    fireEvent.click(
      within(review).getByRole("button", { name: "Mark turn missed" }),
    );
    const confirm = await screen.findByRole("dialog", {
      name: "Mark the turn missed?",
    });
    expect(document.activeElement).toHaveTextContent("Cancel");
    fireEvent.click(
      within(confirm).getByRole("button", { name: "Mark turn missed" }),
    );
    expect(
      await screen.findByText(/host action “missed”/i),
    ).toBeInTheDocument();
  });

  it("starts stream-safe and confirms before showing names", async () => {
    setUrl("?demo=1&view=reveal");
    render(<App />);
    await screen.findByRole("button", { name: "Stream-safe mode" });
    const toggle = screen.getByRole("button", { name: "Stream-safe mode" });
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText(/seeded by Seed owner/)).toBeInTheDocument();
    expect(screen.queryByText("Host tools")).not.toBeInTheDocument();
    expect(screen.queryByText("Noor")).not.toBeInTheDocument();
    expect(screen.queryByText("Remy")).not.toBeInTheDocument();
    expect(
      screen.getByText("The bassline answer in the second half is the best thing I heard all week."),
    ).toBeInTheDocument();
    fireEvent.click(toggle);
    const dialog = await screen.findByRole("dialog", {
      name: "Turn off stream-safe mode?",
    });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "Turn off stream-safe" }),
    );
    expect(screen.getByText(/seeded by Noor/)).toBeInTheDocument();
    expect(screen.getByText("Host tools")).toBeInTheDocument();
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(toggle);
    expect(screen.getByText(/seeded by Seed owner/)).toBeInTheDocument();
    expect(toggle).toHaveAttribute("aria-pressed", "true");
  });

  it("saves a title from the reveal show", async () => {
    setUrl("?demo=1&view=reveal&stream=0");
    render(<App />);
    await screen.findByRole("button", { name: "Stream-safe mode" });
    const field = screen.getByLabelText("Track title");
    fireEvent.change(field, { target: { value: "New Name" } });
    fireEvent.click(screen.getByRole("button", { name: "Save title" }));
    expect(await screen.findByText("Track title saved.")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { level: 2, name: "New Name" }),
    ).toBeInTheDocument();
  });

  it("redacts a comment before the reveal", async () => {
    setUrl("?demo=1&view=reveal&stream=0");
    render(<App />);
    await screen.findByRole("button", { name: "Stream-safe mode" });
    const text = "The bassline answer in the second half is the best thing I heard all week.";
    expect(screen.getByText(text)).toBeInTheDocument();
    fireEvent.click(
      screen.getAllByRole("button", { name: /Redact .* comment/ })[0],
    );
    const dialog = await screen.findByRole("dialog", {
      name: "Redact this comment?",
    });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "Redact comment" }),
    );
    expect(
      await screen.findByText("Comment redacted. It will not appear in the reveal."),
    ).toBeInTheDocument();
    expect(screen.queryByText(text)).not.toBeInTheDocument();
  });

  it("publishes the reveal from the presenter", async () => {
    setUrl("?demo=1&view=reveal");
    render(<App />);
    await screen.findByRole("button", { name: "Stream-safe mode" });
    fireEvent.click(screen.getByRole("button", { name: "Publish reveal" }));
    const dialog = await screen.findByRole("dialog", {
      name: "Publish the reveal?",
    });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "Publish reveal" }),
    );
    expect(
      await screen.findByText("Reveal published to the reveal channel."),
    ).toBeInTheDocument();
    expect(screen.getByText("Published")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Publish reveal" }),
    ).not.toBeInTheDocument();
  });

  it("keeps the publish dialog open when the reveal is blocked", async () => {
    setUrl("?demo=1&view=reveal&blocked=1");
    render(<App />);
    await screen.findByRole("button", { name: "Stream-safe mode" });
    fireEvent.click(screen.getByRole("button", { name: "Publish reveal" }));
    const dialog = await screen.findByRole("dialog", {
      name: "Publish the reveal?",
    });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "Publish reveal" }),
    );
    expect(
      await within(dialog).findByText("The reveal is still blocked."),
    ).toBeInTheDocument();
    expect(within(dialog).getByText("Track S7F3A · round 3")).toBeInTheDocument();
    expect(
      screen.queryByText("Reveal published to the reveal channel."),
    ).not.toBeInTheDocument();
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
    fireEvent.click(
      screen.getAllByRole("button", { name: "Save changes" })[0],
    );
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
      screen.getByText(/only this sample participant's assignments are visible/i),
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

  it("validates a bot token and maps channels", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await screen.findByRole("heading", { level: 1, name: "Settings" });
    fireEvent.click(screen.getByRole("button", { name: "Reconnect bot" }));
    expect(
      screen.getByRole("dialog", { name: "Connect the Discord bot" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Save connection" }),
    ).not.toBeInTheDocument();

    const token = screen.getByLabelText("Bot token");
    expect(token).toHaveAttribute("type", "password");
    fireEvent.change(token, {
      target: {
        value:
          "aaaaaaaaaaaaaaaaaaaaaaaa.bbbbbb.ccccccccccccccccccccccccc",
      },
    });
    fireEvent.click(screen.getByRole("button", { name: "Validate token" }));
    expect(
      await screen.findByText(/token accepted/i),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Server")).toBeInTheDocument();
    expect(screen.getByLabelText("Announcement channel")).toHaveValue(
      "8142276040001",
    );
    expect(screen.getByLabelText("Reveal channel")).toHaveValue(
      "8142276040002",
    );
  });

  it("rejects a malformed token before any Discord request", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await screen.findByRole("heading", { level: 1, name: "Settings" });
    fireEvent.click(screen.getByRole("button", { name: "Reconnect bot" }));
    fireEvent.change(screen.getByLabelText("Bot token"), {
      target: { value: "short" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Validate token" }));
    expect(
      await screen.findByText(/discord rejected this token/i),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Save connection" }),
    ).toBeDisabled();
  });

  it("invalidates the validated identity when the token changes", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await screen.findByRole("heading", { level: 1, name: "Settings" });
    fireEvent.click(screen.getByRole("button", { name: "Reconnect bot" }));
    const dialog = screen.getByRole("dialog", { name: "Connect the Discord bot" });
    const token = screen.getByLabelText("Bot token");
    fireEvent.change(token, {
      target: {
        value:
          "aaaaaaaaaaaaaaaaaaaaaaaa.bbbbbb.ccccccccccccccccccccccccc",
      },
    });
    fireEvent.click(screen.getByRole("button", { name: "Validate token" }));
    await screen.findByText(/token accepted/i);
    fireEvent.change(token, {
      target: {
        value:
          "dddddddddddddddddddddddd.eeeeee.fffffffffffffffffffffffffff",
      },
    });
    expect(
      within(dialog).queryByText(/Monsquaz Bot \(1184\)/),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Save connection" }),
    ).toBeDisabled();
  });

  it("saves the bot connection and clears the token from the form", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await screen.findByRole("heading", { level: 1, name: "Settings" });
    fireEvent.click(screen.getByRole("button", { name: "Reconnect bot" }));
    const token = screen.getByLabelText("Bot token");
    fireEvent.change(token, {
      target: {
        value:
          "aaaaaaaaaaaaaaaaaaaaaaaa.bbbbbb.ccccccccccccccccccccccccc",
      },
    });
    fireEvent.click(screen.getByRole("button", { name: "Validate token" }));
    await screen.findByText(/token accepted/i);
    fireEvent.click(screen.getByRole("button", { name: "Save connection" }));
    expect(
      await screen.findByText(/connection saved\. send a test direct message/i),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Bot token")).toHaveValue("");
    expect(
      screen.queryByDisplayValue(
        "aaaaaaaaaaaaaaaaaaaaaaaa.bbbbbb.ccccccccccccccccccccccccc",
      ),
    ).not.toBeInTheDocument();
  });

  it("sends a test direct message after saving", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await screen.findByRole("heading", { level: 1, name: "Settings" });
    fireEvent.click(screen.getByRole("button", { name: "Reconnect bot" }));
    fireEvent.change(screen.getByLabelText("Bot token"), {
      target: {
        value:
          "aaaaaaaaaaaaaaaaaaaaaaaa.bbbbbb.ccccccccccccccccccccccccc",
      },
    });
    fireEvent.click(screen.getByRole("button", { name: "Validate token" }));
    await screen.findByText(/token accepted/i);
    fireEvent.click(screen.getByRole("button", { name: "Save connection" }));
    await screen.findByText(/connection saved\. send a test direct message/i);
    fireEvent.change(screen.getByLabelText("Send a test direct message"), {
      target: { value: "11840000000000000" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send test" }));
    expect(
      await screen.findByText(/test preview: this build does not send a message/i),
    ).toBeInTheDocument();
  });

  it("offers a session-only fallback when the keychain is unavailable", async () => {
    setUrl("?demo=1&view=settings&dialog=bot&botKeychain=down");
    render(<App />);
    expect(
      await screen.findByRole("dialog", { name: "Connect the Discord bot" }),
    ).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Bot token"), {
      target: {
        value:
          "aaaaaaaaaaaaaaaaaaaaaaaa.bbbbbb.ccccccccccccccccccccccccc",
      },
    });
    fireEvent.click(screen.getByRole("button", { name: "Validate token" }));
    await screen.findByText(/token accepted/i);
    fireEvent.click(screen.getByRole("button", { name: "Save connection" }));
    expect(
      await screen.findByText(/does not provide a keychain/i),
    ).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("checkbox", { name: /use this session only/i }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Save for this session" }),
    );
    expect(
      await screen.findByText(/connection saved for this session only/i),
    ).toBeInTheDocument();
  });

  it("removes the stored connection after confirmation", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await screen.findByRole("heading", { level: 1, name: "Settings" });
    expect(screen.getByText("Connected")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Reconnect bot" }));
    fireEvent.click(screen.getByRole("button", { name: "Remove connection" }));
    fireEvent.click(
      screen.getByRole("button", { name: "Confirm remove connection" }),
    );
    expect(
      await screen.findByText(/connection removed/i),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(
      await screen.findByRole("button", { name: "Add bot token" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Not configured")).toBeInTheDocument();
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
